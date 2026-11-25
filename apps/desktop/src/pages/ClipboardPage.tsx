import {
  Button,
  ConfirmDialog,
  EmptyState,
  IconButton,
  List,
  ListRow,
  ListSkeleton,
  Notice,
  Page,
  PageHeader,
  SearchField,
  SkeletonText,
  Toolbar,
} from "@veronica/ui";
import {
  AtSign,
  Check,
  ClipboardList,
  Copy,
  Link2,
  TextAlignStart,
  Trash,
  Type,
  X,
} from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { bytes, timeAgo } from "../lib/format";
import { ipc } from "../lib/ipc";
import { startVisiblePolling } from "../lib/polling";
import type { ClipRow } from "../lib/types";
import "./ClipboardPage.css";

const TICK_MS = 3000;

type ClearRequest = { total: number; matching: number; query: string };

export function ClipboardPage() {
  const [rows, setRows] = useState<ClipRow[]>([]);
  const [query, setQuery] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState<number | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [clearRequest, setClearRequest] = useState<ClearRequest | null>(null);
  const [clearing, setClearing] = useState(false);
  const search = useRef<HTMLInputElement>(null);

  const load = useCallback(async () => {
    try {
      setRows(await ipc.clipboardList(query));
      setError(null);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setLoaded(true);
    }
  }, [query]);

  useEffect(() => startVisiblePolling(load, TICK_MS), [load]);

  const copy = async (row: ClipRow) => {
    try {
      await navigator.clipboard.writeText(row.text);
      setCopied(row.id);
      setTimeout(() => setCopied((current) => (current === row.id ? null : current)), 1200);
    } catch (reason) {
      setError(`Cannot write to the clipboard: ${reason}`);
    }
  };

  const remove = async (id: number) => {
    try {
      await ipc.clipboardRemove(id);
      await load();
    } catch (reason) {
      setError(String(reason));
    }
  };

  const askToClear = async () => {
    try {
      const total = query ? (await ipc.clipboardList("")).length : rows.length;
      if (total === 0) {
        return;
      }
      setClearRequest({ total, matching: rows.length, query });
    } catch (reason) {
      setError(String(reason));
    }
  };

  const clear = async () => {
    setClearing(true);
    try {
      await ipc.clipboardClear();
      await load();
    } catch (reason) {
      setError(String(reason));
    } finally {
      setClearing(false);
      setClearRequest(null);
    }
  };

  const count = `${rows.length} ${rows.length === 1 ? "entry" : "entries"}`;

  return (
    <Page
      header={
        <PageHeader
          title="Clipboard"
          subtitle="Everything you have copied, kept on this computer"
          actions={
            <Button
              variant="destructive-plain"
              icon={<Trash />}
              onClick={() => void askToClear()}
              disabled={!query && rows.length === 0}
            >
              Clear all
            </Button>
          }
        />
      }
    >
      <Toolbar
        leading={
          <SearchField
            ref={search}
            className="clip-search"
            value={query}
            onChange={setQuery}
            placeholder="Search the history"
          />
        }
        trailing={
          !loaded ? (
            <span className="clip-count">
              <SkeletonText width={64} />
            </span>
          ) : (
            !error && (
              <span className="clip-count ui-num">
                {query ? `${rows.length} ${rows.length === 1 ? "match" : "matches"}` : count}
              </span>
            )
          )
        }
      />

      {error && (
        <Notice
          tone="critical"
          title="Clipboard history is unavailable"
          actions={
            <Button size="small" onClick={() => void load()}>
              Retry
            </Button>
          }
        >
          {error}
        </Notice>
      )}

      {!loaded && <ListSkeleton rows={10} trailing={56} label="Reading clipboard history" />}

      {loaded &&
        rows.length === 0 &&
        !error &&
        (query ? (
          <EmptyState
            icon={<ClipboardList />}
            title={`Nothing matches “${query}”`}
            action={
              <Button
                icon={<X />}
                onClick={() => {
                  setQuery("");
                  search.current?.focus();
                }}
              >
                Clear search
              </Button>
            }
          >
            Try a different word from what you copied.
          </EmptyState>
        ) : (
          <EmptyState icon={<ClipboardList />} title="Nothing captured yet">
            Copy something and it appears here. Capture needs the GNOME Shell extension, because on
            Wayland only the compositor may watch the clipboard.
          </EmptyState>
        ))}

      {rows.length > 0 && (
        <List label="Clipboard history">
          {rows.map((row) => (
            <ListRow
              key={row.id}
              leading={<span className="clip-kind">{kindIcon(row)}</span>}
              title={<span className="clip-text ui-selectable">{row.preview}</span>}
              subtitle={details(row)}
              onClick={() => void copy(row)}
              trailing={
                <>
                  <IconButton
                    size="small"
                    label={copied === row.id ? "Copied" : "Copy"}
                    tooltip={
                      copied === row.id ? "Copied to the clipboard" : "Copy to the clipboard"
                    }
                    icon={copied === row.id ? <Check className="clip-copied" /> : <Copy />}
                    onClick={() => void copy(row)}
                  />
                  <IconButton
                    size="small"
                    label="Forget entry"
                    tooltip="Forget this entry"
                    icon={<X />}
                    onClick={() => void remove(row.id)}
                  />
                </>
              }
            />
          ))}
        </List>
      )}

      <ConfirmDialog
        open={clearRequest !== null}
        onClose={() => setClearRequest(null)}
        onConfirm={() => void clear()}
        busy={clearing}
        destructive
        title="Clear clipboard history?"
        message={
          clearRequest
            ? clearRequest.query
              ? `This deletes all ${clearRequest.total} entries, not only the ${clearRequest.matching} matching “${clearRequest.query}”. It cannot be undone.`
              : `This deletes all ${clearRequest.total} entries. It cannot be undone.`
            : undefined
        }
        confirmLabel="Clear all"
      />
    </Page>
  );
}

function kindIcon(row: ClipRow) {
  const text = row.text.trim();
  if (/^https?:\/\/\S+$/i.test(text)) {
    return <Link2 aria-label="Link" />;
  }
  if (/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(text)) {
    return <AtSign aria-label="Email address" />;
  }
  if (row.lines > 1) {
    return <TextAlignStart aria-label="Multiline text" />;
  }
  return <Type aria-label="Text" />;
}

function details(row: ClipRow): string {
  return [
    timeAgo(row.lastSeen),
    row.lines > 1 ? `${row.lines} lines` : null,
    row.count > 1 ? `copied ${row.count}×` : null,
    bytes(row.bytes),
  ]
    .filter(Boolean)
    .join(" · ");
}
