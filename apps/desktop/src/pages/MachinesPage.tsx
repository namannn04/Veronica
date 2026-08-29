import { useCallback, useEffect, useState } from "react";

import { ipc } from "../lib/ipc";
import { bytes, countdown, percent } from "../lib/format";
import type { ContainerInfo, MachineDirectory, MachineReport } from "../lib/types";

/** How often to re-probe while the page is open. */
const TICK_MS = 5000;

/**
 * The fleet.
 *
 * This computer is always listed and needs no configuration. Remote machines are
 * reached by running `ssh`, so whatever already works in a terminal works here:
 * config aliases, keys, agents and jump hosts all apply, and Veronica never
 * handles a key itself.
 */
export function MachinesPage() {
  const [reports, setReports] = useState<MachineReport[]>([]);
  const [discovered, setDiscovered] = useState<string[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [target, setTarget] = useState("");
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    try {
      setReports(await ipc.machinesProbe());
      setError(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  const loadDiscovered = useCallback(async () => {
    try {
      setDiscovered(await ipc.machinesDiscover());
    } catch {
      // No SSH config is perfectly normal.
    }
  }, []);

  useEffect(() => {
    void load();
    void loadDiscovered();
    // Probing a remote host takes a moment, so the interval is generous.
    const timer = setInterval(load, TICK_MS);
    return () => clearInterval(timer);
  }, [load, loadDiscovered]);

  const add = async (sshTarget: string, label?: string) => {
    if (!sshTarget.trim()) return;
    setBusy(true);
    try {
      await ipc.machinesAdd(sshTarget.trim(), label?.trim() || null, null);
      setTarget("");
      setName("");
      setError(null);
      await Promise.all([load(), loadDiscovered()]);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const remove = async (id: string) => {
    setBusy(true);
    try {
      await ipc.machinesRemove(id);
      await Promise.all([load(), loadDiscovered()]);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <div className="page-head">
        <div>
          <h1>Machines</h1>
          <div className="page-sub">
            This computer and any host you can reach over SSH
          </div>
        </div>
        <button className="button" onClick={load} disabled={loading}>
          {loading ? "Probing…" : "Refresh"}
        </button>
      </div>

      {error && <div className="banner error">{error}</div>}

      <div className="grid" style={{ gap: 12 }}>
        {reports.map((report) => (
          <MachineCard
            key={report.machine.id}
            report={report}
            onRemove={report.machine.reach.kind === "local" ? undefined : remove}
            busy={busy}
          />
        ))}
      </div>

      <section className="card" style={{ marginTop: 12 }}>
        <div className="card-head">
          <h2>Add a machine</h2>
          <span className="card-note">Anything `ssh` can already reach</span>
        </div>
        <div className="toolbar" style={{ marginBottom: 0 }}>
          <input
            className="button"
            style={{ minWidth: 200 }}
            placeholder="ssh target, e.g. tuf or naman@10.0.0.5"
            value={target}
            onChange={(event) => setTarget(event.target.value)}
            aria-label="SSH target"
          />
          <input
            className="button"
            style={{ minWidth: 140 }}
            placeholder="name (optional)"
            value={name}
            onChange={(event) => setName(event.target.value)}
            aria-label="Display name"
          />
          <button
            className="button primary"
            onClick={() => void add(target, name)}
            disabled={busy || !target.trim()}
          >
            Add
          </button>
        </div>

        {discovered.length > 0 && (
          <div style={{ marginTop: 12 }}>
            <div className="tile-label" style={{ marginBottom: 6 }}>
              Found in your SSH config
            </div>
            <div className="toolbar" style={{ marginBottom: 0 }}>
              {discovered.map((alias) => (
                <button
                  key={alias}
                  className="chip"
                  onClick={() => void add(alias)}
                  disabled={busy}
                  title={`Add ${alias}`}
                >
                  + {alias}
                </button>
              ))}
            </div>
          </div>
        )}
      </section>
    </>
  );
}

function MachineCard({
  report,
  onRemove,
  busy,
}: {
  report: MachineReport;
  onRemove?: (id: string) => void;
  busy: boolean;
}) {
  const { machine, stats, error } = report;
  const [tool, setTool] = useState<"files" | "containers" | null>(null);
  const [directory, setDirectory] = useState<MachineDirectory | null>(null);
  const [containers, setContainers] = useState<ContainerInfo[]>([]);
  const [toolBusy, setToolBusy] = useState(false);
  const [toolError, setToolError] = useState("");
  const reach =
    machine.reach.kind === "local" ? "this computer" : `ssh ${machine.reach.target}`;

  const loadFiles = async (path: string | null = null) => {
    setTool("files"); setToolBusy(true); setToolError("");
    try { setDirectory(await ipc.machinesFiles(machine.id, path)); }
    catch (reason) { setToolError(String(reason)); }
    finally { setToolBusy(false); }
  };
  const loadContainers = async () => {
    setTool("containers"); setToolBusy(true); setToolError("");
    try { setContainers(await ipc.machinesContainers(machine.id)); }
    catch (reason) { setToolError(String(reason)); }
    finally { setToolBusy(false); }
  };
  const openFile = async (path: string) => {
    setToolBusy(true); setToolError("");
    try { await ipc.openExternal(await ipc.machinesFileDownload(machine.id, path)); }
    catch (reason) { setToolError(String(reason)); }
    finally { setToolBusy(false); }
  };
  const act = async (container: ContainerInfo, action: "start" | "stop" | "restart") => {
    setToolBusy(true); setToolError("");
    try { await ipc.machinesContainerAction(machine.id, container.engine, container.id, action); await loadContainers(); }
    catch (reason) { setToolError(String(reason)); setToolBusy(false); }
  };

  return (
    <section className="card">
      <div className="card-head">
        <h2>
          {machine.name}{" "}
          {stats ? (
            <span className="pill good">Online</span>
          ) : (
            <span className="pill critical">Unreachable</span>
          )}
        </h2>
        <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
          <span className="card-note mono">{reach}</span>
          <button className="button" disabled={!stats || toolBusy} onClick={() => void ipc.machinesTerminal(machine.id).catch((reason) => setToolError(String(reason)))}>Terminal</button>
          {onRemove && (
            <button
              className="button"
              onClick={() => onRemove(machine.id)}
              disabled={busy}
            >
              Remove
            </button>
          )}
        </div>
      </div>

      {!stats && (
        <p className="card-note">{error ?? "No answer yet."}</p>
      )}

      {stats && (
        <>
          <div className="card-note" style={{ marginBottom: 10 }}>
            {stats.hostName} · {stats.os} · kernel {stats.kernel} · up{" "}
            {countdown(stats.uptimeSecs)}
          </div>

          <div className="grid tiles" style={{ marginBottom: 0 }}>
            <Meter label="CPU" value={stats.cpuPercent} note={`load ${stats.loadAverage[0].toFixed(2)}`} />
            <Meter
              label="Memory"
              value={
                stats.memoryTotalBytes > 0
                  ? ((stats.memoryTotalBytes - stats.memoryAvailableBytes) /
                      stats.memoryTotalBytes) *
                    100
                  : 0
              }
              note={`${bytes(
                stats.memoryTotalBytes - stats.memoryAvailableBytes,
              )} of ${bytes(stats.memoryTotalBytes)}`}
            />
            {stats.disks.slice(0, 2).map((disk) => (
              <Meter
                key={disk.mountPoint}
                label={disk.mountPoint}
                literal
                value={
                  disk.totalBytes > 0
                    ? ((disk.totalBytes - disk.availableBytes) / disk.totalBytes) * 100
                    : 0
                }
                note={bytes(disk.totalBytes)}
              />
            ))}
          </div>
          <div className="machine-actions">
            <button className="button" aria-pressed={tool === "files"} onClick={() => void loadFiles(directory?.path ?? null)}>Files</button>
            <button className="button" aria-pressed={tool === "containers"} onClick={() => void loadContainers()}>Containers</button>
            {tool && <button className="button" onClick={() => { setTool(null); setToolError(""); }}>Close</button>}
          </div>
          {toolError && <div className="banner error">{toolError}</div>}
          {tool === "files" && <div className="machine-tool"><div className="machine-path"><button className="button" disabled={!directory?.parent || toolBusy} onClick={() => void loadFiles(directory?.parent ?? null)}>↑</button><code>{directory?.path ?? "Loading…"}</code><button className="button" disabled={toolBusy} onClick={() => void loadFiles(directory?.path ?? null)}>Refresh</button></div>{directory && directory.entries.length > 0 ? <div className="machine-file-list">{directory.entries.map((entry) => <button key={entry.path} disabled={toolBusy} onClick={() => entry.kind === "directory" ? void loadFiles(entry.path) : void openFile(entry.path)}><span className="machine-file-icon">{entry.kind === "directory" ? "▰" : "▤"}</span><span><strong>{entry.name}</strong><small>{entry.kind === "directory" ? "Folder" : bytes(entry.sizeBytes)}</small></span><span>›</span></button>)}</div> : !toolBusy && <div className="empty compact"><p>This folder is empty.</p></div>}</div>}
          {tool === "containers" && <div className="machine-tool">{containers.length > 0 ? <div className="machine-container-list">{containers.map((container) => <div key={`${container.engine}-${container.id}`}><span className={`pill ${container.state === "running" ? "good" : "warning"}`}>{container.state}</span><span><strong>{container.name}</strong><small>{container.image} · {container.status}</small></span><div>{container.state === "running" ? <button className="button" disabled={toolBusy} onClick={() => void act(container, "stop")}>Stop</button> : <button className="button" disabled={toolBusy} onClick={() => void act(container, "start")}>Start</button>}<button className="button" disabled={toolBusy} onClick={() => void act(container, "restart")}>Restart</button></div></div>)}</div> : !toolBusy && <div className="empty compact"><p>No Docker or Podman containers found.</p></div>}</div>}
        </>
      )}
    </section>
  );
}

function Meter({
  label,
  value,
  note,
  literal,
}: {
  label: string;
  value: number;
  note: string;
  /** True when the label is a literal value, such as a mount point. */
  literal?: boolean;
}) {
  // A meter is a status readout, so the band carries a word as well as a colour.
  const status = value >= 85 ? "critical" : value >= 60 ? "warning" : "good";
  return (
    <div className="card" style={{ padding: "11px 12px" }}>
      <div className={`tile-label truncate${literal ? " literal" : ""}`} title={label}>
        {label}
      </div>
      <div className="tile-value" style={{ fontSize: 20 }}>
        {percent(value)}
      </div>
      <div style={{ height: 5, background: "var(--grid)", borderRadius: 3, marginTop: 7 }}>
        <div
          style={{
            width: `${Math.min(100, Math.max(value, 1))}%`,
            height: "100%",
            background: `var(--status-${status})`,
            borderRadius: 3,
          }}
        />
      </div>
      <div className="tile-note truncate">{note}</div>
    </div>
  );
}
