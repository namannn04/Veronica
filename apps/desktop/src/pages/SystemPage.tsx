import { useEffect, useMemo, useState } from "react";

import { CleanerPane } from "./CleanerPane";
import { ipc } from "../lib/ipc";
import { bytes, countdown, percent } from "../lib/format";
import type { AudioStream, BluetoothState, RunningProcess, SystemSnapshot, VolumeState } from "../lib/types";

const TICK_MS = 2000;
// The mixer and BlueZ change on human timescales, not per-frame, and each read
// spawns a process or a bus round trip, so they refresh far less often than the
// metrics above them.
const DEVICE_TICK_MS = 5000;
type SortKey = "name" | "cpu" | "memory";

export function SystemPage() {
  const [snapshot, setSnapshot] = useState<SystemSnapshot | null>(null);
  const [processes, setProcesses] = useState<RunningProcess[]>([]);
  const [mic, setMic] = useState<VolumeState | null>(null);
  const [streams, setStreams] = useState<AudioStream[]>([]);
  const [bluetooth, setBluetooth] = useState<BluetoothState | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [sort, setSort] = useState<{ key: SortKey; ascending: boolean }>({ key: "cpu", ascending: false });

  useEffect(() => {
    let live = true;
    const tick = async () => {
      const [system, apps] = await Promise.allSettled([ipc.systemSnapshot(), ipc.systemProcesses()]);
      if (!live) return;
      if (system.status === "fulfilled") setSnapshot(system.value);
      if (apps.status === "fulfilled") setProcesses(apps.value);
    };
    void tick();
    const timer = setInterval(tick, TICK_MS);
    return () => { live = false; clearInterval(timer); };
  }, []);

  useEffect(() => { void ipc.microphoneState().then(setMic).catch((reason) => setError(String(reason))); }, []);

  useEffect(() => {
    let live = true;
    const tick = async () => {
      const [mixer, radio] = await Promise.allSettled([ipc.audioStreams(), ipc.bluetoothState()]);
      if (!live) return;
      // A missing pw-dump or BlueZ is reported by the card itself, not as a
      // page-level error that would sit above working metrics.
      setStreams(mixer.status === "fulfilled" ? mixer.value : []);
      if (radio.status === "fulfilled") setBluetooth(radio.value);
    };
    void tick();
    const timer = setInterval(tick, DEVICE_TICK_MS);
    return () => { live = false; clearInterval(timer); };
  }, []);

  // The slider must feel immediate, so the row moves on input and the reported
  // value is only overwritten by the next refresh.
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

  const quit = async (process: RunningProcess) => {
    if (!window.confirm(`Quit ${process.name}?\n\nThe process will receive a normal terminate request.`)) return;
    try { await ipc.systemQuitProcess(process.pid); setProcesses((current) => current.filter((row) => row.pid !== process.pid)); setError(null); }
    catch (reason) { setError(String(reason)); }
  };

  if (!snapshot) return <div className="empty"><div className="empty-glyph pulse">⌁</div><h3>Reading this computer…</h3></div>;
  const memoryPercent = snapshot.memory.totalBytes > 0 ? snapshot.memory.usedBytes / snapshot.memory.totalBytes * 100 : 0;

  return <div className="system-page">
    <div className="page-head edith-head"><div><h1>System</h1><div className="page-sub">{snapshot.distribution} · up {countdown(snapshot.uptimeSecs)}</div></div><button className={`mic-button ${mic?.muted ? "muted" : ""}`} onClick={() => void toggleMic()}><span>{mic?.muted ? "⊘" : "●"}</span><div><strong>Microphone</strong><small>{mic ? (mic.muted ? "Muted" : "Live") : "Unavailable"}</small></div></button></div>
    {error && <div className="banner error">{error}</div>}

    <div className="system-summary">
      <Summary label="CPU" value={percent(snapshot.cpu.usagePercent)} note={`${snapshot.cpu.logicalCores} threads`} meter={snapshot.cpu.usagePercent} />
      <Summary label="Memory" value={percent(memoryPercent)} note={`${bytes(snapshot.memory.usedBytes)} of ${bytes(snapshot.memory.totalBytes)}`} meter={memoryPercent} />
      <Summary label="Running apps" value={String(processes.length)} note={`${bytes(totalAppMemory)} resident`} />
      <Summary label="Load" value={snapshot.loadAverage[0].toFixed(2)} note={`${snapshot.loadAverage[1].toFixed(2)} · ${snapshot.loadAverage[2].toFixed(2)}`} />
    </div>

    <section className="card process-card"><div className="card-head"><h2>Running apps</h2><span className="card-note">Current user · updates live</span></div><div className="process-table"><div className="process-header"><SortButton label="App" sortKey="name" current={sort} onClick={changeSort} /><SortButton label="CPU" sortKey="cpu" current={sort} onClick={changeSort} /><SortButton label="Memory" sortKey="memory" current={sort} onClick={changeSort} /><span /></div>{rows.length === 0 ? <div className="process-empty">Reading running apps…</div> : rows.slice(0, 40).map((process) => <div className="process-row" key={process.pid}><div className="process-name"><span>{process.name.slice(0, 1).toUpperCase()}</span><div><strong>{process.name}</strong><small title={process.executable ?? ""}>{process.executable || `PID ${process.pid}`}</small></div></div><code className={process.cpuPercent > 25 ? "hot" : ""}>{process.cpuPercent >= 10 ? process.cpuPercent.toFixed(0) : process.cpuPercent.toFixed(1)}%</code><code>{bytes(process.memoryBytes)}</code><button aria-label={`Quit ${process.name}`} title={`Quit ${process.name}`} onClick={() => void quit(process)}>×</button></div>)}</div></section>

    <div className="grid two-col system-detail-grid">
      <section className="card mixer-card">
        <div className="card-head"><h2>Audio mixer</h2><span className="card-note">{streams.length ? `${streams.length} stream${streams.length === 1 ? "" : "s"}` : "per application"}</span></div>
        {streams.length === 0
          ? <p className="quiet">Nothing is playing or recording. Applications appear here the moment they open an audio stream.</p>
          : <div className="mixer-list">{streams.map((stream) => <div className="mixer-row" key={stream.id}>
              <div className="mixer-name"><strong>{streamLabel(stream)}</strong><small>{stream.direction === "capture" ? "Recording" : "Playing"}</small></div>
              <button className={`mixer-mute ${stream.muted ? "muted" : ""}`} aria-label={`${stream.muted ? "Unmute" : "Mute"} ${streamLabel(stream)}`} onClick={() => void toggleStreamMute(stream.id)}>{stream.muted ? "⊘" : "◉"}</button>
              <input type="range" min={0} max={100} value={Math.round(stream.volume * 100)} aria-label={`${streamLabel(stream)} volume`} disabled={stream.muted} onChange={(event) => setStreamVolume(stream.id, Number(event.target.value) / 100)} />
              <code>{Math.round(stream.volume * 100)}%</code>
            </div>)}</div>}
      </section>

      <section className="card"><div className="card-head"><h2>Bluetooth</h2><span className="card-note">{bluetooth ? bluetoothSummary(bluetooth) : "reading…"}</span></div>
        {bluetooth?.unavailable
          ? <p className="quiet">{bluetooth.unavailable}</p>
          : bluetooth && bluetooth.devices.length === 0
            ? <p className="quiet">No paired devices. Pair one from Ubuntu&apos;s Settings; Veronica reports what BlueZ knows.</p>
            : <div className="list bluetooth-list">{bluetooth?.devices.map((device) => <div className="list-row" key={device.address}>
                <div><strong>{device.name}</strong><small>{device.address}</small></div>
                <span className={device.connected ? "pill good" : "pill"}>{device.connected ? "Connected" : device.paired ? "Paired" : "Seen"}{device.batteryPercent !== null ? ` · ${device.batteryPercent}%` : ""}</span>
              </div>)}</div>}
      </section>
    </div>

    <CleanerPane />

    <div className="grid two-col system-detail-grid"><section className="card"><div className="card-head"><h2>Storage</h2></div>{snapshot.disks.map((disk) => { const used = disk.totalBytes > 0 ? (disk.totalBytes - disk.availableBytes) / disk.totalBytes * 100 : 0; return <div className="disk-row" key={disk.mountPoint}><div><strong>{disk.mountPoint}</strong><span>{disk.fileSystem} · {bytes(disk.totalBytes - disk.availableBytes)} of {bytes(disk.totalBytes)}</span></div><b>{percent(used)}</b><div><i style={{ width: `${Math.min(100, used)}%` }} /></div></div>; })}</section><section className="card"><div className="card-head"><h2>Hardware</h2></div><dl className="hardware-list"><div><dt>Processor</dt><dd>{snapshot.cpu.brand || `${snapshot.cpu.logicalCores}-thread CPU`}</dd></div><div><dt>Kernel</dt><dd>{snapshot.kernel || "Linux"}</dd></div><div><dt>Hostname</dt><dd>{snapshot.hostName || "This computer"}</dd></div>{snapshot.temperatures.map((reading) => <div key={reading.label}><dt>{reading.label}</dt><dd>{reading.celsius.toFixed(1)} °C</dd></div>)}{snapshot.battery && <div><dt>Battery</dt><dd>{snapshot.battery.percent.toFixed(0)}%{snapshot.battery.charging ? " · charging" : ""}</dd></div>}</dl></section></div>
  </div>;
}

