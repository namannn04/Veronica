import { useCallback, useEffect, useState } from "react";
import { ipc } from "../lib/ipc";
import type { AttentionFocusSession, AttentionStatus } from "../lib/types";

function duration(seconds: number) {
  const value = Math.max(0, Math.floor(seconds));
  const hours = Math.floor(value / 3600);
  const minutes = Math.floor((value % 3600) / 60);
  return hours ? `${hours}h ${minutes}m` : `${minutes}m`;
}

export function AttentionPage() {
  const [status, setStatus] = useState<AttentionStatus | null>(null);
  const [history, setHistory] = useState<AttentionFocusSession[]>([]);
  const [name, setName] = useState("Deep work");
  const [minutes, setMinutes] = useState(45);
  const [now, setNow] = useState(Date.now());
  const [error, setError] = useState("");
  const refresh = useCallback(async () => {
    try {
      const [next, sessions] = await Promise.all([ipc.attentionStatus(), ipc.attentionHistory()]);
      setStatus(next); setHistory(sessions); setError("");
    } catch (cause) { setError(String(cause)); }
  }, []);
  useEffect(() => { void refresh(); }, [refresh]);
  useEffect(() => { const timer = window.setInterval(() => setNow(Date.now()), 1000); return () => window.clearInterval(timer); }, []);
  const active = status?.active;
  const remaining = active ? active.plannedDurationSeconds - Math.floor((now - Date.parse(active.startedAt)) / 1000) : 0;
  const start = async () => { try { await ipc.attentionStart(name, minutes * 60); await refresh(); } catch (cause) { setError(String(cause)); } };
  const stop = async () => { try { await ipc.attentionStop(); await refresh(); } catch (cause) { setError(String(cause)); } };
  return <div className="page attention-page">
    <div className="page-head edith-head"><div><h1>Attention</h1><div className="page-sub">Protect focused work and keep a private local history</div></div></div>
    {error && <div className="notice error">{error}</div>}
    <section className="card attention-focus">
      {active ? <><span className="eyebrow">FOCUSING ON</span><h2>{active.name}</h2><div className="attention-clock">{remaining >= 0 ? duration(remaining) : `Overtime ${duration(-remaining)}`}</div><button className="button primary" onClick={() => void stop()}>Finish session</button></> : <><h2>Start a focus session</h2><div className="backup-path"><input value={name} onChange={event => setName(event.target.value)} aria-label="Session name"/><input type="number" min="1" max="1440" value={minutes} onChange={event => setMinutes(Number(event.target.value))} aria-label="Minutes"/><button className="button primary" onClick={() => void start()}>Start</button></div></>}
    </section>
    <div className="metric-grid"><div className="metric-card"><span>Completed</span><strong>{status?.completedSessions ?? 0}</strong></div><div className="metric-card"><span>Total focus</span><strong>{duration(status?.totalFocusSeconds ?? 0)}</strong></div></div>
    <section className="card"><div className="card-head"><h2>Focus history</h2></div>{history.length ? <div className="list">{history.map(item => <div className="list-row" key={item.id}><div><strong>{item.name}</strong><small>{new Date(item.startedAt).toLocaleString()}</small></div><span>{duration((Date.parse(item.endedAt ?? item.startedAt) - Date.parse(item.startedAt)) / 1000)}</span></div>)}</div> : <p className="quiet">Completed sessions will appear here.</p>}</section>
  </div>;
}
