import { useEffect, useState } from "react";

import { ipc } from "../lib/ipc";
import { bytes, countdown, percent } from "../lib/format";
import type { SystemSnapshot, VolumeState } from "../lib/types";

/** Live metrics refresh; slow enough to stay near-idle when the panel is open. */
const TICK_MS = 2000;

export function SystemPage() {
  const [snapshot, setSnapshot] = useState<SystemSnapshot | null>(null);
  const [mic, setMic] = useState<VolumeState | null>(null);
  const [micError, setMicError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    const tick = async () => {
      try {
        const next = await ipc.systemSnapshot();
        if (live) setSnapshot(next);
      } catch {
        // A dropped sample is not worth surfacing; the next tick retries.
      }
    };
    void tick();
    const timer = setInterval(tick, TICK_MS);
    return () => {
      live = false;
      clearInterval(timer);
    };
  }, []);

  useEffect(() => {
    ipc
      .microphoneState()
      .then(setMic)
      .catch((e) => setMicError(String(e)));
  }, []);

  const toggleMic = async () => {
    try {
      setMic(await ipc.microphoneToggle());
      setMicError(null);
    } catch (e) {
      setMicError(String(e));
    }
  };

  if (!snapshot) return <div className="empty">Reading system metrics…</div>;

  const memoryPercent =
    snapshot.memory.totalBytes > 0
      ? (snapshot.memory.usedBytes / snapshot.memory.totalBytes) * 100
      : 0;

  return (
    <>
      <div className="page-head">
        <div>
          <h1>System</h1>
          <div className="page-sub">
            {snapshot.distribution} · up {countdown(snapshot.uptimeSecs)}
          </div>
        </div>
      </div>

      <div className="grid tiles">
        <Meter label="CPU" value={snapshot.cpu.usagePercent} note={`${snapshot.cpu.logicalCores} threads`} />
        <Meter
          label="Memory"
          value={memoryPercent}
          note={`${bytes(snapshot.memory.usedBytes)} of ${bytes(snapshot.memory.totalBytes)}`}
        />
        <div className="card">
          <div className="tile-label">Load</div>
          <div className="tile-value">{snapshot.loadAverage[0].toFixed(2)}</div>
          <div className="tile-note">
            {snapshot.loadAverage[1].toFixed(2)} · {snapshot.loadAverage[2].toFixed(2)}
          </div>
        </div>
        <div className="card">
          <div className="tile-label">Microphone</div>
          <div className="tile-value">{mic ? (mic.muted ? "Muted" : "Live") : "—"}</div>
          <button className="button" style={{ marginTop: 6 }} onClick={toggleMic}>
            {mic?.muted ? "Unmute" : "Mute"}
          </button>
        </div>
      </div>

      {micError && <div className="banner error">{micError}</div>}

      <div className="grid two-col">
        <section className="card">
          <div className="card-head">
            <h2>Disks</h2>
          </div>
          <table>
            <thead>
              <tr>
                <th>Mount</th>
                <th>Type</th>
                <th className="num">Used</th>
                <th className="num">Size</th>
              </tr>
            </thead>
            <tbody>
              {snapshot.disks.map((disk) => (
                <tr key={disk.mountPoint}>
                  <td className="mono truncate">{disk.mountPoint}</td>
                  <td className="card-note">{disk.fileSystem}</td>
                  <td className="num">
                    {percent(
                      disk.totalBytes > 0
                        ? ((disk.totalBytes - disk.availableBytes) / disk.totalBytes) * 100
                        : 0,
                    )}
                  </td>
                  <td className="num">{bytes(disk.totalBytes)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </section>

        <section className="card">
          <div className="card-head">
            <h2>Temperatures</h2>
          </div>
          {snapshot.temperatures.length === 0 ? (
            <p className="card-note">No thermal sensors exposed.</p>
          ) : (
            <table>
              <tbody>
                {snapshot.temperatures.map((reading) => (
                  <tr key={reading.label}>
                    <td className="mono">{reading.label}</td>
                    <td className="num">{reading.celsius.toFixed(1)} °C</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
          {snapshot.battery && (
            <div className="card-note" style={{ marginTop: 10 }}>
              Battery {snapshot.battery.percent.toFixed(0)}%
              {snapshot.battery.charging ? " · charging" : ""}
            </div>
          )}
        </section>
      </div>
    </>
  );
}

function Meter({ label, value, note }: { label: string; value: number; note: string }) {
  // A meter is a status readout, so the band carries a word as well as a colour.
  const status = value >= 85 ? "critical" : value >= 60 ? "warning" : "good";
  return (
    <div className="card">
      <div className="tile-label">{label}</div>
      <div className="tile-value">{percent(value)}</div>
      <div style={{ height: 6, background: "var(--grid)", borderRadius: 3, marginTop: 8 }}>
        <div
          style={{
            width: `${Math.min(100, Math.max(value, 1))}%`,
            height: "100%",
            background: `var(--status-${status})`,
            borderRadius: 3,
          }}
        />
      </div>
      <div className="tile-note">{note}</div>
    </div>
  );
}
