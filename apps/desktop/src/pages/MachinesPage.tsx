import { useCallback, useEffect, useState } from "react";
import { ArrowUp, ChevronRight, File, Folder, Plus, Power, RefreshCw, RotateCcw, Server, SquareTerminal, X, Zap } from "lucide-react";

import { startVisiblePolling } from "../lib/polling";
import { ipc } from "../lib/ipc";
import { bytes, countdown } from "../lib/format";
import type { ContainerInfo, MachineDirectory, MachineReport } from "../lib/types";
import { MeterStat } from "../components/MeterStat";
import {
  Button,
  Card,
  CardHeader,
  ConfirmDialog,
  Dialog,
  EmptyState,
  Field,
  IconButton,
  KeyValueList,
  List,
  ListRow,
  Notice,
  Page,
  PageHeader,
  Segmented,
  SkeletonRows,
  StatGrid,
  StatusDot,
  Table,
  TextField,
  Toolbar,
  type Tone,
} from "../ui";
import "./MachinesPage.css";

const TICK_MS = 5000;
const STROKE = 1.75;

type Tool = "overview" | "files" | "containers" | "sensors";

const TOOLS: { value: Tool; label: string }[] = [
  { value: "overview", label: "Overview" },
  { value: "files", label: "Files" },
  { value: "containers", label: "Containers" },
  { value: "sensors", label: "Sensors" },
];

function reachLabel(report: MachineReport) {
  return report.machine.reach.kind === "local" ? "this computer" : `ssh ${report.machine.reach.target}`;
}

function containerTone(state: string): Tone {
  if (state === "running") return "good";
  if (state === "paused" || state === "restarting") return "warning";
  return "neutral";
}

