import type { CapabilityState, Diagnostics } from "../lib/types";

const STATE_PILL: Record<string, { className: string; label: string }> = {
  available: { className: "pill good", label: "Available" },
  permissionRequired: { className: "pill warn", label: "Permission" },
  integrationRequired: { className: "pill serious", label: "Integration" },
  unsupported: { className: "pill critical", label: "Unsupported" },
};

function label(id: string): string {
  const spaced = id.replace(/([A-Z])/g, " $1").toLowerCase().trim();
  return spaced.charAt(0).toUpperCase() + spaced.slice(1);
}

/**
 * What this machine can actually do.
 *
 * Edith's equivalent lists macOS permission grants. On Ubuntu the interesting
 * facts are which session is running and which service backs each capability,
 * because that is what decides whether a feature can work at all.
 */
export function DiagnosticsPage({ diagnostics }: { diagnostics: Diagnostics | null }) {
  if (!diagnostics) return <div className="empty">Reading the environment…</div>;

  const { session, directories, capabilities } = diagnostics;
  const entries = Object.entries(capabilities.states);
  const available = entries.filter(([, state]) => state.state === "available").length;

  return (
    <>
      <div className="page-head">
        <div>
          <h1>Diagnostics</h1>
          <div className="page-sub">
            {available} of {entries.length} capabilities available on this session
          </div>
        </div>
      </div>

      <div className="grid two-col">
        <section className="card">
          <div className="card-head">
            <h2>Session</h2>
          </div>
          <table>
            <tbody>
              <Row label="Version" value={diagnostics.version} />
              <Row label="Session" value={session.kind} />
              <Row
                label="Window backend"
                value={session.toolkitBackend || session.kind}
              />
              <Row label="Desktop" value={session.desktop || "unknown"} />
              <Row
                label="Shortcuts portal"
                value={session.hasGlobalShortcutsPortal ? "present" : "absent"}
              />
              <Row label="PipeWire" value={session.hasPipewire ? "present" : "absent"} />
              <Row
                label="Container runtime"
                value={session.hasContainerRuntime ? "present" : "absent"}
              />
            </tbody>
          </table>
        </section>

        <section className="card">
          <div className="card-head">
            <h2>Directories</h2>
          </div>
          <table>
            <tbody>
              <Row label="Config" value={directories.configuration} mono />
              <Row label="Data" value={directories.data} mono />
              <Row label="Cache" value={directories.cache} mono />
              <Row label="State" value={directories.state} mono />
              <Row label="Runtime" value={directories.runtime} mono />
            </tbody>
          </table>
        </section>
      </div>

      <section className="card" style={{ marginTop: 12 }}>
        <div className="card-head">
          <h2>Capabilities</h2>
          <span className="card-note">What each feature talks to</span>
        </div>
        <table>
          <thead>
            <tr>
              <th>Capability</th>
              <th>State</th>
              <th>Detail</th>
            </tr>
          </thead>
          <tbody>
            {entries.map(([id, state]) => (
              <tr key={id}>
                <td>{label(id)}</td>
                <td>
                  <StatePill state={state} />
                </td>
                <td className="card-note">{"reason" in state ? state.reason : "Ready to use."}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>
    </>
  );
}

function StatePill({ state }: { state: CapabilityState }) {
  const pill = STATE_PILL[state.state] ?? STATE_PILL.unsupported;
  return <span className={pill.className}>{pill.label}</span>;
}

function Row({ label: name, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <tr>
      <td style={{ color: "var(--ink-muted)", width: 130 }}>{name}</td>
      <td className={mono ? "mono truncate" : "truncate"} title={value}>
        {value}
      </td>
    </tr>
  );
}