function Summary({ label, value, note, meter }: { label: string; value: string; note: string; meter?: number }) { return <div className="system-summary-card"><span>{label.toUpperCase()}</span><strong>{value}</strong>{meter !== undefined && <div><i style={{ width: `${Math.min(100, Math.max(1, meter))}%` }} /></div>}<small>{note}</small></div>; }
function SortButton({ label, sortKey, current, onClick }: { label: string; sortKey: SortKey; current: { key: SortKey; ascending: boolean }; onClick: (key: SortKey) => void }) { return <button className={current.key === sortKey ? "active" : ""} onClick={() => onClick(sortKey)}>{label.toUpperCase()} {current.key === sortKey ? (current.ascending ? "↑" : "↓") : ""}</button>; }

/** The application, with the stream name appended only when it adds something —
 *  the same rule `AudioStream::label` applies in the core. */
function streamLabel(stream: AudioStream) {
  const name = stream.mediaName;
  if (!name || name === stream.application || ["Playback", "playback", "RecordStream", "record"].includes(name)) return stream.application;
  return `${stream.application} — ${name}`;
}

function bluetoothSummary(state: BluetoothState) {
  if (state.unavailable) return "unavailable";
  if (!state.adapters.some((adapter) => adapter.powered)) return "off";
  const connected = state.devices.filter((device) => device.connected).length;
  return connected === 0 ? "on" : `${connected} connected`;
}
