import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  Apple,
  Car,
  Clock,
  Flag,
  Hand,
  Heart,
  Lamp,
  Leaf,
  SmilePlus,
  Trophy,
  X,
} from "lucide-react";
import {
  type KeyboardEvent,
  type ReactNode,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";

import { ipc } from "../lib/ipc";
import { applyAppearance } from "../lib/preferences";
import type { EmojiGroup, EmojiRow } from "../lib/types";
import { IconButton, Kbd, Notice, SearchField } from "../ui";
import "./EmojiPicker.css";

const PAGE = 120;
const WHOLE_GROUP = 2_000;
const COLUMNS = 9;

const GROUP_ICONS: Record<string, ReactNode> = {
  smileys: <SmilePlus />,
  people: <Hand />,
  animals: <Leaf />,
  food: <Apple />,
  travel: <Car />,
  activities: <Trophy />,
  objects: <Lamp />,
  symbols: <Heart />,
  flags: <Flag />,
};

type Message = { tone: "neutral" | "critical"; text: string };

export function EmojiPicker() {
  const [groups, setGroups] = useState<EmojiGroup[]>([]);
  const [emoji, setEmoji] = useState<EmojiRow[] | null>(null);
  const [recents, setRecents] = useState<EmojiRow[]>([]);
  const [query, setQuery] = useState("");
  const [group, setGroup] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<Message | null>(null);
  const [active, setActive] = useState(0);
  const search = useRef<HTMLInputElement>(null);
  const grid = useRef<HTMLElement>(null);

  const close = useCallback(() => {
    void getCurrentWindow()
      .close()
      .catch((reason) => setMessage({ tone: "critical", text: String(reason) }));
  }, []);

  useEffect(() => {
    void ipc
      .emojiGroups()
      .then(setGroups)
      .catch((reason) => setMessage({ tone: "critical", text: String(reason) }));
    void ipc
      .emojiRecents(18)
      .then(setRecents)
      .catch(() => {});
    void ipc
      .settingsAll()
      .then((settings) => applyAppearance(settings.appearance))
      .catch(() => {});

    const onKeyDown = (event: globalThis.KeyboardEvent) => {
      if (event.key === "Escape") {
        close();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [close]);

  const activeGroup = query.trim() ? null : group;

  useEffect(() => {
    let live = true;
    void ipc
      .emojiSearch(query, activeGroup, activeGroup === null ? PAGE : WHOLE_GROUP)
      .then((rows) => {
        if (live) {
          setEmoji(rows);
          setActive(0);
          setMessage(null);
        }
      })
      .catch((reason) => {
        if (live) {
          setMessage({ tone: "critical", text: String(reason) });
        }
      });
    return () => {
      live = false;
    };
  }, [activeGroup, query]);

  const visibleRecents = useMemo(
    () => (!query.trim() && activeGroup === null ? recents : []),
    [activeGroup, query, recents],
  );
  const cells = useMemo(() => [...visibleRecents, ...(emoji ?? [])], [emoji, visibleRecents]);
  const current = cells[Math.min(active, cells.length - 1)];

  const pick = async (row: EmojiRow) => {
    if (busy) {
      return;
    }
    setBusy(true);
    try {
      const settings = await ipc.settingsAll().catch(() => ({}) as Record<string, unknown>);
      const insert = settings.emojiInsertInPlace !== false;
      const result = await ipc.emojiCopy(row.character, insert);
      setMessage(
        insert && !result.inserted
          ? {
              tone: "neutral",
              text: "Copied. Focus your text field, then choose another emoji to insert.",
            }
          : null,
      );
      void ipc
        .emojiRecents(18)
        .then(setRecents)
        .catch(() => {});
    } catch (reason) {
      setMessage({ tone: "critical", text: String(reason) });
    } finally {
      setBusy(false);
    }
  };

  const focusCell = (index: number) => {
    const clamped = Math.max(0, Math.min(cells.length - 1, index));
    setActive(clamped);
    grid.current?.querySelector<HTMLButtonElement>(`[data-index="${clamped}"]`)?.focus();
  };

  const onSearchKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Escape") {
      close();
    } else if (event.key === "ArrowDown" && cells.length > 0) {
      event.preventDefault();
      focusCell(0);
    } else if (event.key === "Enter" && cells[0]) {
      event.preventDefault();
      void pick(cells[0]);
    }
  };

  const onGridKeyDown = (event: KeyboardEvent<HTMLElement>) => {
    const moves: Record<string, number> = {
      ArrowRight: 1,
      ArrowLeft: -1,
      ArrowDown: COLUMNS,
      ArrowUp: -COLUMNS,
    };
    if (event.key in moves) {
      event.preventDefault();
      const next = active + moves[event.key];
      if (next < 0 && event.key === "ArrowUp") {
        search.current?.focus();
      } else {
        focusCell(next);
      }
    } else if (event.key === "Home") {
      event.preventDefault();
      focusCell(0);
    } else if (event.key === "End") {
      event.preventDefault();
      focusCell(cells.length - 1);
    } else if (
      event.key.length === 1 &&
      !event.ctrlKey &&
      !event.metaKey &&
      !event.altKey &&
      event.key !== " "
    ) {
      search.current?.focus();
    }
  };

  const selectGroup = (index: number | null) => {
    setGroup(index);
    setQuery("");
  };

  const heading = query.trim()
    ? "Results"
    : activeGroup === null
      ? "All emoji"
      : groups[activeGroup]?.name;

  return (
    <main className="emoji-picker" aria-label="Emoji picker">
      <header className="emoji-picker-head" data-tauri-drag-region>
        <SearchField
          ref={search}
          className="emoji-picker-search"
          value={query}
          onChange={setQuery}
          onKeyDown={onSearchKeyDown}
          placeholder="Search emoji"
          aria-label="Search emoji"
          autoFocus
        />
        <IconButton label="Close" tooltip="Close the picker" icon={<X />} onClick={close} />
      </header>

      <nav className="emoji-picker-groups" aria-label="Emoji categories" data-tauri-drag-region>
        <IconButton
          size="small"
          label="Recent and all"
          tooltip="Show recent and all emoji"
          icon={<Clock />}
          pressed={activeGroup === null && !query.trim()}
          onClick={() => selectGroup(null)}
        />
        {groups.map((entry, index) => (
          <IconButton
            key={entry.id}
            size="small"
            label={entry.name}
            tooltip={`Show ${entry.name}`}
            icon={GROUP_ICONS[entry.id] ?? <SmilePlus />}
            pressed={activeGroup === index}
            onClick={() => selectGroup(index)}
          />
        ))}
      </nav>

      {message && (
        <div className="emoji-picker-message">
          <Notice tone={message.tone} compact>
            {message.text}
          </Notice>
        </div>
      )}

      <section
        className="emoji-picker-scroll"
        aria-label="Emoji"
        ref={grid}
        onKeyDown={onGridKeyDown}
      >
        {visibleRecents.length > 0 && (
          <section aria-label="Recently used">
            <h2 className="emoji-picker-label">Recently used</h2>
            <div className="emoji-picker-grid">
              {visibleRecents.map((row, index) => (
                <PickerCell
                  key={`recent-${row.character}`}
                  row={row}
                  index={index}
                  active={index === active}
                  disabled={busy}
                  onPick={pick}
                  onFocus={setActive}
                />
              ))}
            </div>
          </section>
        )}
        {emoji !== null && (
          <section aria-label={heading}>
            <h2 className="emoji-picker-label">{heading}</h2>
            {emoji.length > 0 ? (
              <div className="emoji-picker-grid">
                {emoji.map((row, offset) => {
                  const index = visibleRecents.length + offset;
                  return (
                    <PickerCell
                      key={row.character}
                      row={row}
                      index={index}
                      active={index === active}
                      disabled={busy}
                      onPick={pick}
                      onFocus={setActive}
                    />
                  );
                })}
              </div>
            ) : (
              <p className="emoji-picker-empty">
                {query.trim() ? `No emoji match “${query.trim()}”` : "No emoji loaded"}
              </p>
            )}
          </section>
        )}
      </section>

      <footer className="emoji-picker-foot">
        <span className="emoji-picker-current" aria-live="polite">
          {current ? (
            <>
              <span className="emoji-picker-current-glyph" aria-hidden="true">
                {current.character}
              </span>
              <span className="emoji-picker-current-name">{current.name}</span>
            </>
          ) : null}
        </span>
        <span className="emoji-picker-keys">
          <Kbd>↵</Kbd> insert
          <Kbd>Esc</Kbd> close
        </span>
      </footer>
    </main>
  );
}

function PickerCell({
  row,
  index,
  active,
  disabled,
  onPick,
  onFocus,
}: {
  row: EmojiRow;
  index: number;
  active: boolean;
  disabled: boolean;
  onPick: (row: EmojiRow) => void;
  onFocus: (index: number) => void;
}) {
  return (
    <button
      type="button"
      className="emoji-picker-cell"
      data-index={index}
      tabIndex={active ? 0 : -1}
      aria-label={row.name}
      aria-disabled={disabled || undefined}
      onFocus={() => onFocus(index)}
      onMouseEnter={() => onFocus(index)}
      onClick={() => onPick(row)}
    >
      {row.character}
    </button>
  );
}
