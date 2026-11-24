import type { CapabilityState, Diagnostics } from "../lib/types";
import { Badge, FormGroup, FormRow, SkeletonRows, Stack, type Tone } from "../ui";

const STATE_BADGES: Record<string, { tone: Tone; label: string }> = {
  available: { tone: "good", label: "Available" },
  permissionRequired: { tone: "warning", label: "Needs permission" },
  integrationRequired: { tone: "warning", label: "Needs integration" },
  unsupported: { tone: "critical", label: "Unsupported" },
};

function capabilityLabel(id: string): string {
  const spaced = id.replace(/([A-Z])/g, " $1").toLowerCase().trim();
  return spaced.charAt(0).toUpperCase() + spaced.slice(1);
}

function presence(value: boolean) {
  return value ? "Present" : "Absent";
}

export function DiagnosticsPage({ diagnostics }: { diagnostics: Diagnostics | null }) {
  if (!diagnostics) {
    return (
      <FormGroup title="Capabilities">
        <SkeletonRows rows={5} label="Reading the environment" />
      </FormGroup>
    );
  }

  const { session, directories, capabilities } = diagnostics;
  const entries = Object.entries(capabilities.states);
  const available = entries.filter(([, state]) => state.state === "available").length;

  const sessionRows: [string, string][] = [
    ["Version", diagnostics.version],
    ["Session", session.kind],
    ["Window backend", session.toolkitBackend || session.kind],
    ["Desktop", session.desktop || "Unknown"],
    ["Shortcuts portal", presence(session.hasGlobalShortcutsPortal)],
    ["PipeWire", presence(session.hasPipewire)],
    ["Container runtime", presence(session.hasContainerRuntime)],
  ];

  const directoryRows: [string, string][] = [
    ["Config", directories.configuration],
    ["Data", directories.data],
    ["Cache", directories.cache],
    ["State", directories.state],
    ["Runtime", directories.runtime],
  ];

  return (
    <Stack>
      <FormGroup title="Capabilities" description={`${available} of ${entries.length} available in this session. Each feature talks to one of these.`}>
        {entries.map(([id, state]) => (
          <FormRow key={id} label={capabilityLabel(id)} description={"reason" in state ? state.reason : "Ready to use."} control={<StateBadge state={state} />} />
        ))}
      </FormGroup>

      <FormGroup title="Session" description="Which session is running decides whether a feature can work at all.">
        {sessionRows.map(([label, value]) => (
          <FormRow key={label} label={label} control={<span className="settings-value ui-selectable">{value}</span>} />
        ))}
      </FormGroup>

      <FormGroup title="Directories" description="Where Veronica keeps its configuration and data.">
        {directoryRows.map(([label, value]) => (
          <FormRow
            key={label}
            label={label}
            control={
              <span className="settings-value settings-path ui-mono ui-selectable" title={value}>
                {value}
              </span>
            }
          />
        ))}
      </FormGroup>
    </Stack>
  );
}

function StateBadge({ state }: { state: CapabilityState }) {
  const badge = STATE_BADGES[state.state] ?? STATE_BADGES.unsupported;
  return <Badge tone={badge.tone}>{badge.label}</Badge>;
}
