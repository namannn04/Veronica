import { useCallback, useEffect, useRef, useState } from "react";
import { startVisiblePolling } from "../lib/polling";
import { ipc } from "../lib/ipc";
import { endpointLabel, matchesContainer, matchesPort } from "../lib/services";
import type { DockerContainer, DockerSnapshot, ListeningPort, PortOwner, PortsSnapshot } from "../lib/types";

type Change = { kind: "process"; row: ListeningPort; owner: PortOwner } | { kind: "container"; container: DockerContainer; endpoint: string; action: "start" | "stop" | "restart" };

export function ServicesPage() {
  const [ports, setPorts] = useState<PortsSnapshot | null>(null);
  const [docker, setDocker] = useState<DockerSnapshot | null>(null);
  const [portsError, setPortsError] = useState<string | null>(null);
  const [dockerError, setDockerError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [pending, setPending] = useState<Change | null>(null);
  const [busy, setBusy] = useState(false);
  const [readingPorts, setReadingPorts] = useState(false);
  const [readingDocker, setReadingDocker] = useState(false);
  const live = useRef(true);
  const portRead = useRef(false);
  const dockerRead = useRef(false);
  const actionRunning = useRef(false);
  const loadPorts = useCallback(async () => {
    if (portRead.current) return;
    portRead.current = true; setReadingPorts(true);
    try { const data = await ipc.portsSnapshot(); if (live.current) { setPorts(data); setPortsError(null); } }
    catch (error) { if (live.current) { setPorts(null); setPortsError(String(error)); } }
    finally { portRead.current = false; if (live.current) setReadingPorts(false); }
  }, []);
  const loadDocker = useCallback(async () => {
    if (dockerRead.current) return;
    dockerRead.current = true; setReadingDocker(true);
    try { const data = await ipc.dockerSnapshot(); if (live.current) { setDocker(data); setDockerError(null); } }
    catch (error) { if (live.current) { setDocker(null); setDockerError(String(error)); } }
    finally { dockerRead.current = false; if (live.current) setReadingDocker(false); }
  }, []);
  useEffect(() => {
    live.current = true;
    const stopPorts = startVisiblePolling(async () => { if (!actionRunning.current) await loadPorts(); }, 5000);
    const stopDocker = startVisiblePolling(async () => { if (!actionRunning.current) await loadDocker(); }, 10000);
    return () => { live.current = false; stopPorts(); stopDocker(); };
  }, [loadPorts, loadDocker]);
  const refresh = () => { void loadPorts(); void loadDocker(); };
  const apply = async () => {
    if (!pending || actionRunning.current) return;
    const change = pending;
    actionRunning.current = true; setBusy(true); setActionError(null); setMessage(null);
    try {
      if (change.kind === "process") {
        const message = await ipc.portsStop({ pid: change.owner.pid, startedAt: change.owner.startedAt, inode: change.row.inode });
        if (live.current) setMessage(message);
      } else {
        await ipc.dockerAction(change.endpoint, change.container.id, change.action);
        if (live.current) setMessage(`${change.container.name}: ${change.action} completed.`);
      }
      if (live.current) setPending(null);
    } catch (error) { if (live.current) setActionError(String(error)); }
    finally {
      actionRunning.current = false;
      if (live.current) { setBusy(false); refresh(); }
    }
  };
  const portRows = ports?.ports.filter((row) => matchesPort(row, query)) ?? [];
  const containers = docker?.containers.filter((row) => matchesContainer(row, query)) ?? [];
  const running = docker?.containers.filter((row) => row.state === "running").length ?? 0;
  return <div className="page services-page">
    <header className="page-head"><div><h1>Ports &amp; Docker</h1><p>See what is running on this computer and free a busy port.</p></div><button className="button" onClick={refresh} disabled={busy || readingPorts || readingDocker}>Refresh</button></header>
    <div className="services-summary">
      <div className="card"><span className="eyebrow">Listening sockets</span><strong>{ports ? ports.ports.length : "—"}</strong><small>TCP listeners and unconnected UDP sockets</small></div>
      <div className="card"><span className="eyebrow">Docker</span><strong>{docker?.status === "ready" ? `${running} running` : docker?.status === "missing" ? "Not installed" : docker?.status === "remote" ? "External endpoint" : docker?.status === "unavailable" ? "Unavailable" : dockerError ? "Unavailable" : "Checking…"}</strong><small>{docker?.version ? `Server ${docker.version} · ${docker.containers.length} containers` : "Local daemon and published ports"}</small></div>
    </div>
    <label className="services-search"><span>Find a port, process or container</span><input type="search" placeholder="Try 3000, node, postgres or nginx" value={query} onChange={(event) => setQuery(event.target.value)} /></label>
    {message && <p className="services-message" role="status">{message}</p>}
    {actionError && <div className="banner error" role="alert">{actionError}</div>}
    {pending && <section className="card services-confirm" aria-label="Confirm service change">
      <h2>{pending.kind === "process" ? `Stop ${pending.owner.name} (PID ${pending.owner.pid})?` : `${pending.action[0].toUpperCase() + pending.action.slice(1)} ${pending.container.name}?`}</h2>
      <p>{pending.kind === "process" ? `This requests a graceful stop of the whole process on ${endpointLabel(pending.row)}, including any other ports it owns. A supervisor may restart it.` : pending.action === "start" ? "The container will start on the displayed local Docker endpoint." : "Connections served by this container will be interrupted."}</p>
      <div className="button-row"><button className="button" disabled={busy} onClick={() => setPending(null)}>Cancel</button><button className="button danger" disabled={busy} onClick={() => void apply()}>{busy ? "Applying…" : "Confirm"}</button></div>
    </section>}
    <section className="card services-section"><div className="services-section-heading"><h2>Listening ports</h2><span className="muted">{readingPorts ? "Refreshing…" : "Updates every 5 seconds"}</span></div>
      <p className="muted">IPv4 and IPv6 in the host network namespace. Docker port forwarding may appear only in the container mappings below.</p>
      {portsError && <div className="banner error" role="alert">{portsError}</div>}
      {!ports && !portsError && <p className="muted">Reading listening ports…</p>}
      {ports?.warnings.map((warning) => <p className="services-warning" key={warning}>{warning}</p>)}
      {ports && portRows.length === 0 && <div className="empty compact"><p>{query ? `No listening ports match “${query}”.` : "No listening ports found."}</p></div>}
      <div className="services-list">{portRows.map((row) => <article className="services-row" key={`${row.protocol}-${row.inode}-${row.address}-${row.port}`}>
        <div className="services-endpoint"><span className="pill">{row.protocol}</span><strong>{endpointLabel(row)}</strong></div>
        <div className="services-owners">{row.owners.length === 0 ? <small className="muted">Owner hidden by permissions or no longer running</small> : row.owners.map((owner) => <div className="services-owner" key={`${owner.pid}-${owner.startedAt}`}><span><strong>{owner.name}</strong><small>PID {owner.pid} · UID {owner.uid}</small></span><button className="button" disabled={busy || !!portsError || !owner.canStop} title={owner.canStop ? "Gracefully stop this process" : "This process cannot be stopped with your account"} onClick={() => { setActionError(null); setPending({ kind: "process", row, owner }); }}>Stop process</button></div>)}</div>
      </article>)}</div>
    </section>
    <section className="card services-section"><div className="services-section-heading"><h2>Docker containers</h2><span className={`pill ${docker?.status === "ready" ? "good" : "warning"}`}>{docker?.status === "ready" ? "Available" : "Not connected"}</span></div>
      {dockerError && <div className="banner error" role="alert">{dockerError}</div>}
      {docker && <p className="muted">{docker.detail}</p>}
      {docker?.endpoint && <p className="services-docker-endpoint">{docker.endpoint}</p>}
      {!docker && !dockerError && <p className="muted">Checking Docker…</p>}
      {docker?.status === "ready" && containers.length === 0 && <div className="empty compact"><p>{query ? `No containers match “${query}”.` : "Docker is available; there are no containers yet."}</p></div>}
      <div className="services-list">{containers.map((container) => <article className="services-container" key={container.id}>
        <div className="services-container-head"><div><strong>{container.name}</strong><small>{container.image} · {container.id.slice(0, 12)}</small></div><span className={`pill ${container.state === "running" ? "good" : "warning"}`}>{container.state}</span></div>
        <p className="muted">{container.status}</p><div className="services-port-map">{container.ports || "No published or exposed ports"}</div><small className="muted">host:port → container:port · A bare port is exposed inside Docker, not published to the host.</small>
        <div className="button-row">{(container.state === "running" ? ["stop", "restart"] as const : ["start"] as const).map((action) => <button className="button" key={action} disabled={busy || docker?.status !== "ready"} onClick={() => { if (docker?.endpoint) { setActionError(null); setPending({ kind: "container", container, endpoint: docker.endpoint, action }); } }}>{action[0].toUpperCase() + action.slice(1)}</button>)}</div>
      </article>)}</div>
    </section>
  </div>;
}
