import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { countdown, timeAgo } from "../lib/format";
import { ipc, listenEvent } from "../lib/ipc";
import type { AttentionOverview, CompanionItem } from "../lib/types";

type Section = "all" | "notes" | "voice" | "activity";

export function CompanionPage() {
  const [items, setItems] = useState<CompanionItem[]>([]);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [section, setSection] = useState<Section>("all");
  const [query, setQuery] = useState("");
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  const [pinned, setPinned] = useState(false);
  const [audio, setAudio] = useState("");
  const [recording, setRecording] = useState(false);
  const [elapsed, setElapsed] = useState(0);
  const [activity, setActivity] = useState<AttentionOverview | null>(null);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState("");
  const queryRef = useRef("");

  const refresh = useCallback(async (search: string) => {
    const next = await ipc.companionList(search);
    setItems(next);
    setSelectedId((current) => current && next.some((item) => item.id === current) ? current : next[0]?.id ?? null);
  }, []);

  useEffect(() => { queryRef.current = query; }, [query]);

  useEffect(() => {
    void refresh("").catch((reason) => setNotice(String(reason)));
    void ipc.attentionOverview(7).then(setActivity).catch(() => {});
    void ipc.companionRecordingStatus().then((status) => {
      setRecording(status.recording); setElapsed(status.elapsedSeconds);
    }).catch(() => {});
    const updated = listenEvent("companion-updated", () => void refresh(queryRef.current));
    return () => { void updated.then((unlisten) => unlisten()); };
  }, []);

  useEffect(() => {
    const timer = window.setTimeout(() => void refresh(query).catch((reason) => setNotice(String(reason))), 180);
    return () => window.clearTimeout(timer);
  }, [query]);

  useEffect(() => {
    if (!recording) return;
    const timer = window.setInterval(() => setElapsed((value) => value + 1), 1000);
    return () => window.clearInterval(timer);
  }, [recording]);

  const selected = items.find((item) => item.id === selectedId) ?? null;
  useEffect(() => {
    setTitle(selected?.title ?? ""); setBody(selected?.body ?? ""); setPinned(selected?.pinned ?? false); setAudio("");
    if (selected?.kind === "voice") void ipc.companionAudio(selected.id).then(setAudio).catch((reason) => setNotice(String(reason)));
  }, [selectedId, selected?.updatedAt]);

  const visible = useMemo(() => items.filter((item) => section === "all" || section === "activity" || item.kind === section), [items, section]);

  const createNote = async () => {
    setBusy(true); setNotice("");
    try { const item = await ipc.companionNoteCreate("Untitled note", ""); setQuery(""); await refresh(""); setSelectedId(item.id); setSection("notes"); }
    catch (reason) { setNotice(String(reason)); } finally { setBusy(false); }
  };
  const save = async () => {
    if (!selected) return;
    setBusy(true); setNotice("");
    try { await ipc.companionUpdate(selected.id, title, body, pinned); await refresh(query); setNotice("Saved locally"); }
    catch (reason) { setNotice(String(reason)); } finally { setBusy(false); }
  };
  const remove = async () => {
    if (!selected || !window.confirm(`Delete “${selected.title}”?`)) return;
    setBusy(true);
    try { await ipc.companionRemove(selected.id); setSelectedId(null); await refresh(query); }
    catch (reason) { setNotice(String(reason)); } finally { setBusy(false); }
  };
  const startRecording = async () => {
    setBusy(true); setNotice("");
    try { await ipc.companionRecordStart(); setElapsed(0); setRecording(true); setSection("voice"); }
    catch (reason) { setNotice(String(reason)); } finally { setBusy(false); }
  };
  const stopRecording = async () => {
    setBusy(true);
    try { const item = await ipc.companionRecordStop(); setRecording(false); setQuery(""); await refresh(""); setSelectedId(item.id); }
    catch (reason) {
      setNotice(String(reason));
      const status = await ipc.companionRecordingStatus().catch(() => null);
      if (status) { setRecording(status.recording); setElapsed(status.elapsedSeconds); }
    } finally { setBusy(false); }
  };
  const cancelRecording = async () => {
    setBusy(true);
    try { await ipc.companionRecordCancel(); setRecording(false); setElapsed(0); }
    catch (reason) { setNotice(String(reason)); } finally { setBusy(false); }
  };

  return <div className="companion-page">
    <div className="page-head edith-head"><div><h1>Companion</h1><div className="page-sub">Private notes, voice memos and your activity — stored only on this computer</div></div><div className="head-actions"><button className="button" disabled={busy} onClick={() => void createNote()}>＋ New note</button>{recording ? <><span className="recording-clock"><i />{countdown(elapsed)}</span><button className="button primary" disabled={busy} onClick={() => void stopRecording()}>Stop & save</button><button className="button" disabled={busy} onClick={() => void cancelRecording()}>Cancel</button></> : <button className="button primary" disabled={busy} onClick={() => void startRecording()}>● Voice memo</button>}</div></div>
    {notice && <div className={`banner ${notice.includes("Saved") ? "" : "error"}`}>{notice}</div>}
    <div className="pill-tabs"><button className={section === "all" ? "active" : ""} onClick={() => setSection("all")}>Everything</button><button className={section === "notes" ? "active" : ""} onClick={() => setSection("notes")}>Notes</button><button className={section === "voice" ? "active" : ""} onClick={() => setSection("voice")}>Voice</button><button className={section === "activity" ? "active" : ""} onClick={() => setSection("activity")}>Activity</button></div>
    {section === "activity" ? <ActivityPanel overview={activity}/> : <div className="companion-layout"><aside className="card companion-list"><div className="companion-search"><input className="search" value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Search everything"/></div><div>{visible.map((item) => <button key={item.id} className={item.id === selectedId ? "active" : ""} onClick={() => setSelectedId(item.id)}><span className="companion-kind">{item.kind === "voice" ? "◉" : "▤"}</span><span><strong>{item.pinned ? "◆ " : ""}{item.title}</strong><small>{item.body || (item.kind === "voice" ? `${countdown(item.durationSeconds ?? 0)} recording` : "Empty note")}</small><em>{timeAgo(item.updatedAt)}</em></span></button>)}{visible.length === 0 && <div className="empty compact"><p>{query ? "Nothing matches your search." : "Create a note or record a voice memo."}</p></div>}</div></aside><section className="card companion-editor">{selected ? <><div className="companion-editor-head"><span className="pill">{selected.kind === "voice" ? "Voice memo" : "Note"}</span><label><input type="checkbox" checked={pinned} onChange={(event) => setPinned(event.target.checked)}/> Pinned</label><span>{timeAgo(selected.updatedAt)}</span></div><input className="companion-title" value={title} maxLength={120} onChange={(event) => setTitle(event.target.value)} aria-label="Title"/>{selected.kind === "voice" && audio && <audio className="companion-audio" src={audio} controls preload="metadata"/>}<textarea className="companion-body" value={body} maxLength={100000} onChange={(event) => setBody(event.target.value)} placeholder={selected.kind === "voice" ? "Add searchable notes about this recording…" : "Write anything…"}/><div className="companion-editor-actions"><span>{body.length.toLocaleString()} characters</span><button className="button danger" disabled={busy} onClick={() => void remove()}>Delete</button><button className="button primary" disabled={busy} onClick={() => void save()}>Save</button></div></> : <div className="empty"><h3>Your private memory</h3><p>Select an item, create a note, or record a voice memo.</p></div>}</section></div>}
  </div>;
}

function ActivityPanel({ overview }: { overview: AttentionOverview | null }) {
  if (!overview) return <div className="empty"><h3>No activity yet</h3><p>Enable Attention to build a private application timeline.</p></div>;
  const applications = overview.applications.slice(0, 8);
  const maximum = Math.max(1, ...applications.map(([, seconds]) => seconds));
  return <div className="companion-activity"><div className="grid tiles"><div className="card"><span className="tile-label">Active time</span><strong className="tile-value">{countdown(overview.activeSeconds)}</strong></div><div className="card"><span className="tile-label">Focused</span><strong className="tile-value">{countdown(overview.focusedSeconds)}</strong></div><div className="card"><span className="tile-label">Context switches</span><strong className="tile-value">{overview.contextSwitches}</strong></div></div><section className="card"><div className="card-head"><h2>Last 7 days</h2><span className="card-note">Private application activity</span></div><div className="attention-bars">{applications.map(([name, seconds]) => <div className="attention-bar" key={name}><span>{name}</span><div><i style={{ width: `${seconds / maximum * 100}%` }}/></div><b>{countdown(seconds)}</b></div>)}{applications.length === 0 && <div className="empty compact"><p>No focused applications recorded.</p></div>}</div></section></div>;
}
