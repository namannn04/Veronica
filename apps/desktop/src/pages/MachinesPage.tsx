import { useCallback, useEffect, useState } from "react";

import { ipc } from "../lib/ipc";
import { bytes, countdown, percent } from "../lib/format";
import type { MachineReport } from "../lib/types";

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
  const reach =
    machine.reach.kind === "local" ? "this computer" : `ssh ${machine.reach.target}`;

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
