import { useEffect, useRef, useState } from "react";
import { ipc } from "../lib/ipc";
import { GraftPlayground } from "../components/GraftPlayground";
import type { DoctorReport } from "../lib/workspace";

export function WorkspacePage() {
  const [tab,setTab] = useState<"doctor" | "graft">("doctor");
  const [root, setRoot] = useState("");
  const [ports, setPorts] = useState("");
  const [containers, setContainers] = useState("");
  const [report, setReport] = useState<DoctorReport | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const running = useRef(false), live = useRef(true);
  useEffect(() => { live.current = true; return () => { live.current = false; }; }, []);
  const run = async () => {
    if (running.current) return;
    running.current = true; setBusy(true); setError(""); setReport(null);
    try {
      const selected = ports.trim() ? ports.split(",").map(p => { if (!/^\s*\d+\s*$/.test(p)) throw new Error("Use comma-separated port numbers."); return Number(p); }) : [];
      const result = await ipc.workspaceDoctor({ root: root.trim(), ports: selected, containers: containers.trim() ? containers.split(",").map(s => s.trim()) : [] });
      if (live.current) setReport(result);
    } catch (e) { if (live.current) setError(String(e)); }
    finally { running.current = false; if (live.current) setBusy(false); }
  };
  return <div className="page workspace-page">
    <header className="page-head"><div><h1>Developer Workspace</h1><p>Check a project and explore its callable services.</p></div></header>
    <div className="workspace-tabs" role="group" aria-label="Workspace tools"><button className="button" aria-pressed={tab === "doctor"} onClick={() => setTab("doctor")} disabled={busy}>Project Doctor</button><button className="button" aria-pressed={tab === "graft"} onClick={() => setTab("graft")} disabled={busy}>Graftcode Playground</button></div>
    {tab === "graft" ? <GraftPlayground /> : <>
    <section className="card workspace-form"><h2>Project Doctor</h2><p className="muted">Read-only checks, only when you ask. Project scripts and .env files are never opened or executed.</p>
      <label>Project directory<input value={root} onChange={e => { setRoot(e.target.value); setReport(null); }} placeholder="/home/you/projects/my-app" disabled={busy} /></label>
      <label>Expected TCP ports <span className="muted">optional, IPv4 localhost</span><input value={ports} onChange={e => { setPorts(e.target.value); setReport(null); }} placeholder="3000, 5432" disabled={busy} /></label>
      <label>Expected Docker containers <span className="muted">optional, exact names</span><input value={containers} onChange={e => { setContainers(e.target.value); setReport(null); }} placeholder="my-app-db, my-app-cache" disabled={busy} /></label>
      <div><button className="button" onClick={() => void run()} disabled={busy || !root.trim()}>{busy ? "Checking project…" : "Check project"}</button></div>
      <small className="muted">Up to 16 ports and 16 containers. An open port confirms a TCP listener, not application health. No background scans.</small>
    </section>
    {error && <div className="banner error" role="alert">{error}</div>}
    {report && <section className="card" aria-live="polite"><h2>{report.project || "Project"}</h2><p className="muted workspace-path">{report.root}</p><div className="workspace-checks">{report.checks.map((c,i) => <article key={i}><span className={`pill ${c.status === "pass" ? "good" : "warning"}`}>{c.status === "pass" ? "Ready" : "Check needed"}</span><div><strong>{c.name}</strong><p className="muted">{c.detail}</p></div></article>)}</div></section>}
    </>}
  </div>;
}
