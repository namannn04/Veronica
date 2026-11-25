import {
  Badge,
  FormGroup,
  FormRow,
  KeyValueList,
  Skeleton,
  SkeletonText,
  Stack,
  type Tone,
} from "@veronica/ui";
import type { CapabilityState, Diagnostics } from "../lib/types";

const STATE_BADGES: Record<string, { tone: Tone; label: string }> = {
  available: { tone: "good", label: "Available" },
  permissionRequired: { tone: "warning", label: "Needs permission" },
  integrationRequired: { tone: "warning", label: "Needs integration" },
  unsupported: { tone: "critical", label: "Unsupported" },
};

function capabilityLabel(id: string): string {
  const spaced = id
    .replace(/([A-Z])/g, " $1")
    .toLowerCase()
    .trim();
  return spaced.charAt(0).toUpperCase() + spaced.slice(1);
}

function presence(value: boolean) {
  return value ? "Present" : "Absent";
}

export function DiagnosticsPage({ diagnostics }: { diagnostics: Diagnostics | null }) {
  if (!diagnostics) {
    return (
      <FormGroup title="Capabilities">
        {[0, 1, 2, 3, 4].map((slot) => (
          <FormRow
            key={slot}
            label={<SkeletonText width={120 + ((slot * 37) % 80)} />}
            control={<Skeleton width={64} height={20} radius={10} />}
          />
        ))}
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
      <FormGroup
        title="Capabilities"
        description={`${available} of ${entries.length} available in this session. Each feature talks to one of these.`}
      >
        {entries.map(([id, state]) => (
          <FormRow
            key={id}
            label={capabilityLabel(id)}
            description={"reason" in state ? state.reason : undefined}
            control={<StateBadge state={state} />}
          />
        ))}
      </FormGroup>

      <FormGroup
        title="Session"
        description="Which session is running decides whether a feature can work at all."
      >
        <KeyValueList
          className="settings-kv"
          items={sessionRows.map(([label, value]) => ({ label, value }))}
        />
      </FormGroup>

      <FormGroup title="Directories" description="Where Veronica keeps its configuration and data.">
        <KeyValueList
          className="settings-kv"
          items={directoryRows.map(([label, value]) => ({ label, value, mono: true }))}
        />
      </FormGroup>
    </Stack>
  );
}

function StateBadge({ state }: { state: CapabilityState }) {
  const badge = STATE_BADGES[state.state] ?? STATE_BADGES.unsupported;
  return <Badge tone={badge.tone}>{badge.label}</Badge>;
}
