import { convertFileSrc } from "@tauri-apps/api/core";
import { Activity, AudioLines, Mic, NotebookPen, Pin, Plus, Save, Trash2 } from "lucide-react";
import { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState } from "react";

import { countdown, stopwatch, timeAgo } from "../lib/format";
import { ipc, listenEvent } from "../lib/ipc";
import type { AttentionOverview, CompanionItem } from "../lib/types";
import {
  Badge,
  BarList,
  Button,
  Card,
  Checkbox,
  ConfirmDialog,
  EmptyState,
  List,
  ListRow,
  Notice,
  Page,
  PageHeader,
  SearchField,
  Section,
  Spinner,
  Stat,
  StatGrid,
  StatusDot,
  Tabs,
  TextArea,
} from "../ui";
import "./CompanionPage.css";

const CompanionChat = lazy(() =>
  import("../components/CompanionChat").then((m) => ({ default: m.CompanionChat })),
);

type View = "all" | "notes" | "voice" | "activity" | "chat";

const ICON = { strokeWidth: 1.75 };
const VIEWS = [
  { id: "all", label: "Everything" },
  { id: "notes", label: "Notes" },
  { id: "voice", label: "Voice" },
  { id: "activity", label: "Activity" },
  { id: "chat", label: "Conversation" },
] as const;
const SUBTITLES: Record<View, string> = {
  all: "Local notes and voice memos, searchable in one place",
  notes: "Notes you write, kept on this device",
  voice: "Voice memos recorded on this device",
  activity: "Private application activity from the last 7 days",
  chat: "An opt-in conversation with a model you connect",
};

