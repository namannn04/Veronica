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

      {/* This computer is always one of the reports, so an empty list means the
          probe has not answered yet or could not run - not that the fleet is
          empty. The two say different things and get different words. */}
      {reports.length === 0 && (
        <div className="empty">
          <h3>{loading ? "Probing…" : "No machines answered"}</h3>
          <p>
            {loading
              ? "Reading this computer, then every host in your SSH config."
              : "This computer should always be listed. Refresh, and if it stays empty run `vr machines probe` to see what the probe reports."}
          </p>
        </div>
      )}

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
  const [tool, setTool] = useState<"files" | "containers" | "sensors" | null>(null);
  const [directory, setDirectory] = useState<MachineDirectory | null>(null);
  const [containers, setContainers] = useState<ContainerInfo[]>([]);
  const [toolBusy, setToolBusy] = useState(false);
  const [toolError, setToolError] = useState("");
  const [logs, setLogs] = useState<{ container: string; text: string } | null>(null);
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
  const showLogs = async (container: ContainerInfo) => {
    setToolBusy(true); setToolError(""); setLogs(null);
    try { setLogs({ container: container.name, text: await ipc.machinesContainerLogs(machine.id, container.engine, container.id) }); }
    catch (reason) { setToolError(String(reason)); }
    finally { setToolBusy(false); }
  };
  // Restarting or shutting down a machine cannot be undone from here, so it is
  // confirmed by name rather than by a button that is easy to hit twice.
  const powerOff = async (action: "restart" | "shutdown") => {
    const verb = action === "restart" ? "Restart" : "Shut down";
    if (!window.confirm(`${verb} ${machine.name}?\n\nVeronica asks the machine over SSH and never escalates; the account it connects as has to be permitted already.`)) return;
    setToolBusy(true); setToolError("");
    try { await ipc.machinesPower(machine.id, action); setToolError(""); }
    catch (reason) { setToolError(String(reason)); }
    finally { setToolBusy(false); }
  };
  const wake = async () => {
    setToolBusy(true); setToolError("");
    try { await ipc.machinesWake(machine.id); setToolError("Wake packet sent. Nothing confirms it: the machine is not answering yet."); }
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
          {machine.reach.kind !== "local" && <>
            {machine.mac && !stats && <button className="button" disabled={toolBusy} onClick={() => void wake()}>Wake</button>}
            {stats && <button className="button" disabled={toolBusy} onClick={() => void powerOff("restart")}>Restart</button>}
            {stats && <button className="button danger" disabled={toolBusy} onClick={() => void powerOff("shutdown")}>Shut down</button>}
          </>}
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
            {stats.gpus.map((gpu) => (
              <Meter
                key={gpu.name}
                label="GPU"
                value={gpu.utilizationPercent}
                note={`${bytes(gpu.memoryUsedBytes)} of ${bytes(gpu.memoryTotalBytes)}${gpu.temperatureCelsius === null ? "" : ` · ${gpu.temperatureCelsius.toFixed(0)} °C`}`}
              />
            ))}
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
            <button className="button" aria-pressed={tool === "sensors"} onClick={() => { setTool("sensors"); setToolError(""); }}>Sensors &amp; processes</button>
            {tool && <button className="button" onClick={() => { setTool(null); setToolError(""); }}>Close</button>}
          </div>
          {toolError && <div className="banner error">{toolError}</div>}
          {tool === "files" && <div className="machine-tool"><div className="machine-path"><button className="button" disabled={!directory?.parent || toolBusy} onClick={() => void loadFiles(directory?.parent ?? null)}>↑</button><code>{directory?.path ?? "Loading…"}</code><button className="button" disabled={toolBusy} onClick={() => void loadFiles(directory?.path ?? null)}>Refresh</button></div>{directory && directory.entries.length > 0 ? <div className="machine-file-list">{directory.entries.map((entry) => <button key={entry.path} disabled={toolBusy} onClick={() => entry.kind === "directory" ? void loadFiles(entry.path) : void openFile(entry.path)}><span className="machine-file-icon">{entry.kind === "directory" ? "▰" : "▤"}</span><span><strong>{entry.name}</strong><small>{entry.kind === "directory" ? "Folder" : bytes(entry.sizeBytes)}</small></span><span>›</span></button>)}</div> : !toolBusy && <div className="empty compact"><p>This folder is empty.</p></div>}</div>}
          {tool === "containers" && <div className="machine-tool">{containers.length > 0 ? <div className="machine-container-list">{containers.map((container) => <div key={`${container.engine}-${container.id}`}><span className={`pill ${container.state === "running" ? "good" : "warning"}`}>{container.state}</span><span><strong>{container.name}</strong><small>{container.image} · {container.status}</small></span><div>{container.state === "running" ? <button className="button" disabled={toolBusy} onClick={() => void act(container, "stop")}>Stop</button> : <button className="button" disabled={toolBusy} onClick={() => void act(container, "start")}>Start</button>}<button className="button" disabled={toolBusy} onClick={() => void act(container, "restart")}>Restart</button><button className="button" disabled={toolBusy} onClick={() => void showLogs(container)}>Logs</button></div></div>)}</div> : !toolBusy && <div className="empty compact"><p>No Docker or Podman containers found.</p></div>}
            {logs && <div className="machine-logs"><div className="card-head"><h2>{logs.container}</h2><button className="button" onClick={() => setLogs(null)}>Close</button></div><pre>{logs.text.trim() || "This container has written nothing."}</pre></div>}</div>}
          {tool === "sensors" && <div className="machine-tool">
            {stats.temperatures.length === 0 && stats.fans.length === 0 && stats.processes.length === 0
              ? <div className="empty compact"><p>This machine exposes no sensors, and its <code>ps</code> reported nothing.</p></div>
              : <div className="machine-sensors">
                  {stats.gpus.map((gpu) => <div className="sensor-row" key={`gpu-${gpu.name}`}><span>{gpu.name}</span><b>{gpu.utilizationPercent.toFixed(0)}% · {bytes(gpu.memoryUsedBytes)}{gpu.temperatureCelsius === null ? "" : ` · ${gpu.temperatureCelsius.toFixed(0)} °C`}</b></div>)}
                  {stats.temperatures.map((reading, index) => <div className="sensor-row" key={`t-${reading.label}-${index}`}><span>{reading.label}</span><b className={reading.celsius >= 85 ? "hot" : ""}>{reading.celsius.toFixed(1)} °C</b></div>)}
                  {stats.fans.map((fan) => <div className="sensor-row" key={`f-${fan.label}`}><span>{fan.label}</span><b>{fan.rpm.toLocaleString()} rpm</b></div>)}
                  {stats.processes.length > 0 && <div className="sensor-heading">Busiest processes</div>}
                  {stats.processes.map((process) => <div className="sensor-row" key={process.pid}><span>{process.name}<small> · {process.pid}</small></span><b>{process.cpuPercent.toFixed(1)}% · {bytes(process.memoryBytes)}</b></div>)}
                </div>}
          </div>}
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
