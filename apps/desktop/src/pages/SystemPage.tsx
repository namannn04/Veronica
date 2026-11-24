import { useCallback, useEffect, useMemo, useState } from "react";
import { ArrowDown, ArrowUp, Bluetooth, Mic, MicOff, Volume2, VolumeX, X } from "lucide-react";

import { startVisiblePolling } from "../lib/polling";
import { ipc, listenEvent } from "../lib/ipc";
import { bytes, countdown } from "../lib/format";
import type { AudioStream, BluetoothState, RunningProcess, SystemSnapshot, VolumeState } from "../lib/types";
import {
  Badge,
  Button,
  Card,
  CardHeader,
  Columns,
  ConfirmDialog,
  EmptyState,
  IconButton,
  KeyValueList,
  List,
  ListRow,
  Notice,
  Page,
  PageHeader,
  ProgressBar,
  Section,
  Skeleton,
  SkeletonRows,
  Slider,
  Stat,
  StatGrid,
  Table,
  Tabs,
  type Tone,
} from "../ui";
import { CleanerPane } from "./CleanerPane";
import "./SystemPage.css";

const TICK_MS = 2000;
const DEVICE_TICK_MS = 5000;
const PROCESS_LIMIT = 40;
const STROKE = 1.75;

type SortKey = "name" | "cpu" | "memory";
type View = "overview" | "apps" | "devices" | "cleaner";

const VIEWS: { id: View; label: string }[] = [
  { id: "overview", label: "Overview" },
  { id: "apps", label: "Apps" },
  { id: "devices", label: "Audio and Bluetooth" },
  { id: "cleaner", label: "Cleaner" },
];

