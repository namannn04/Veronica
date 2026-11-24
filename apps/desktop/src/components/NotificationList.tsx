import { Bell, BellOff, CheckCheck, X } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { timeAgo } from "../lib/format";
import { ipc, listenEvent } from "../lib/ipc";
import type { DesktopNotification } from "../lib/types";
import {
  Badge,
  Button,
  Card,
  CardHeader,
  EmptyState,
  IconButton,
  List,
  ListSkeleton,
  ListRow,
  Notice,
} from "../ui";
import "./NotificationList.css";

export function NotificationList({ limit, className }: { limit?: number; className?: string }) {
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

  useEffect(() => {
    const received = listenEvent<DesktopNotification>("notifications-received", (event) => {
      setRows((current) => [
        event.payload,
        ...current.filter((row) => row.id !== event.payload.id),
      ]);
    });
    return () => {
      void received.then((un) => un());
    };
  }, []);

  const forget = async (id: number) => {
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
  const hidden = limit ? Math.max(0, rows.length - limit) : 0;

  return (
    <Card className={["notes-card", className].filter(Boolean).join(" ")}>
      <CardHeader
        icon={<Bell />}
        title="Notifications"
        actions={
          rows.length > 0 ? (
            <>
              {hidden > 0 && <span className="notes-more ui-num">{hidden} more</span>}
              <IconButton
                size="small"
                label="Clear notifications"
                tooltip="Clear the list in Veronica"
                icon={<CheckCheck />}
                onClick={() => void clear()}
              />
            </>
          ) : undefined
        }
      />

      {error && (
        <Notice
          tone="critical"
          compact
          title="Notifications could not be read"
          actions={
            <Button size="small" onClick={() => void load()}>
              Retry
            </Button>
          }
        >
          {error}
        </Notice>
      )}

      {!loaded && (
        <ListSkeleton
          rows={4}
          inset={false}
          leading={0}
          meta={40}
          trailing={22}
          label="Loading notifications"
        />
      )}

      {loaded && rows.length === 0 && !error && (
        <EmptyState size="compact" icon={<BellOff />} title="No notifications yet">
          Veronica only reads the notification bus, so desktop banners are unaffected.
        </EmptyState>
      )}

      {shown.length > 0 && (
        <List inset={false} label="Recent notifications">
          {shown.map((row) => (
            <ListRow
              key={row.id}
              title={
                <span className="notes-title">
                  <span className="notes-summary">{row.summary || row.appName}</span>
                  {row.urgency === "critical" && <Badge tone="critical">Urgent</Badge>}
                </span>
              }
              subtitle={[row.appName, stripMarkup(row.body)].filter(Boolean).join(" · ")}
              meta={timeAgo(new Date(row.receivedAt).toISOString())}
              trailing={
                <IconButton
                  size="small"
                  label={`Forget notification from ${row.appName}`}
                  tooltip="Forget in Veronica only"
                  icon={<X />}
                  onClick={() => void forget(row.id)}
                />
              }
            />
          ))}
        </List>
      )}
    </Card>
  );
}

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