export function MachinesPage() {
  const [reports, setReports] = useState<MachineReport[]>([]);
  const [discovered, setDiscovered] = useState<string[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [adding, setAdding] = useState(false);
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
      setDiscovered([]);
    }
  }, []);

  useEffect(() => {
    const stop = startVisiblePolling(load, TICK_MS);
    void loadDiscovered();
    return stop;
  }, [load, loadDiscovered]);

  const add = async (sshTarget: string, label?: string) => {
    if (!sshTarget.trim()) return;
    setBusy(true);
    try {
      await ipc.machinesAdd(sshTarget.trim(), label?.trim() || null, null);
      setTarget("");
      setName("");
      setError(null);
      setAdding(false);
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

  const refresh = () => { setLoading(true); void load(); };
  const selected = reports.find((report) => report.machine.id === selectedId) ?? reports[0] ?? null;
  const online = reports.filter((report) => report.stats).length;

  return (
    <Page
      className="mach-page"
      header={
        <PageHeader
          title="Machines"
          subtitle={reports.length > 0 ? `${online} of ${reports.length} answering · this computer and your SSH hosts` : "This computer and any host you can reach over SSH"}
          actions={
            <>
              <Button icon={<RefreshCw strokeWidth={STROKE} />} busy={loading && reports.length > 0} disabled={loading} onClick={refresh}>
                {loading && reports.length > 0 ? "Probing…" : "Refresh"}
              </Button>
              <Button variant="primary" icon={<Plus strokeWidth={STROKE} />} onClick={() => setAdding(true)}>Add machine</Button>
            </>
          }
        />
      }
    >
      {error && (
        <Notice
          tone="critical"
          title={reports.length === 0 ? "No machine could be probed" : "The last probe failed"}
          actions={<Button size="small" disabled={loading} onClick={refresh}>Retry</Button>}
        >
          {error}
        </Notice>
      )}

      {reports.length === 0 ? (
        loading ? (
          <div className="mach-layout">
            <List label="Machines"><div className="mach-pad"><SkeletonRows rows={3} label="Probing this computer and your SSH hosts" /></div></List>
            <Card><SkeletonRows rows={4} /></Card>
          </div>
        ) : !error && (
          <Card>
            <EmptyState
              icon={<Server strokeWidth={STROKE} />}
              title="No machines answered"
              action={<Button onClick={refresh}>Refresh</Button>}
            >
              This computer is always listed. If it stays empty, run vr machines list to see what the probe reports.
            </EmptyState>
          </Card>
        )
      ) : (
        <div className="mach-layout">
          <div className="mach-sidebar">
            <List label="Machines">
              {reports.map((report) => (
                <ListRow
                  key={report.machine.id}
                  leading={<StatusDot tone={report.stats ? "good" : "critical"} label={report.stats ? "Online" : "Unreachable"} />}
                  title={report.machine.name}
                  subtitle={<span className="ui-mono">{reachLabel(report)}</span>}
                  meta={report.stats ? `${Math.round(report.stats.cpuPercent)}% CPU` : "Unreachable"}
                  selected={selected?.machine.id === report.machine.id}
                  onClick={() => setSelectedId(report.machine.id)}
                />
              ))}
            </List>
            {discovered.length > 0 && (
              <Button variant="plain" size="small" icon={<Plus strokeWidth={STROKE} />} onClick={() => setAdding(true)}>
                {discovered.length} host{discovered.length === 1 ? "" : "s"} found in your SSH config
              </Button>
            )}
          </div>
          {selected && (
            <MachineDetail
              key={selected.machine.id}
              report={selected}
              onRemove={selected.machine.reach.kind === "local" ? undefined : remove}
              busy={busy}
            />
          )}
        </div>
      )}

      <Dialog
        open={adding}
        onClose={() => { if (!busy) setAdding(false); }}
        title="Add a machine"
        description="Anything ssh can already reach: config aliases, keys, agents and jump hosts all apply."
        width={460}
        footer={
          <>
            <Button onClick={() => setAdding(false)} disabled={busy}>Cancel</Button>
            <Button variant="primary" busy={busy} disabled={!target.trim()} onClick={() => void add(target, name)}>Add machine</Button>
          </>
        }
      >
        <div className="mach-form">
          <Field label="SSH target" hint="An alias or user@host, for example tuf or naman@10.0.0.5">
            {(id) => (
              <TextField
                id={id}
                mono
                value={target}
                placeholder="user@host"
                aria-label="SSH target"
                onChange={(event) => setTarget(event.target.value)}
                onKeyDown={(event) => { if (event.key === "Enter") void add(target, name); }}
              />
            )}
          </Field>
          <Field label="Name" hint="Optional. Shown instead of the target.">
            {(id) => (
              <TextField
                id={id}
                value={name}
                placeholder="Display name"
                aria-label="Display name"
                onChange={(event) => setName(event.target.value)}
                onKeyDown={(event) => { if (event.key === "Enter") void add(target, name); }}
              />
            )}
          </Field>
          {discovered.length > 0 && (
            <div className="mach-discovered">
              <span className="mach-discovered-label">Found in your SSH config</span>
              <div className="mach-discovered-list">
                {discovered.map((alias) => (
                  <Button key={alias} size="small" icon={<Plus strokeWidth={STROKE} />} disabled={busy} onClick={() => void add(alias)}>
                    <span className="ui-mono">{alias}</span>
                  </Button>
                ))}
              </div>
            </div>
          )}
        </div>
      </Dialog>
    </Page>
  );
}

function MachineDetail({
  report,
  onRemove,
  busy,
}: {
  report: MachineReport;
  onRemove?: (id: string) => void;
  busy: boolean;
}) {
  const { machine, stats, error } = report;
  const [tool, setTool] = useState<Tool>("overview");
  const [directory, setDirectory] = useState<MachineDirectory | null>(null);
  const [containers, setContainers] = useState<ContainerInfo[] | null>(null);
  const [toolBusy, setToolBusy] = useState(false);
  const [toolError, setToolError] = useState("");
  const [toolNote, setToolNote] = useState("");
  const [logs, setLogs] = useState<{ container: string; text: string } | null>(null);
  const [power, setPower] = useState<"restart" | "shutdown" | null>(null);

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
  const powerOff = async () => {
    if (!power) return;
    setToolBusy(true); setToolError("");
    try { await ipc.machinesPower(machine.id, power); }
    catch (reason) { setToolError(String(reason)); }
    finally { setToolBusy(false); setPower(null); }
  };
  const wake = async () => {
    setToolBusy(true); setToolError(""); setToolNote("");
    try { await ipc.machinesWake(machine.id); setToolNote("Wake packet sent. Nothing confirms it: the machine is not answering yet."); }
    catch (reason) { setToolError(String(reason)); }
    finally { setToolBusy(false); }
  };
  const act = async (container: ContainerInfo, action: "start" | "stop" | "restart") => {
    setToolBusy(true); setToolError("");
    try { await ipc.machinesContainerAction(machine.id, container.engine, container.id, action); await loadContainers(); }
    catch (reason) { setToolError(String(reason)); setToolBusy(false); }
  };
  const openTerminal = () => void ipc.machinesTerminal(machine.id).catch((reason) => setToolError(String(reason)));

  const chooseTool = (next: Tool) => {
    setToolError("");
    if (next === "files") void loadFiles(directory?.path ?? null);
    else if (next === "containers") void loadContainers();
    else setTool(next);
  };

  const remote = machine.reach.kind !== "local";
  const verb = power === "restart" ? "Restart" : "Shut down";

  return (
    <Card className="mach-detail">
      <CardHeader
        title={
          <span className="mach-title">
            <span>{machine.name}</span>
            <span className={stats ? "mach-state mach-state-good" : "mach-state mach-state-bad"}>{stats ? "Online" : "Unreachable"}</span>
          </span>
        }
        subtitle={
          stats
            ? <span className="ui-mono">{stats.hostName} · {stats.os} · kernel {stats.kernel} · up {countdown(stats.uptimeSecs)}</span>
            : <span className="ui-mono">{reachLabel(report)}</span>
        }
      />

      <Toolbar
        className="mach-actions"
        leading={
          <>
            <Button size="small" icon={<SquareTerminal strokeWidth={STROKE} />} disabled={!stats || toolBusy} onClick={openTerminal}>Terminal</Button>
            {remote && machine.mac && !stats && (
              <Button size="small" icon={<Zap strokeWidth={STROKE} />} disabled={toolBusy} onClick={() => void wake()}>Wake</Button>
            )}
            {remote && stats && (
              <Button size="small" icon={<RotateCcw strokeWidth={STROKE} />} disabled={toolBusy} onClick={() => setPower("restart")}>Restart</Button>
            )}
            {remote && stats && (
              <Button size="small" variant="destructive-plain" icon={<Power strokeWidth={STROKE} />} disabled={toolBusy} onClick={() => setPower("shutdown")}>Shut down</Button>
            )}
          </>
        }
        trailing={onRemove && (
          <Button size="small" variant="plain" disabled={busy} onClick={() => onRemove(machine.id)}>Remove</Button>
        )}
      />

      {toolNote && <Notice tone="neutral" compact>{toolNote}</Notice>}

      {!stats ? (
        <Notice tone="warning" title="Not answering">
          <span className="ui-mono ui-selectable">{error ?? "No answer yet."}</span>
        </Notice>
      ) : (
        <>
          <Segmented label="Machine view" options={TOOLS} value={tool} onChange={chooseTool} size="small" />

          {toolError && <Notice tone="critical" compact>{toolError}</Notice>}

          {tool === "overview" && (
            <StatGrid min={150}>
              <MeterStat label="CPU" value={stats.cpuPercent} note={`load ${stats.loadAverage[0].toFixed(2)}`} />
              <MeterStat
                label="Memory"
                value={stats.memoryTotalBytes > 0 ? ((stats.memoryTotalBytes - stats.memoryAvailableBytes) / stats.memoryTotalBytes) * 100 : 0}
                note={`${bytes(stats.memoryTotalBytes - stats.memoryAvailableBytes)} of ${bytes(stats.memoryTotalBytes)}`}
              />
              {stats.gpus.map((gpu) => (
                <MeterStat
                  key={gpu.name}
                  label="GPU"
                  value={gpu.utilizationPercent}
                  note={`${bytes(gpu.memoryUsedBytes)} of ${bytes(gpu.memoryTotalBytes)}${gpu.temperatureCelsius === null ? "" : ` · ${gpu.temperatureCelsius.toFixed(0)} °C`}`}
                />
              ))}
              {stats.disks.slice(0, 2).map((disk) => (
                <MeterStat
                  key={disk.mountPoint}
                  label={disk.mountPoint}
                  mono
                  value={disk.totalBytes > 0 ? ((disk.totalBytes - disk.availableBytes) / disk.totalBytes) * 100 : 0}
                  note={`${bytes(disk.totalBytes)} total`}
                  warnAt={80}
                  criticalAt={90}
                />
              ))}
            </StatGrid>
          )}

          {tool === "files" && (
            <div className="mach-tool">
              <div className="mach-path">
                <IconButton size="small" label="Go to parent folder" icon={<ArrowUp strokeWidth={STROKE} />} disabled={!directory?.parent || toolBusy} onClick={() => void loadFiles(directory?.parent ?? null)} />
                <span className="mach-path-text ui-mono ui-selectable">{directory?.path ?? "Loading…"}</span>
                <IconButton size="small" label="Reload this folder" icon={<RefreshCw strokeWidth={STROKE} />} disabled={toolBusy} onClick={() => void loadFiles(directory?.path ?? null)} />
              </div>
              <div className="mach-scroll">
                {!directory && toolBusy ? (
                  <div className="mach-pad"><SkeletonRows rows={5} label="Reading the folder" /></div>
                ) : directory && directory.entries.length > 0 ? (
                  <List label="Folder contents" inset={false}>
                    {directory.entries.map((entry) => (
                      <ListRow
                        key={entry.path}
                        leading={entry.kind === "directory" ? <Folder strokeWidth={STROKE} /> : <File strokeWidth={STROKE} />}
                        title={entry.name}
                        subtitle={entry.kind === "directory" ? "Folder" : bytes(entry.sizeBytes)}
                        trailing={entry.kind === "directory" ? <ChevronRight className="mach-chevron" strokeWidth={STROKE} aria-hidden="true" /> : undefined}
                        onClick={() => {
                          if (toolBusy) return;
                          if (entry.kind === "directory") void loadFiles(entry.path);
                          else void openFile(entry.path);
                        }}
                      />
                    ))}
                  </List>
                ) : (
                  !toolBusy && <EmptyState size="compact" icon={<Folder strokeWidth={STROKE} />} title="This folder is empty" />
                )}
              </div>
            </div>
          )}

          {tool === "containers" && (
            <div className="mach-tool">
              <div className="mach-scroll">
                {containers === null && toolBusy ? (
                  <div className="mach-pad"><SkeletonRows rows={3} label="Reading containers" /></div>
                ) : containers && containers.length > 0 ? (
                  <List label="Containers" inset={false}>
                    {containers.map((container) => (
                      <ListRow
                        key={`${container.engine}-${container.id}`}
                        className="mach-container-row"
                        leading={<StatusDot tone={containerTone(container.state)} label={container.state} />}
                        title={container.name}
                        subtitle={
                          <span className="mach-container-meta">
                            <span>{container.state} · {container.image} · {container.status}</span>
                            {container.ports && <span className="ui-mono">{container.ports}</span>}
                          </span>
                        }
                        trailing={
                          <>
                            {container.state === "running"
                              ? <Button size="small" disabled={toolBusy} onClick={() => void act(container, "stop")}>Stop</Button>
                              : <Button size="small" disabled={toolBusy} onClick={() => void act(container, "start")}>Start</Button>}
                            <Button size="small" disabled={toolBusy} onClick={() => void act(container, "restart")}>Restart</Button>
                            <Button size="small" variant="plain" disabled={toolBusy} onClick={() => void showLogs(container)}>Logs</Button>
                          </>
                        }
                      />
                    ))}
                  </List>
                ) : (
                  !toolBusy && <EmptyState size="compact" title="No containers">This machine has no Docker or Podman containers.</EmptyState>
                )}
              </div>
              {logs && (
                <div className="mach-logs">
                  <div className="mach-logs-head">
                    <span className="mach-logs-title">Logs · <span className="ui-mono">{logs.container}</span></span>
                    <IconButton size="small" label="Close logs" icon={<X strokeWidth={STROKE} />} onClick={() => setLogs(null)} />
                  </div>
                  <pre className="mach-logs-body ui-mono ui-selectable">{logs.text.trim() || "This container has written nothing."}</pre>
                </div>
              )}
            </div>
          )}

          {tool === "sensors" && (
            stats.temperatures.length === 0 && stats.fans.length === 0 && stats.processes.length === 0 && stats.gpus.length === 0 ? (
              <EmptyState size="compact" title="No sensors">This machine exposes no sensors, and its ps reported nothing.</EmptyState>
            ) : (
              <div className="mach-sensors">
                {(stats.gpus.length > 0 || stats.temperatures.length > 0 || stats.fans.length > 0) && (
                  <KeyValueList
                    items={[
                      ...stats.gpus.map((gpu) => ({
                        label: gpu.name,
                        value: `${gpu.utilizationPercent.toFixed(0)}% · ${bytes(gpu.memoryUsedBytes)}${gpu.temperatureCelsius === null ? "" : ` · ${gpu.temperatureCelsius.toFixed(0)} °C`}`,
                      })),
                      ...stats.temperatures.map((reading) => ({
                        label: reading.label,
                        value: <span className={reading.celsius >= 85 ? "mach-hot" : undefined}>{reading.celsius.toFixed(1)} °C{reading.celsius >= 85 ? " · hot" : ""}</span>,
                      })),
                      ...stats.fans.map((fan) => ({ label: fan.label, value: `${fan.rpm.toLocaleString()} rpm` })),
                    ]}
                  />
                )}
                {stats.processes.length > 0 && (
                  <div className="mach-processes">
                    <span className="mach-subhead">Busiest processes</span>
                    <Table label="Busiest processes">
                      <thead>
                        <tr><th>Process</th><th>PID</th><th className="num">CPU</th><th className="num">Memory</th></tr>
                      </thead>
                      <tbody>
                        {stats.processes.map((process) => (
                          <tr key={process.pid}>
                            <td>{process.name}</td>
                            <td className="ui-mono muted">{process.pid}</td>
                            <td className="num">{process.cpuPercent.toFixed(1)}%</td>
                            <td className="num">{bytes(process.memoryBytes)}</td>
                          </tr>
                        ))}
                      </tbody>
                    </Table>
                  </div>
                )}
              </div>
            )
          )}
        </>
      )}

      <ConfirmDialog
        open={power !== null}
        onClose={() => { if (!toolBusy) setPower(null); }}
        onConfirm={() => void powerOff()}
        title={`${verb} ${machine.name}?`}
        message="Veronica asks the machine over SSH and never escalates; the account it connects as has to be permitted already."
        confirmLabel={verb}
        destructive={power === "shutdown"}
        busy={toolBusy && power !== null}
      />
    </Card>
  );
}