export function CompanionPage() {
  const [items, setItems] = useState<CompanionItem[]>([]);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [view, setView] = useState<View>("all");
  const [query, setQuery] = useState("");
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  const [pinned, setPinned] = useState(false);
  const [audio, setAudio] = useState("");
  const [recording, setRecording] = useState(false);
  const [elapsed, setElapsed] = useState(0);
  const [activity, setActivity] = useState<AttentionOverview | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [saved, setSaved] = useState(false);
  const [loaded, setLoaded] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const queryRef = useRef("");

  const refresh = useCallback(async (search: string) => {
    const next = await ipc.companionList(search);
    setItems(next);
    setLoaded(true);
    setSelectedId((current) =>
      current && next.some((item) => item.id === current) ? current : (next[0]?.id ?? null),
    );
  }, []);

  const loadFailed = (reason: unknown) => {
    setError(String(reason));
    setLoaded(true);
  };

  useEffect(() => {
    queryRef.current = query;
  }, [query]);

  useEffect(() => {
    void refresh("").catch(loadFailed);
    void ipc
      .attentionOverview(7)
      .then(setActivity)
      .catch(() => {});
    void ipc
      .companionRecordingStatus()
      .then((status) => {
        setRecording(status.recording);
        setElapsed(status.elapsedSeconds);
      })
      .catch(() => {});
    const updated = listenEvent("companion-updated", () => void refresh(queryRef.current));
    return () => {
      void updated.then((unlisten) => unlisten());
    };
  }, []);

  useEffect(() => {
    const timer = window.setTimeout(() => void refresh(query).catch(loadFailed), 180);
    return () => window.clearTimeout(timer);
  }, [query]);

  useEffect(() => {
    if (!recording) return;
    const timer = window.setInterval(() => setElapsed((value) => value + 1), 1000);
    return () => window.clearInterval(timer);
  }, [recording]);

  const selected = items.find((item) => item.id === selectedId) ?? null;
  useEffect(() => {
    setTitle(selected?.title ?? "");
    setBody(selected?.body ?? "");
    setPinned(selected?.pinned ?? false);
    setAudio("");
    setSaved(false);
    if (selected?.kind === "voice")
      void ipc
        .companionAudio(selected.id)
        .then((path) => setAudio(convertFileSrc(path)))
        .catch((reason) => setError(String(reason)));
  }, [selectedId, selected?.updatedAt]);

  const visible = useMemo(
    () =>
      items.filter(
        (item) =>
          view === "all" ||
          view === "activity" ||
          (view === "notes" ? item.kind === "note" : item.kind === view),
      ),
    [items, view],
  );

  const createNote = async () => {
    setBusy(true);
    setError("");
    try {
      const item = await ipc.companionNoteCreate("Untitled note", "");
      setQuery("");
      await refresh("");
      setSelectedId(item.id);
      setView("notes");
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  };
  const save = async () => {
    if (!selected) return;
    setBusy(true);
    setError("");
    try {
      await ipc.companionUpdate(selected.id, title, body, pinned);
      await refresh(query);
      setSaved(true);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  };
  const remove = async () => {
    if (!selected) return;
    setConfirmDelete(false);
    setBusy(true);
    try {
      await ipc.companionRemove(selected.id);
      setSelectedId(null);
      await refresh(query);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  };
  const startRecording = async () => {
    setBusy(true);
    setError("");
    try {
      await ipc.companionRecordStart();
      setElapsed(0);
      setRecording(true);
      setView("voice");
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  };
  const stopRecording = async () => {
    setBusy(true);
    try {
      const item = await ipc.companionRecordStop();
      setRecording(false);
      setQuery("");
      await refresh("");
      setSelectedId(item.id);
    } catch (reason) {
      setError(String(reason));
      const status = await ipc.companionRecordingStatus().catch(() => null);
      if (status) {
        setRecording(status.recording);
        setElapsed(status.elapsedSeconds);
      }
    } finally {
      setBusy(false);
    }
  };
  const cancelRecording = async () => {
    setBusy(true);
    try {
      await ipc.companionRecordCancel();
      setRecording(false);
      setElapsed(0);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  };

  const listView = view !== "chat" && view !== "activity";
  const dirty =
    selected !== null &&
    (title !== selected.title || body !== selected.body || pinned !== selected.pinned);

  return (
    <Page
      fill={listView}
      className="comp-page"
      header={
        <PageHeader
          title="Companion"
          subtitle={SUBTITLES[view]}
          actions={
            recording ? (
              <>
                <span className="comp-recording ui-num" role="status">
                  <StatusDot tone="critical" />
                  Recording {stopwatch(elapsed)}
                </span>
                <Button variant="plain" disabled={busy} onClick={() => void cancelRecording()}>
                  Cancel
                </Button>
                <Button variant="primary" busy={busy} onClick={() => void stopRecording()}>
                  Stop and save
                </Button>
              </>
            ) : (
              <>
                <Button icon={<Plus {...ICON} />} disabled={busy} onClick={() => void createNote()}>
                  New note
                </Button>
                <Button
                  variant={view === "chat" ? "secondary" : "primary"}
                  icon={<Mic {...ICON} />}
                  disabled={busy}
                  onClick={() => void startRecording()}
                >
                  Record memo
                </Button>
              </>
            )
          }
          accessory={<Tabs items={VIEWS} value={view} onChange={setView} label="Companion views" />}
        />
      }
    >
      {error && (
        <Notice
          tone="critical"
          title="Companion hit a problem"
          actions={
            <Button
              size="small"
              onClick={() => {
                setError("");
                void refresh(query).catch(loadFailed);
              }}
            >
              Retry
            </Button>
          }
        >
          {error}
        </Notice>
      )}

      {view === "chat" ? (
        <Suspense
          fallback={
            <div className="comp-loading">
              <Spinner label="Opening the conversation" />
            </div>
          }
        >
          <CompanionChat />
        </Suspense>
      ) : view === "activity" ? (
        <ActivityPanel overview={activity} />
      ) : error && items.length === 0 ? null : (
        <div className="comp-panes">
          <Card padding="none" className="comp-list-pane">
            <div className="comp-search">
              <SearchField value={query} onChange={setQuery} placeholder="Search notes and memos" />
            </div>
            <div className="comp-list-scroll">
              {!loaded ? (
                <div className="comp-loading">
                  <Spinner label="Reading notes" />
                </div>
              ) : visible.length ? (
                <List inset={false} label="Notes and memos">
                  {visible.map((item) => (
                    <ListRow
                      key={item.id}
                      leading={
                        item.kind === "voice" ? (
                          <AudioLines {...ICON} aria-label="Voice memo" />
                        ) : (
                          <NotebookPen {...ICON} aria-label="Note" />
                        )
                      }
                      title={item.title}
                      subtitle={
                        item.body ||
                        (item.kind === "voice"
                          ? `${stopwatch(item.durationSeconds ?? 0)} recording`
                          : "Empty note")
                      }
                      meta={
                        <span className="comp-row-meta">
                          {item.pinned && (
                            <Pin {...ICON} aria-label="Pinned" className="comp-pin" />
                          )}
                          {timeAgo(item.updatedAt)}
                        </span>
                      }
                      selected={item.id === selectedId}
                      onClick={() => setSelectedId(item.id)}
                    />
                  ))}
                </List>
              ) : query ? (
                <EmptyState
                  size="compact"
                  title="Nothing matches"
                  action={
                    <Button size="small" onClick={() => setQuery("")}>
                      Clear search
                    </Button>
                  }
                >
                  Try different words.
                </EmptyState>
              ) : (
                <p className="comp-list-empty">
                  {view === "voice"
                    ? "No voice memos yet"
                    : view === "notes"
                      ? "No notes yet"
                      : "Nothing saved yet"}
                </p>
              )}
            </div>
          </Card>

          <Card className="comp-editor">
            {selected ? (
              <>
                <div className="comp-editor-head">
                  <Badge tone={selected.kind === "voice" ? "accent" : "neutral"}>
                    {selected.kind === "voice" ? "Voice memo" : "Note"}
                  </Badge>
                  <span className="comp-editor-time">Edited {timeAgo(selected.updatedAt)}</span>
                  <span className="comp-spacer" />
                  <Checkbox label="Pinned" checked={pinned} onChange={setPinned} />
                </div>
                <input
                  className="comp-title"
                  value={title}
                  maxLength={120}
                  onChange={(event) => setTitle(event.target.value)}
                  aria-label="Title"
                  spellCheck
                />
                {selected.kind === "voice" && audio && (
                  <audio className="comp-audio" src={audio} controls preload="metadata" />
                )}
                <TextArea
                  className="comp-body"
                  value={body}
                  maxLength={100000}
                  onChange={(event) => setBody(event.target.value)}
                  aria-label="Body"
                  placeholder={
                    selected.kind === "voice"
                      ? "Add searchable notes about this recording"
                      : "Write anything"
                  }
                />
                <div className="comp-editor-foot">
                  <span className="comp-count ui-num">
                    {dirty
                      ? "Unsaved changes"
                      : saved
                        ? "Saved on this device"
                        : `${body.length.toLocaleString()} characters`}
                  </span>
                  <span className="comp-spacer" />
                  <Button
                    variant="destructive-plain"
                    icon={<Trash2 {...ICON} />}
                    disabled={busy}
                    onClick={() => setConfirmDelete(true)}
                  >
                    Delete
                  </Button>
                  <Button
                    variant="primary"
                    icon={<Save {...ICON} />}
                    busy={busy}
                    onClick={() => void save()}
                  >
                    Save
                  </Button>
                </div>
              </>
            ) : (
              <div className="comp-editor-empty">
                <EmptyState
                  icon={<NotebookPen {...ICON} />}
                  title="Your private memory"
                  action={<Button onClick={() => void createNote()}>New note</Button>}
                >
                  Select an item, write a note, or record a voice memo.
                </EmptyState>
              </div>
            )}
          </Card>
        </div>
      )}

      <ConfirmDialog
        open={confirmDelete}
        onClose={() => setConfirmDelete(false)}
        onConfirm={() => void remove()}
        title={`Delete “${selected?.title ?? ""}”?`}
        message="This removes it from this device."
        confirmLabel="Delete"
        destructive
      />
    </Page>
  );
}

function ActivityPanel({ overview }: { overview: AttentionOverview | null }) {
  if (!overview)
    return (
      <Card>
        <EmptyState icon={<Activity {...ICON} />} title="No activity yet">
          Turn on Attention to build a private application timeline.
        </EmptyState>
      </Card>
    );
  const applications = overview.applications.slice(0, 8);
  return (
    <>
      <StatGrid>
        <Stat
          label="Active time"
          value={<span className="ui-num">{countdown(overview.activeSeconds)}</span>}
        />
        <Stat
          label="Focused"
          value={<span className="ui-num">{countdown(overview.focusedSeconds)}</span>}
        />
        <Stat
          label="Context switches"
          value={<span className="ui-num">{overview.contextSwitches}</span>}
        />
      </StatGrid>
      <Section title="Applications" description="Private application activity, last 7 days">
        <Card>
          {applications.length ? (
            <BarList
              label="Time per application"
              items={applications.map(([name, seconds]) => ({
                id: name,
                label: name,
                value: seconds,
                display: countdown(seconds),
              }))}
            />
          ) : (
            <EmptyState size="compact" title="Nothing recorded">
              No focused applications were recorded this week.
            </EmptyState>
          )}
        </Card>
      </Section>
    </>
  );
}
