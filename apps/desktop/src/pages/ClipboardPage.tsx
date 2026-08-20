import { useCallback, useEffect, useState } from "react";

import { ipc } from "../lib/ipc";
import { bytes, timeAgo } from "../lib/format";
import type { ClipRow } from "../lib/types";

/** How often to pick up newly captured entries. */
const TICK_MS = 3000;

/**
 * The clipboard history.
 *
 * Capture happens in the GNOME Shell extension, because on Wayland only the
 * compositor may watch the selection. This page reads what was captured, and
 * copying back uses the browser clipboard API, which is allowed here because it
 * happens in response to a click in a focused window.
 */
export function ClipboardPage() {
  const [rows, setRows] = useState<ClipRow[]>([]);
  const [query, setQuery] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState<number | null>(null);
  const [loaded, setLoaded] = useState(false);

  const load = useCallback(async () => {
    try {
      setRows(await ipc.clipboardList(query));
      setError(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoaded(true);
    }
  }, [query]);

  useEffect(() => {
    void load();
    const timer = setInterval(load, TICK_MS);
    return () => clearInterval(timer);
  }, [load]);

  const copy = async (row: ClipRow) => {
    try {
      await navigator.clipboard.writeText(row.text);
      setCopied(row.id);
      // A brief confirmation; the row is otherwise unchanged.
      setTimeout(() => setCopied((current) => (current === row.id ? null : current)), 1200);
    } catch (e) {
      setError(`Cannot write to the clipboard: ${e}`);
    }
  };

  const remove = async (id: number) => {
    try {
      await ipc.clipboardRemove(id);
      await load();
    } catch (e) {
      setError(String(e));
    }
  };

  const clear = async () => {
    try {
      await ipc.clipboardClear();
      await load();
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <>
      <div className="page-head">
        <div>
          <h1>Clipboard</h1>
          <div className="page-sub">
            Everything you have copied, kept on this computer
          </div>
        </div>
        <button className="button" onClick={clear} disabled={rows.length === 0}>
          Clear all
        </button>
      </div>

      {error && <div className="banner error">{error}</div>}

      <div className="toolbar">
        <input
          className="button"
          style={{ minWidth: 260 }}
          placeholder="Search the history"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          aria-label="Search the clipboard history"
        />
        <span className="spacer" />
        <span className="card-note">
          {rows.length} {rows.length === 1 ? "entry" : "entries"}
        </span>
      </div>

      {loaded && rows.length === 0 && (
        <div className="empty">
          <h3>{query ? "Nothing matches" : "Nothing captured yet"}</h3>
          <p>
            {query
              ? "Try a different search."
              : "Copy something, and it appears here. Capture needs the GNOME Shell " +
                "extension, because on Wayland only the compositor may watch the clipboard."}
          </p>
        </div>
      )}

      {rows.length > 0 && (
        <section className="card">
          {rows.map((row) => (
            <div className="clip-row" key={row.id}>
              <button
                className="clip-preview"
                onClick={() => void copy(row)}
                title="Click to copy"
              >
                <span className="clip-text">{row.preview}</span>
                <span className="clip-meta">
                  {row.lines > 1 && <span className="pill">{row.lines} lines</span>}
                  {row.count > 1 && <span className="pill">copied {row.count}×</span>}
                  <span className="card-note">{bytes(row.bytes)}</span>
                  <span className="card-note">{timeAgo(row.lastSeen)}</span>
                </span>
              </button>
              <div className="clip-actions">
                {copied === row.id ? (
                  <span className="pill good">Copied</span>
                ) : (
                  <button className="button" onClick={() => void copy(row)}>
                    Copy
                  </button>
                )}
                <button
                  className="button"
                  onClick={() => void remove(row.id)}
                  aria-label="Forget this entry"
                  title="Forget"
                >
                  ✕
                </button>
              </div>
            </div>
          ))}
        </section>
      )}
    </>
  );
}