export function SystemPage() {
  const [view, setView] = useState<View>("overview");
  const [snapshot, setSnapshot] = useState<SystemSnapshot | null>(null);
  const [snapshotError, setSnapshotError] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);
  const [processes, setProcesses] = useState<RunningProcess[]>([]);
  const [processesRead, setProcessesRead] = useState(false);
  const [mic, setMic] = useState<VolumeState | null>(null);
  const [streams, setStreams] = useState<AudioStream[]>([]);
  const [mixerError, setMixerError] = useState<string | null>(null);
  const [devicesRead, setDevicesRead] = useState(false);
  const [bluetooth, setBluetooth] = useState<BluetoothState | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [sort, setSort] = useState<{ key: SortKey; ascending: boolean }>({ key: "cpu", ascending: false });
  const [quitting, setQuitting] = useState<RunningProcess | null>(null);
  const [quitBusy, setQuitBusy] = useState(false);

  useEffect(() => {
    let live = true;
    const tick = async () => {
      const [system, apps] = await Promise.allSettled([ipc.systemSnapshot(), ipc.systemProcesses()]);
      if (!live) return;
      if (system.status === "fulfilled") {
        setSnapshot(system.value);
        setSnapshotError(null);
      } else {
        setSnapshotError(String(system.reason));
      }
      if (apps.status === "fulfilled") setProcesses(apps.value);
      setProcessesRead(true);
    };
    const stop = startVisiblePolling(tick, TICK_MS);
    return () => { live = false; stop(); };
  }, [attempt]);

  useEffect(() => {
    void ipc.microphoneState().then(setMic).catch((reason) => setError(String(reason)));
    const toggled = listenEvent<VolumeState>("microphone-updated", (event) => setMic(event.payload));
    return () => { void toggled.then((unlisten) => unlisten()); };
  }, []);

  useEffect(() => {
    let live = true;
    const tick = async () => {
      const [mixer, radio, microphone] = await Promise.allSettled([ipc.audioStreams(), ipc.bluetoothState(), ipc.microphoneState()]);
      if (!live) return;
      if (microphone.status === "fulfilled") setMic(microphone.value);
      setStreams(mixer.status === "fulfilled" ? mixer.value : []);
      setMixerError(mixer.status === "rejected" ? String(mixer.reason) : null);
      if (radio.status === "fulfilled") setBluetooth(radio.value);
      else setBluetooth((current) => current ?? { adapters: [], devices: [], unavailable: String(radio.reason) });
      setDevicesRead(true);
    };
    const stop = startVisiblePolling(tick, DEVICE_TICK_MS);
    return () => { live = false; stop(); };
  }, []);

  const setStreamVolume = (id: number, volume: number) => {
    setStreams((current) => current.map((row) => (row.id === id ? { ...row, volume } : row)));
    void ipc.audioStreamVolume(id, volume).catch((reason) => setError(String(reason)));
  };

  const toggleStreamMute = async (id: number) => {
    try {
      const muted = await ipc.audioStreamToggleMute(id);
      setStreams((current) => current.map((row) => (row.id === id ? { ...row, muted } : row)));
      setError(null);
    } catch (reason) { setError(String(reason)); }
  };

  const toggleMic = async () => {
    try { setMic(await ipc.microphoneToggle()); setError(null); }
    catch (reason) { setError(String(reason)); }
  };

  const changeSort = (key: SortKey) => setSort((current) => ({ key, ascending: current.key === key ? !current.ascending : key === "name" }));
  const rows = useMemo(() => [...processes].sort((a, b) => {
    const direction = sort.ascending ? 1 : -1;
    if (sort.key === "name") return a.name.localeCompare(b.name) * direction;
    return ((sort.key === "cpu" ? a.cpuPercent - b.cpuPercent : a.memoryBytes - b.memoryBytes) * direction);
  }), [processes, sort]);
  const totalAppMemory = processes.reduce((sum, process) => sum + process.memoryBytes, 0);

  const quit = async () => {
    if (!quitting) return;
    const process = quitting;
    setQuitBusy(true);
    try {
      await ipc.systemQuitProcess(process.pid);
      setProcesses((current) => current.filter((row) => row.pid !== process.pid));
      setError(null);
    } catch (reason) { setError(String(reason)); }
    finally { setQuitBusy(false); setQuitting(null); }
  };

  const retry = useCallback(() => { setSnapshotError(null); setAttempt((value) => value + 1); }, []);

  const header = (
    <PageHeader
      title="System"
      subtitle={snapshot ? `${snapshot.distribution} · up ${countdown(snapshot.uptimeSecs)}` : "This computer"}
      actions={
        <Button
          icon={mic?.muted ? <MicOff strokeWidth={STROKE} /> : <Mic strokeWidth={STROKE} />}
          disabled={!mic}
          onClick={() => void toggleMic()}
        >
          {mic ? (mic.muted ? "Unmute microphone" : "Mute microphone") : "Microphone unavailable"}
        </Button>
      }
      accessory={<Tabs label="System views" items={VIEWS} value={view} onChange={setView} />}
    />
  );

  return (
    <Page header={header} className="sys-page">
      {error && (
        <Notice tone="critical" title="That did not work" actions={<Button size="small" onClick={() => setError(null)}>Dismiss</Button>}>
          {error}
        </Notice>
      )}

      {view === "overview" && (
        !snapshot
          ? snapshotError
            ? <Notice tone="critical" title="This computer could not be read" actions={<Button size="small" onClick={retry}>Retry</Button>}>{snapshotError}</Notice>
            : <OverviewSkeleton />
          : <Overview snapshot={snapshot} />
      )}

      {view === "apps" && (
        <Section
          title={`${processes.length} running app${processes.length === 1 ? "" : "s"}`}
          description={`${bytes(totalAppMemory)} resident · current user · updates live`}
        >
          <Card padding="none" className="sys-process-card">
            {!processesRead
              ? <div className="sys-card-pad"><SkeletonRows rows={6} label="Reading running apps" /></div>
              : rows.length === 0
                ? <EmptyState size="compact" title="No running apps">Apps you start appear here within a couple of seconds.</EmptyState>
                : (
                  <Table label="Running apps" className="sys-process-table">
                    <thead>
                      <tr>
                        <SortHeader label="App" sortKey="name" current={sort} onClick={changeSort} />
                        <th className="sys-col-pid">PID</th>
                        <SortHeader label="CPU" sortKey="cpu" current={sort} onClick={changeSort} numeric />
                        <SortHeader label="Memory" sortKey="memory" current={sort} onClick={changeSort} numeric />
                        <th className="sys-col-action"><span className="ui-visually-hidden">Actions</span></th>
                      </tr>
                    </thead>
                    <tbody>
                      {rows.slice(0, PROCESS_LIMIT).map((process) => (
                        <tr key={process.pid}>
                          <td>
                            <div className="sys-process-name">
                              <strong>{process.name}</strong>
                              <span className="ui-mono ui-selectable" title={process.executable ?? ""}>{process.executable || `PID ${process.pid}`}</span>
                            </div>
                          </td>
                          <td className="sys-col-pid ui-mono muted">{process.pid}</td>
                          <td className={["num", process.cpuPercent > 25 ? "sys-hot" : ""].join(" ")}>
                            {process.cpuPercent >= 10 ? process.cpuPercent.toFixed(0) : process.cpuPercent.toFixed(1)}%
                          </td>
                          <td className="num">{bytes(process.memoryBytes)}</td>
                          <td className="sys-col-action">
                            <IconButton size="small" label={`Quit ${process.name}`} tooltip="Quit this app" icon={<X strokeWidth={STROKE} />} onClick={() => setQuitting(process)} />
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </Table>
                )}
          </Card>
        </Section>
      )}

      {view === "devices" && (
        <Columns ratio="wide-start">
          <Card>
            <CardHeader
              title="Audio mixer"
              subtitle={streams.length ? `${streams.length} stream${streams.length === 1 ? "" : "s"}` : "Per application"}
            />
            {!devicesRead
              ? <SkeletonRows rows={3} label="Reading the mixer" />
              : mixerError
                ? <Notice tone="warning" title="The mixer cannot be read" compact>{mixerError}</Notice>
                : streams.length === 0
                  ? <EmptyState size="compact" icon={<Volume2 strokeWidth={STROKE} />} title="Nothing is playing or recording">Apps appear here the moment they open an audio stream.</EmptyState>
                  : (
                    <List label="Audio streams" inset={false}>
                      {streams.map((stream) => (
                        <ListRow
                          key={stream.id}
                          className="sys-mixer-row"
                          leading={
                            <IconButton
                              size="small"
                              label={`${stream.muted ? "Unmute" : "Mute"} ${streamLabel(stream)}`}
                              tooltip={stream.muted ? "Unmute" : "Mute"}
                              pressed={stream.muted}
                              icon={stream.muted ? <VolumeX strokeWidth={STROKE} /> : <Volume2 strokeWidth={STROKE} />}
                              onClick={() => void toggleStreamMute(stream.id)}
                            />
                          }
                          title={streamLabel(stream)}
                          subtitle={stream.muted ? `${stream.direction === "capture" ? "Recording" : "Playing"} · muted` : stream.direction === "capture" ? "Recording" : "Playing"}
                          trailing={
                            <div className="sys-mixer-control">
                              <Slider
                                label={`${streamLabel(stream)} volume`}
                                value={Math.round(stream.volume * 100)}
                                disabled={stream.muted}
                                onChange={(value) => setStreamVolume(stream.id, value / 100)}
                              />
                              <span className="sys-mixer-value ui-num">{Math.round(stream.volume * 100)}%</span>
                            </div>
                          }
                        />
                      ))}
                    </List>
                  )}
          </Card>

          <Card>
            <CardHeader title="Bluetooth" subtitle={bluetooth ? bluetoothSummary(bluetooth) : "Reading…"} />
            {!bluetooth
              ? <SkeletonRows rows={3} label="Reading Bluetooth" />
              : bluetooth.unavailable
                ? <Notice tone="warning" title="Bluetooth cannot be read" compact>{bluetooth.unavailable}</Notice>
                : bluetooth.devices.length === 0
                  ? <EmptyState size="compact" icon={<Bluetooth strokeWidth={STROKE} />} title="No paired devices">Pair one from Ubuntu&apos;s Settings and it appears here.</EmptyState>
                  : (
                    <List label="Bluetooth devices" inset={false}>
                      {bluetooth.devices.map((device) => (
                        <ListRow
                          key={device.address}
                          title={device.name}
                          subtitle={<span className="ui-mono">{device.address}</span>}
                          meta={device.batteryPercent !== null ? `${device.batteryPercent}%` : undefined}
                          trailing={<Badge tone={device.connected ? "good" : "neutral"} dot={device.connected}>{device.connected ? "Connected" : device.paired ? "Paired" : "Seen"}</Badge>}
                        />
                      ))}
                    </List>
                  )}
          </Card>
        </Columns>
      )}

      {view === "cleaner" && <CleanerPane />}

      <ConfirmDialog
        open={quitting !== null}
        onClose={() => { if (!quitBusy) setQuitting(null); }}
        onConfirm={() => void quit()}
        title={quitting ? `Quit ${quitting.name}?` : "Quit app?"}
        message="The process receives a normal terminate request, so unsaved work may be lost."
        confirmLabel="Quit app"
        destructive
        busy={quitBusy}
      />
    </Page>
  );
}

function Overview({ snapshot }: { snapshot: SystemSnapshot }) {
  const memoryPercent = snapshot.memory.totalBytes > 0 ? snapshot.memory.usedBytes / snapshot.memory.totalBytes * 100 : 0;
  const root = snapshot.disks.find((disk) => disk.mountPoint === "/") ?? snapshot.disks[0];
  const rootPercent = root ? usedPercent(root.totalBytes, root.availableBytes) : 0;
  const hottest = snapshot.temperatures.reduce<SystemSnapshot["temperatures"][number] | null>(
    (best, reading) => (best === null || reading.celsius > best.celsius ? reading : best),
    null,
  );

  return (
    <>
      <StatGrid min={170}>
        <MeterStat label="CPU" value={snapshot.cpu.usagePercent} note={`${snapshot.cpu.logicalCores} threads`} />
        <MeterStat label="Memory" value={memoryPercent} note={`${bytes(snapshot.memory.usedBytes)} of ${bytes(snapshot.memory.totalBytes)}`} />
        {root && <MeterStat label={`Disk ${root.mountPoint}`} value={rootPercent} note={`${bytes(root.availableBytes)} free`} warnAt={80} criticalAt={90} />}
        {hottest
          ? <Stat label="Hottest sensor" value={hottest.celsius.toFixed(0)} unit="°C" tone={temperatureTone(hottest.celsius, hottest.criticalCelsius)} note={temperatureNote(hottest)} />
          : <Stat label="Load" value={snapshot.loadAverage[0].toFixed(2)} note={`${snapshot.loadAverage[1].toFixed(2)} · ${snapshot.loadAverage[2].toFixed(2)}`} />}
      </StatGrid>

      <Columns>
        <Card>
          <CardHeader title="Storage" subtitle={`${snapshot.disks.length} volume${snapshot.disks.length === 1 ? "" : "s"}`} />
          {snapshot.disks.length === 0
            ? <EmptyState size="compact" title="No volumes reported">The kernel listed no mounted filesystems.</EmptyState>
            : (
              <div className="sys-disks">
                {snapshot.disks.map((disk) => {
                  const used = usedPercent(disk.totalBytes, disk.availableBytes);
                  const tone = thresholdTone(used, 80, 90);
                  return (
                    <div className="sys-disk" key={disk.mountPoint}>
                      <div className="sys-disk-head">
                        <span className="sys-disk-mount ui-mono ui-selectable">{disk.mountPoint}</span>
                        <span className="sys-disk-value ui-num">{Math.round(used)}%</span>
                      </div>
                      <ProgressBar value={used / 100} tone={tone === "neutral" ? "accent" : tone} size="thin" label={`${disk.mountPoint} used`} />
                      <span className="sys-disk-note ui-num">
                        {disk.fileSystem} · {bytes(disk.totalBytes - disk.availableBytes)} of {bytes(disk.totalBytes)}
                        {disk.removable ? " · removable" : ""}
                        {tone !== "neutral" ? ` · ${tone === "critical" ? "almost full" : "filling up"}` : ""}
                      </span>
                    </div>
                  );
                })}
              </div>
            )}
        </Card>

        <Card>
          <CardHeader title="Hardware" subtitle={<span className="ui-mono">{snapshot.hostName || "This computer"}</span>} />
          <KeyValueList
            items={[
              { label: "Processor", value: snapshot.cpu.brand || `${snapshot.cpu.logicalCores}-thread CPU` },
              { label: "Kernel", value: snapshot.kernel || "Linux", mono: true },
              { label: "Load average", value: snapshot.loadAverage.map((value) => value.toFixed(2)).join(" · ") },
              ...(snapshot.battery ? [{ label: "Battery", value: `${snapshot.battery.percent.toFixed(0)}%${snapshot.battery.charging ? " · charging" : ""}` }] : []),
            ]}
          />
        </Card>
      </Columns>

      {snapshot.temperatures.length > 0 && (
        <Section title="Sensors">
          <List label="Temperature sensors">
            {snapshot.temperatures.map((reading) => {
              const tone = temperatureTone(reading.celsius, reading.criticalCelsius);
              return (
                <ListRow
                  key={reading.label}
                  title={reading.label}
                  subtitle={reading.criticalCelsius !== null ? `Critical at ${reading.criticalCelsius.toFixed(0)} °C` : undefined}
                  meta={<span className="sys-sensor-value">{reading.celsius.toFixed(1)} °C</span>}
                  trailing={tone !== "neutral" ? <Badge tone={tone}>{tone === "critical" ? "Hot" : "Warm"}</Badge> : undefined}
                />
              );
            })}
          </List>
        </Section>
      )}
    </>
  );
}

function OverviewSkeleton() {
  return (
    <>
      <StatGrid min={170}>
        {[0, 1, 2, 3].map((index) => (
          <div className="ui-stat" key={index} role={index === 0 ? "status" : undefined} aria-label={index === 0 ? "Reading this computer" : undefined}>
            <Skeleton width={64} height={11} />
            <Skeleton width={72} height={26} />
            <Skeleton width="80%" height={9} />
          </div>
        ))}
      </StatGrid>
      <Columns>
        <Card><SkeletonRows rows={3} /></Card>
        <Card><SkeletonRows rows={3} /></Card>
      </Columns>
    </>
  );
}

function MeterStat({ label, value, note, warnAt = 60, criticalAt = 85 }: { label: string; value: number; note: string; warnAt?: number; criticalAt?: number }) {
  const tone = thresholdTone(value, warnAt, criticalAt);
  return (
    <Stat
      className="sys-meter"
      label={label}
      value={Math.round(value)}
      unit="%"
      tone={tone}
      note={
        <>
          <ProgressBar value={Math.max(0.01, value / 100)} tone={tone === "neutral" ? "accent" : tone} size="thin" label={`${label} usage`} />
          <span>{tone === "neutral" ? note : `${tone === "critical" ? "Very high" : "High"} · ${note}`}</span>
        </>
      }
    />
  );
}

function SortHeader({ label, sortKey, current, onClick, numeric = false }: { label: string; sortKey: SortKey; current: { key: SortKey; ascending: boolean }; onClick: (key: SortKey) => void; numeric?: boolean }) {
  const active = current.key === sortKey;
  return (
    <th className={numeric ? "num" : undefined} aria-sort={active ? (current.ascending ? "ascending" : "descending") : "none"}>
      <button type="button" className="sys-sort" data-active={active || undefined} onClick={() => onClick(sortKey)}>
        <span>{label}</span>
        {active && (current.ascending ? <ArrowUp strokeWidth={STROKE} aria-hidden="true" /> : <ArrowDown strokeWidth={STROKE} aria-hidden="true" />)}
      </button>
    </th>
  );
}

function usedPercent(total: number, available: number) {
  return total > 0 ? (total - available) / total * 100 : 0;
}

function thresholdTone(value: number, warnAt: number, criticalAt: number): Tone {
  if (value >= criticalAt) return "critical";
  if (value >= warnAt) return "warning";
  return "neutral";
}

function temperatureTone(celsius: number, critical: number | null): Tone {
  const limit = critical ?? 95;
  if (celsius >= limit * 0.9) return "critical";
  if (celsius >= limit * 0.8) return "warning";
  return "neutral";
}

function temperatureNote(reading: { label: string; celsius: number; criticalCelsius: number | null }) {
  const tone = temperatureTone(reading.celsius, reading.criticalCelsius);
  return tone === "neutral" ? reading.label : `${tone === "critical" ? "Hot" : "Warm"} · ${reading.label}`;
}

function streamLabel(stream: AudioStream) {
  const name = stream.mediaName;
  if (!name || name === stream.application || ["Playback", "playback", "RecordStream", "record"].includes(name)) return stream.application;
  return `${stream.application}: ${name}`;
}

function bluetoothSummary(state: BluetoothState) {
  if (state.unavailable) return "Unavailable";
  if (!state.adapters.some((adapter) => adapter.powered)) return "Off";
  const connected = state.devices.filter((device) => device.connected).length;
  return connected === 0 ? "On" : `${connected} connected`;
}
