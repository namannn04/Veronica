import { useCallback, useEffect, useMemo, useState } from "react";

import { ipc } from "../lib/ipc";
import type { EmojiGroup, EmojiRow } from "../lib/types";

/**
 * The emoji picker.
 *
 * The catalogue, the ranking and the recents ledger are Edith's, so a search
 * here returns what Edith's picker would. What differs is the last step: the
 * clipboard is the part that always works, and typing the emoji into the app
 * you were in goes through Veronica's shell extension, because only the
 * compositor may synthesise input into a window it does not own.
 */

/** Edith's tone list, in its order. */
const TONES: { key: string; title: string; sample: string }[] = [
  { key: "standard", title: "Default", sample: "✋" },
  { key: "light", title: "Light", sample: "✋🏻" },
  { key: "mediumLight", title: "Medium light", sample: "✋🏼" },
  { key: "medium", title: "Medium", sample: "✋🏽" },
  { key: "mediumDark", title: "Medium dark", sample: "✋🏾" },
  { key: "dark", title: "Dark", sample: "✋🏿" },
];

/** Enough to fill the grid several times over without rendering two thousand cells. */
const PAGE = 240;

export function EmojiPage() {
  const [groups, setGroups] = useState<EmojiGroup[]>([]);
  const [emoji, setEmoji] = useState<EmojiRow[]>([]);
  const [recents, setRecents] = useState<EmojiRow[]>([]);
  const [query, setQuery] = useState("");
  const [group, setGroup] = useState<number | null>(null);
  const [settings, setSettings] = useState<Record<string, unknown>>({});
  const [copied, setCopied] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const tone = typeof settings.emojiSkinTone === "string" ? settings.emojiSkinTone : "standard";
  // Inserting needs the extension; without it the picker still copies, so the
  // switch defaults off rather than silently failing on every pick.
  const insert = settings.emojiInsertInPlace === true;

  const loadRecents = useCallback(async () => {
    try { setRecents(await ipc.emojiRecents(24)); } catch { /* an empty ledger is not an error */ }
  }, []);

  useEffect(() => {
    void ipc.emojiGroups().then(setGroups).catch((reason) => setError(String(reason)));
    void ipc.settingsAll().then(setSettings).catch(() => {});
    void loadRecents();
  }, [loadRecents]);

  // Searching narrows across every group, so an active tab is cleared the
  // moment there is a query — otherwise a hit outside the tab looks like a
  // catalogue with nothing in it.
  const activeGroup = query.trim() ? null : group;

  useEffect(() => {
    let live = true;
    void ipc
      .emojiSearch(query, activeGroup, PAGE)
      .then((rows) => { if (live) { setEmoji(rows); setError(null); } })
      .catch((reason) => { if (live) setError(String(reason)); });
    return () => { live = false; };
  }, [query, activeGroup, tone]);

  const set = async (key: string, value: unknown) => {
    try {
      await ipc.settingsSet(key, value);
      setSettings((current) => ({ ...current, [key]: value }));
      setError(null);
    } catch (reason) { setError(String(reason)); }
  };

  const pick = async (row: EmojiRow) => {
    try {
      const result = await ipc.emojiCopy(row.character, insert);
      setCopied(result.character);
      setError(null);
      window.setTimeout(() => setCopied((current) => (current === result.character ? null : current)), 1200);
      await loadRecents();
    } catch (reason) { setError(String(reason)); }
  };

  const groupName = useMemo(
    () => new Map(groups.map((entry, index) => [index, entry.name])),
    [groups]
  );

  return <div className="page emoji-page">
    <div className="page-head edith-head">
      <div><h1>Emoji</h1><div className="page-sub">Every emoji on Ctrl+Alt+E, copied on click</div></div>
      <div className="head-actions">
        <select value={tone} onChange={(event) => void set("emojiSkinTone", event.target.value)} aria-label="Skin tone">
          {TONES.map((item) => <option key={item.key} value={item.key}>{item.sample}  {item.title}</option>)}
        </select>
        <label className="emoji-insert">
          <input type="checkbox" checked={insert} onChange={(event) => void set("emojiInsertInPlace", event.target.checked)} />
          Type into the app I was in
        </label>
      </div>
    </div>

    {error && <div className="banner error">{error}</div>}

    <div className="emoji-toolbar">
      <input className="search" value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Search by name or keyword" aria-label="Search emoji" autoFocus />
      <div className="emoji-tabs">
        <button className={activeGroup === null ? "active" : ""} onClick={() => { setGroup(null); setQuery(""); }}>All</button>
        {groups.map((entry, index) => (
          <button key={entry.id} className={activeGroup === index ? "active" : ""} onClick={() => { setGroup(index); setQuery(""); }}>{entry.name}</button>
        ))}
      </div>
    </div>

    {recents.length > 0 && !query.trim() && <section className="card">
      <div className="card-head"><h2>Most used</h2><span className="card-note">yours, kept locally</span></div>
      <div className="emoji-grid">{recents.map((row) => <EmojiCell key={`recent-${row.character}`} row={row} copied={copied} groupName={groupName} onPick={pick} />)}</div>
    </section>}

    <section className="card">
      <div className="card-head"><h2>{query.trim() ? "Results" : activeGroup === null ? "Everything" : groupName.get(activeGroup)}</h2><span className="card-note">{emoji.length === PAGE ? `first ${PAGE}` : `${emoji.length}`}</span></div>
      {emoji.length === 0
        ? query.trim()
          ? <div className="empty"><h3>Nothing matches “{query}”</h3><p>Try a shorter word, or one of the keywords an emoji is known by.</p></div>
          // An empty grid with an empty search box is the catalogue failing to
          // load, not a search that found nothing.
          : <div className="empty"><h3>No emoji loaded</h3><p>The catalogue ships with Veronica and is read at launch. Run `vr emoji search smile` to see what the catalogue reports.</p></div>
        : <div className="emoji-grid">{emoji.map((row) => <EmojiCell key={row.character} row={row} copied={copied} groupName={groupName} onPick={pick} />)}</div>}
    </section>
  </div>;
}

function EmojiCell({ row, copied, groupName, onPick }: {
  row: EmojiRow;
  copied: string | null;
  groupName: Map<number, string>;
  onPick: (row: EmojiRow) => void;
}) {
  return <button
    className={`emoji-cell ${copied === row.character ? "copied" : ""}`}
    title={`${row.name} · ${groupName.get(row.groupIndex) ?? ""}`}
    aria-label={row.name}
    onClick={() => onPick(row)}
  >{row.character}</button>;
}
