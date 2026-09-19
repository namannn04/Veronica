import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";

import { ipc } from "../lib/ipc";
import type { HerdrAgent, HerdrBoard } from "../lib/types";

const STATUSES: HerdrAgent["status"][] = ["blocked", "working", "unknown", "done", "idle"];
const STATUS_LABEL: Record<HerdrAgent["status"], string> = { blocked: "Blocked", working: "Working", unknown: "Unknown", done: "Done", idle: "Idle" };

export function HerdrPage() {
  const [board, setBoard] = useState<HerdrBoard | null>(null);
  const [error, setError] = useState("");
  const [session, setSession] = useState("all");
  const [kinds, setKinds] = useState<string[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [copied, setCopied] = useState<string | null>(null);

  const load = useCallback(async () => {
    try { setBoard(await ipc.herdrBoard()); setError(""); }
    catch (reason) { setError(String(reason)); }
  }, []);

  useEffect(() => {
    let live = true;
    const refresh = () => { if (live) void load(); };
    refresh();
    const timer = window.setInterval(refresh, 2_000);
    return () => { live = false; window.clearInterval(timer); };
  }, [load]);

  const kindChoices = useMemo(() => [...new Set(board?.agents.map((agent) => agent.kind) ?? [])].sort(), [board]);
  const agents = useMemo(() => (board?.agents ?? []).filter((agent) => (session === "all" || agent.session === session) && (kinds.length === 0 || kinds.includes(agent.kind))), [board, kinds, session]);
  const selectedAgent = board?.agents.find((agent) => agent.id === selected) ?? null;

  const toggleKind = (kind: string) => setKinds((current) => current.includes(kind) ? current.filter((item) => item !== kind) : [...current, kind]);
  // The line comes from the backend, which builds it from the same helper the
  // terminal launcher and `vr herdr attach` use, so the three cannot drift.
  const copyAttach = async (agent: HerdrAgent) => {
    try { await navigator.clipboard.writeText(agent.attachCommand); setCopied(agent.id); window.setTimeout(() => setCopied((id) => id === agent.id ? null : id), 1_200); }
    catch (reason) { setError(`Cannot copy the attach command: ${reason}`); }
  };

  return <div className="herdr-page">
    <div className="page-head edith-head"><div><div className="herdr-title"><span>H</span><h1>Herdr</h1></div><div className="page-sub">Live coding-agent sessions across persistent terminal workspaces</div></div><div className="head-actions"><button className="button" onClick={() => void load()}>Refresh</button>{board?.sessions[0] && <button className="button primary" onClick={() => void ipc.herdrOpen(board.sessions[0].name)}>Open Herdr</button>}</div></div>

    {error && <div className="banner error">{error}</div>}
    {!error && board?.installed && board.error && <div className="banner warn">{board.error}</div>}
    {!board && !error && <HerdrSkeleton />}
    {board && !board.installed && <div className="empty herdr-empty"><div className="empty-glyph">H</div><h3>Herdr is not installed</h3><p>Install Herdr to coordinate live coding agents and persistent terminal sessions from Veronica.</p></div>}

    {board?.installed && <>
      <div className="herdr-toolbar"><select value={session} onChange={(event) => setSession(event.target.value)} aria-label="Machine or session"><option value="all">All sessions</option>{board.sessions.map((item) => <option value={item.name} key={item.name}>{item.name}{item.running ? " · running" : " · stopped"}</option>)}</select><div className="herdr-kinds"><button aria-pressed={kinds.length === 0} onClick={() => setKinds([])}>All</button>{kindChoices.map((kind) => <button key={kind} aria-pressed={kinds.includes(kind)} onClick={() => toggleKind(kind)}><KindMark kind={kind} />{kind}</button>)}</div><span className="spacer" /><span className="live-count"><i />{agents.length} live</span></div>

      <div className={`herdr-workspace ${selectedAgent ? "detail-open" : ""}`}>
        <aside className="herdr-rail"><RailSection title="Agents" count={agents.length}>{agents.map((agent) => <button className={selected === agent.id ? "selected" : ""} key={agent.id} onClick={() => setSelected(agent.id)}><KindMark kind={agent.kind} /><div><strong>{agent.title}</strong><small>{agent.workspace || agent.session}</small></div><i className={`status-dot ${agent.status}`} /></button>)}</RailSection><RailSection title="Terminals" count={board.sessions.length}>{board.sessions.map((item) => <button key={item.name} onClick={() => void ipc.herdrOpen(item.name)}><span className="terminal-mark">›_</span><div><strong>{item.name}</strong><small>{item.running ? "Running" : "Stopped · open to start"}</small></div><i className={`status-dot ${item.running ? "working" : "idle"}`} /></button>)}</RailSection></aside>

        <main className="herdr-board">{STATUSES.map((status) => { const rows = agents.filter((agent) => agent.status === status); return <section className={`herdr-column ${status}`} key={status}><header><div><i /><strong>{STATUS_LABEL[status]}</strong></div><span>{rows.length}</span></header><div>{rows.map((agent) => <button className={selected === agent.id ? "selected" : ""} key={agent.id} onClick={() => setSelected(agent.id)}><div className="agent-card-head"><KindMark kind={agent.kind} /><span>{agent.kind}</span>{agent.focused && <b>Focused</b>}</div><strong>{agent.title}</strong><p>{agent.workspace || "Workspace"}</p><footer><span>{agent.session}</span><span>Open →</span></footer></button>)}{rows.length === 0 && <div className="column-empty">No {STATUS_LABEL[status].toLowerCase()} agents</div>}</div></section>; })}</main>

        {selectedAgent && <aside className="herdr-detail"><button className="detail-close" onClick={() => setSelected(null)} aria-label="Close details">×</button><div className="detail-kind"><KindMark kind={selectedAgent.kind} /><span>{selectedAgent.kind}</span></div><h2>{selectedAgent.title}</h2><div className="agent-view-toggle"><button aria-pressed>Agent</button><button disabled>Diff</button></div><Detail label="Status" value={STATUS_LABEL[selectedAgent.status]} /><Detail label="Session" value={selectedAgent.session} /><Detail label="Workspace" value={selectedAgent.workspace || "—"} /><Detail label="Directory" value={selectedAgent.cwd || "Not reported"} mono /><Detail label="Pane" value={selectedAgent.paneId} mono /><div className="attach-block"><span>ATTACH</span><code>{selectedAgent.attachCommand}</code><div><button className="button" onClick={() => void copyAttach(selectedAgent)}>{copied === selectedAgent.id ? "Copied" : "Copy command"}</button><button className="button primary" onClick={() => void ipc.herdrOpen(selectedAgent.session, selectedAgent.paneId).catch((reason) => setError(String(reason)))}>Open terminal</button></div></div></aside>}
      </div>

      {board.sessions.length === 0 && <div className="empty herdr-empty"><div className="empty-glyph">H</div><h3>No Herdr sessions yet</h3><p>Launch Herdr once and the board will begin following its agents automatically.</p></div>}
      {board.sessions.length > 0 && board.agents.length === 0 && <div className="herdr-session-strip">{board.sessions.map((item) => <div key={item.name}><i className={item.running ? "running" : ""} /><div><strong>{item.name}</strong><span>{item.error || (item.running ? "Session is running; no agents detected yet." : "Session is stopped.")}</span></div><button className="button" onClick={() => void ipc.herdrOpen(item.name)}>{item.running ? "Attach" : "Start"}</button></div>)}</div>}
    </>}
  </div>;
}

function KindMark({ kind }: { kind: string }) { return <span className="kind-mark" aria-hidden="true">{kind.slice(0, 1).toUpperCase()}</span>; }
function Detail({ label, value, mono }: { label: string; value: string; mono?: boolean }) { return <div className="herdr-meta"><span>{label}</span><strong className={mono ? "mono" : ""} title={value}>{value}</strong></div>; }
function RailSection({ title, count, children }: { title: string; count: number; children: ReactNode }) { return <section className="rail-section"><header><strong>{title}</strong><span>{count}</span></header><div>{children}</div></section>; }
function HerdrSkeleton() { return <div className="herdr-skeleton"><aside><i /><i /><i /></aside><main>{STATUSES.map((status) => <section key={status}><i /><i /><i /></section>)}</main></div>; }
