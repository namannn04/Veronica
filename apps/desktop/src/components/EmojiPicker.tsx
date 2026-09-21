import { getCurrentWindow } from "@tauri-apps/api/window";
import { useCallback, useEffect, useMemo, useState } from "react";

import { ipc } from "../lib/ipc";
import { applyAppearance } from "../lib/preferences";
import type { EmojiGroup, EmojiRow } from "../lib/types";

const PAGE = 120;

export function EmojiPicker() {
  const [groups, setGroups] = useState<EmojiGroup[]>([]);
  const [emoji, setEmoji] = useState<EmojiRow[]>([]);
  const [recents, setRecents] = useState<EmojiRow[]>([]);
  const [query, setQuery] = useState("");
  const [group, setGroup] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const close = useCallback(() => {
    void getCurrentWindow().close();
  }, []);

  useEffect(() => {
    void ipc.emojiGroups().then(setGroups).catch((reason) => setError(String(reason)));
    void ipc.emojiRecents(18).then(setRecents).catch(() => {});
    void ipc.settingsAll().then((settings) => applyAppearance(settings.appearance)).catch(() => {});

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") close();
    };
    window.addEventListener("keydown", onKeyDown);

    // Ignore the focus transition while the webview is first being mapped;
    // afterwards clicking anywhere else dismisses it like a real popover.
    const blurTimer = window.setTimeout(() => window.addEventListener("blur", close), 300);
    return () => {
      window.clearTimeout(blurTimer);
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("blur", close);
    };
  }, [close]);

  const activeGroup = query.trim() ? null : group;
  useEffect(() => {
    let live = true;
    void ipc.emojiSearch(query, activeGroup, PAGE)
      .then((rows) => {
        if (live) {
          setEmoji(rows);
          setError(null);
        }
      })
      .catch((reason) => { if (live) setError(String(reason)); });
    return () => { live = false; };
  }, [activeGroup, query]);

  const visibleRecents = useMemo(
    () => (!query.trim() && activeGroup === null ? recents : []),
    [activeGroup, query, recents],
  );

  const pick = async (row: EmojiRow) => {
    if (busy) return;
    setBusy(true);
    try {
      // The compact surface is an insertion tool, not the settings page: a
      // click always returns the character to the app that had focus.
      await ipc.emojiCopy(row.character, true);
      close();
    } catch (reason) {
      setError(String(reason));
      setBusy(false);
    }
  };

  return <main className="emoji-popover" aria-label="Emoji picker">
    <header className="emoji-popover-head" data-tauri-drag-region>
      <div data-tauri-drag-region>
        <strong data-tauri-drag-region>Emoji</strong>
        <span data-tauri-drag-region>Choose to insert</span>
      </div>
      <button className="emoji-popover-close" onClick={close} aria-label="Close">×</button>
    </header>

    <div className="emoji-popover-search">
      <span aria-hidden="true">⌕</span>
      <input
        value={query}
        onChange={(event) => setQuery(event.target.value)}
        placeholder="Search emoji"
        aria-label="Search emoji"
        autoFocus
      />
      <kbd>Esc</kbd>
    </div>

    <div className="emoji-popover-tabs" aria-label="Emoji categories">
      <button className={activeGroup === null ? "active" : ""} onClick={() => { setGroup(null); setQuery(""); }}>Recent</button>
      {groups.map((entry, index) => (
        <button key={entry.id} className={activeGroup === index ? "active" : ""} onClick={() => { setGroup(index); setQuery(""); }}>{entry.name}</button>
      ))}
    </div>

    {error && <div className="emoji-popover-error">{error}</div>}

    <div className="emoji-popover-scroll">
      {visibleRecents.length > 0 && <section>
        <h2>Recently used</h2>
        <div className="emoji-popover-grid">
          {visibleRecents.map((row) => <PickerCell key={`recent-${row.character}`} row={row} disabled={busy} onPick={pick} />)}
        </div>
      </section>}
      <section>
        <h2>{query.trim() ? "Results" : activeGroup === null ? "All emoji" : groups[activeGroup]?.name}</h2>
        {emoji.length > 0
          ? <div className="emoji-popover-grid">
              {emoji.map((row) => <PickerCell key={row.character} row={row} disabled={busy} onPick={pick} />)}
            </div>
          : <p className="emoji-popover-empty">No emoji found</p>}
      </section>
    </div>

    <footer>Click an emoji to insert it into your previous app</footer>
  </main>;
}

function PickerCell({ row, disabled, onPick }: {
  row: EmojiRow;
  disabled: boolean;
  onPick: (row: EmojiRow) => void;
}) {
  return <button
    className="emoji-popover-cell"
    title={row.name}
    aria-label={row.name}
    disabled={disabled}
    onClick={() => onPick(row)}
  >{row.character}</button>;
}
