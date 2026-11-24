import { useCallback, useEffect, useMemo, useState } from "react";
import { SearchX, Smile } from "lucide-react";

import { ipc } from "../lib/ipc";
import type { EmojiGroup, EmojiRow } from "../lib/types";
import {
  Button,
  Card,
  EmptyState,
  Kbd,
  Notice,
  Page,
  PageHeader,
  SearchField,
  Section,
  Select,
  Skeleton,
  Tabs,
  Toolbar,
  Tooltip,
} from "../ui";
import "./EmojiPage.css";

const SKIN_TONES: { key: string; title: string; sample: string }[] = [
  { key: "standard", title: "Default", sample: "✋" },
  { key: "light", title: "Light", sample: "✋🏻" },
  { key: "mediumLight", title: "Medium light", sample: "✋🏼" },
  { key: "medium", title: "Medium", sample: "✋🏽" },
  { key: "mediumDark", title: "Medium dark", sample: "✋🏾" },
  { key: "dark", title: "Dark", sample: "✋🏿" },
];

const SHORT_GROUP_NAMES: Record<string, string> = {
  smileys: "Smileys",
  people: "People",
  animals: "Nature",
  food: "Food",
  travel: "Travel",
  activities: "Activities",
  objects: "Objects",
  symbols: "Symbols",
  flags: "Flags",
};

const PAGE = 240;
const WHOLE_GROUP = 2_000;
const ALL = "all";

