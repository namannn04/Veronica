import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";

import { ipc } from "../lib/ipc";
import { timeAgo } from "../lib/format";
import type { DesktopNotification } from "../lib/types";

/**
 * Desktop notifications Veronica has seen.
 *
 * GNOME Shell owns `org.freedesktop.Notifications`, so Veronica watches the bus
 * rather than serving it. That makes this a read-only history: forgetting an
 * entry removes it from Veronica's list and does not recall the shell's banner.
 * The wording says so, because a "dismiss" that leaves the banner up would be a
 * lie.
 *
 * Monitoring can be refused by the bus, in which case the list is simply empty
 * and says why rather than looking broken.
 */
export function NotificationList({ limit }: { limit?: number }) {
  const [rows, setRows] = useState<DesktopNotification[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);

  const load = useCallback(async () => {
    try {
      setRows(await ipc.notificationsList());
      setError(null);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setLoaded(true);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  // Pushed from the monitor, so the list is live without polling the bus.
  useEffect(() => {
    const received = listen<DesktopNotification>("notifications-received", (event) => {
      setRows((current) => [event.payload, ...current.filter((row) => row.id !== event.payload.id)]);
    });
    return () => {
      void received.then((un) => un());
    };
  }, []);

  const forget = async (id: number) => {
    // Optimistic: the command only mutates Veronica's own list.
    setRows((current) => current.filter((row) => row.id !== id));
    try {
      await ipc.notificationsDismiss(id);
    } catch (reason) {
      setError(String(reason));
      await load();
    }
  };

  const clear = async () => {
    try {
      await ipc.notificationsClear();
      setRows([]);
    } catch (reason) {
      setError(String(reason));
    }
  };

  const shown = limit ? rows.slice(0, limit) : rows;

  return (
    <>
      <div className="card-head">
        <h2>Notifications</h2>
        {rows.length > 0 ? (
          <button className="jump-link" onClick={() => void clear()}>
            Clear<span>×</span>
          </button>
        ) : (
          <span className="card-note">seen on this computer</span>
        )}
      </div>

      {error && <div className="banner error">{error}</div>}

      {loaded && rows.length === 0 && !error && (
        <p className="quiet">
          Nothing yet. Veronica watches the notification bus read-only, so the desktop's own
          banners are unaffected.
        </p>
      )}

      {shown.length > 0 && (
        <div className="notice-list">
          {shown.map((row) => (
            <div className={`notice-row urgency-${row.urgency}`} key={row.id}>
              <div className="notice-body">
                <div className="notice-head">
                  <strong>{row.summary || row.appName}</strong>
                  <span className="card-note">{timeAgo(new Date(row.receivedAt).toISOString())}</span>
                </div>
                {row.body && <p>{stripMarkup(row.body)}</p>}
                <small>{row.appName}</small>
              </div>
              <button
                onClick={() => void forget(row.id)}
                aria-label={`Forget notification from ${row.appName}`}
                title="Forget — the desktop's own banner is untouched"
              >
                ✕
              </button>
            </div>
          ))}
          {limit && rows.length > limit && (
            <span className="card-note">and {rows.length - limit} more</span>
          )}
        </div>
      )}
    </>
  );
}

/**
 * Notification bodies may contain the small HTML subset the spec allows
 * (`<b>`, `<i>`, `<a href>`, `<img>`). React would render it as literal text, so
 * the tags are stripped rather than shown, and never interpreted.
 */
function stripMarkup(body: string): string {
  return body
    .replace(/<[^>]*>/g, "")
    .replace(/&amp;/g, "&")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"')
    .replace(/&apos;/g, "'")
    .trim();
}