export function EmojiPage() {
  const [groups, setGroups] = useState<EmojiGroup[]>([]);
  const [emoji, setEmoji] = useState<EmojiRow[] | null>(null);
  const [recents, setRecents] = useState<EmojiRow[]>([]);
  const [query, setQuery] = useState("");
  const [group, setGroup] = useState<number | null>(null);
  const [settings, setSettings] = useState<Record<string, unknown>>({});
  const [copied, setCopied] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);

  const tone = typeof settings.emojiSkinTone === "string" ? settings.emojiSkinTone : "standard";

  const loadRecents = useCallback(async () => {
    try {
      setRecents(await ipc.emojiRecents(24));
    } catch {
      setRecents([]);
    }
  }, []);

  useEffect(() => {
    void ipc.emojiGroups().then(setGroups).catch((reason) => setError(String(reason)));
    void ipc.settingsAll().then(setSettings).catch(() => {});
  }, [attempt]);

  useEffect(() => {
    void loadRecents();
  }, [loadRecents, tone]);

  const activeGroup = query.trim() ? null : group;

  useEffect(() => {
    let live = true;
    void ipc
      .emojiSearch(query, activeGroup, activeGroup === null ? PAGE : WHOLE_GROUP)
      .then((rows) => {
        if (live) {
          setEmoji(rows);
          setError(null);
        }
      })
      .catch((reason) => {
        if (live) setError(String(reason));
      });
    return () => {
      live = false;
    };
  }, [query, activeGroup, tone, attempt]);

  const setTone = async (value: string) => {
    try {
      await ipc.settingsSet("emojiSkinTone", value);
      setSettings((current) => ({ ...current, emojiSkinTone: value }));
      setError(null);
    } catch (reason) {
      setError(String(reason));
    }
  };

  const pick = async (row: EmojiRow) => {
    try {
      const result = await ipc.emojiCopy(row.character, false);
      setCopied(result.character);
      setError(null);
      window.setTimeout(() => setCopied((current) => (current === result.character ? null : current)), 1200);
      await loadRecents();
    } catch (reason) {
      setError(String(reason));
    }
  };

  const groupName = useMemo(() => new Map(groups.map((entry, index) => [index, entry.name])), [groups]);

  const tabs = useMemo(
    () => [
      { id: ALL, label: "All" },
      ...groups.map((entry, index) => ({ id: String(index), label: SHORT_GROUP_NAMES[entry.id] ?? entry.name })),
    ],
    [groups],
  );

  const selectGroup = (id: string) => {
    setGroup(id === ALL ? null : Number(id));
    setQuery("");
  };

  const searching = query.trim().length > 0;
  const resultsTitle = searching ? "Results" : activeGroup === null ? "All emoji" : groupName.get(activeGroup) ?? "Emoji";
  const resultsNote =
    emoji === null ? undefined : emoji.length === PAGE && activeGroup === null ? `Showing the first ${PAGE}` : `${emoji.length} emoji`;

  return (
    <Page
      header={
        <PageHeader
          title="Emoji"
          subtitle={
            <>
              Click to copy. <Kbd>Ctrl Alt E</Kbd> opens the picker.
            </>
          }
          accessory={
            groups.length > 0 && (
              <Tabs items={tabs} value={activeGroup === null ? ALL : String(activeGroup)} onChange={selectGroup} label="Emoji categories" />
            )
          }
        />
      }
    >
      <Toolbar
        leading={<SearchField className="emoji-search" value={query} onChange={setQuery} placeholder="Search by name or keyword" autoFocus />}
        trailing={
          <Select variant="chip" value={tone} onChange={(event) => void setTone(event.target.value)} aria-label="Skin tone">
            {SKIN_TONES.map((item) => (
              <option key={item.key} value={item.key}>
                {item.sample} {item.title}
              </option>
            ))}
          </Select>
        }
      />

      {error && (
        <Notice tone="critical" title="The emoji catalogue did not load" actions={<Button size="small" onClick={() => setAttempt((value) => value + 1)}>Retry</Button>}>
          {error}
        </Notice>
      )}

      {recents.length > 0 && !searching && activeGroup === null && (
        <Section title="Most used" description="Yours, kept on this computer">
          <Card padding="compact">
            <EmojiGrid rows={recents} copied={copied} groupName={groupName} onPick={pick} keyPrefix="recent-" />
          </Card>
        </Section>
      )}

      {!(emoji === null && error) && (
        <Section title={resultsTitle} description={resultsNote}>
          {emoji === null ? (
            <Card padding="compact" aria-label="Loading emoji">
              <div className="emoji-grid">
                {Array.from({ length: 48 }, (_, index) => (
                  <Skeleton key={index} width={40} height={40} radius={8} />
                ))}
              </div>
            </Card>
          ) : emoji.length === 0 ? (
            <Card>
              {searching ? (
                <EmptyState
                  size="compact"
                  icon={<SearchX />}
                  title={`Nothing matches “${query.trim()}”`}
                  action={<Button onClick={() => setQuery("")}>Clear search</Button>}
                >
                  Try a shorter word, or one of the keywords an emoji is known by.
                </EmptyState>
              ) : (
                <EmptyState size="compact" icon={<Smile />} title="No emoji loaded">
                  The catalogue ships with Veronica and is read at launch. Run vr emoji search smile to see what it reports.
                </EmptyState>
      )}
          </Card>
        ) : (
          <Card padding="compact">
            <EmojiGrid rows={emoji} copied={copied} groupName={groupName} onPick={pick} keyPrefix="" />
          </Card>
        )}
      </Section>
      )}

      <p className="ui-visually-hidden" role="status">
        {copied ? `Copied ${copied}` : ""}
      </p>
    </Page>
  );
}

function EmojiGrid({
  rows,
  copied,
  groupName,
  onPick,
  keyPrefix,
}: {
  rows: EmojiRow[];
  copied: string | null;
  groupName: Map<number, string>;
  onPick: (row: EmojiRow) => void;
  keyPrefix: string;
}) {
  return (
    <div className="emoji-grid">
      {rows.map((row) => (
        <Tooltip key={`${keyPrefix}${row.character}`} content={[row.name, groupName.get(row.groupIndex)].filter(Boolean).join(" · ")}>
          <button
            type="button"
            className="emoji-cell"
            data-copied={copied === row.character || undefined}
            aria-label={row.name}
            onClick={() => onPick(row)}
          >
            {row.character}
          </button>
        </Tooltip>
      ))}
    </div>
  );
}
