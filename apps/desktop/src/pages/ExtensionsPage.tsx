import { useEffect, useMemo, useState, type ReactNode } from "react";
import {
  ChartColumn,
  Cpu,
  Diamond,
  NotebookPen,
  PanelTop,
  SquareTerminal,
  Target,
  Wrench,
  Blocks,
  Brain,
  Calendar,
  ClipboardList,
  Columns3,
  Contrast,
  Database,
  Download,
  FileSearch,
  Gauge,
  GitBranch,
  Hourglass,
  Inbox,
  Keyboard,
  Laptop,
  MicOff,
  Music,
  Package,
  Pipette,
  Server,
  Smile,
  Theater,
  ToggleRight,
  type LucideIcon,
} from "lucide-react";

import { ipc } from "../lib/ipc";
import { NAV_ITEMS } from "../lib/navigation";
import { installNotice, type InstallNotice } from "../lib/toolInstall";
import type { Diagnostics, ExtensionGroup, ExtensionReport, ToolReport, ToolSurvey } from "../lib/types";
import {
  Badge,
  Button,
  Card,
  EmptyState,
  List,
  ListRow,
  Page,
  PageHeader,
  SearchField,
  Section,
  Segmented,
  SkeletonRows,
  Stack,
  Switch,
  Toolbar,
} from "../ui";
import "./Extensions.css";

type GroupFilter = ExtensionGroup | "all";

const FILTERS: { value: GroupFilter; label: string }[] = [
  { value: "all", label: "All" },
  { value: "agent", label: "Agent" },
  { value: "system", label: "System" },
  { value: "media", label: "Media" },
  { value: "utilities", label: "Utilities" },
];

const GROUP_TITLES: Record<ExtensionGroup, string> = {
  agent: "Agents",
  system: "System",
  media: "Media",
  utilities: "Utilities",
};

const GROUP_ORDER: ExtensionGroup[] = ["agent", "system", "media", "utilities"];

const ICONS: Record<string, LucideIcon> = {
  hourglass: Hourglass,
  "bar-chart-3": ChartColumn,
  "columns-3": Columns3,
  "git-branch": GitBranch,
  "file-search": FileSearch,
  "toggle-right": ToggleRight,
  package: Package,
  server: Server,
  database: Database,
  brain: Brain,
  gauge: Gauge,
  "mic-off": MicOff,
  laptop: Laptop,
  music: Music,
  calendar: Calendar,
  inbox: Inbox,
  "clipboard-list": ClipboardList,
  keyboard: Keyboard,
  contrast: Contrast,
  theater: Theater,
  smile: Smile,
  pipette: Pipette,
  chart: ChartColumn,
  terminal: SquareTerminal,
  diamond: Diamond,
  target: Target,
  cpu: Cpu,
  wrench: Wrench,
  eyedropper: Pipette,
  search: FileSearch,
  note: NotebookPen,
  mic: MicOff,
  notch: PanelTop,
};

type ToolNotice = InstallNotice | { tone: "critical"; text: string };

function capabilityLabel(id: string): string {
  const spaced = id.replace(/([A-Z])/g, " $1").toLowerCase().trim();
  return spaced.charAt(0).toUpperCase() + spaced.slice(1);
}

export function ExtensionsPage({ diagnostics, onChanged }: { diagnostics: Diagnostics | null; onChanged: () => void }) {
  const [group, setGroup] = useState<GroupFilter>("all");
  const [query, setQuery] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [toggleErrors, setToggleErrors] = useState<Record<string, string>>({});
  const [installing, setInstalling] = useState<string | null>(null);
  const [installNotices, setInstallNotices] = useState<Record<string, ToolNotice>>({});
  const [survey, setSurvey] = useState<ToolSurvey | null>(null);

  useEffect(() => {
    void ipc.toolsReadiness().then(setSurvey).catch(() => setSurvey(null));
  }, []);

  const toolsById = useMemo(() => {
    const index = new Map<string, ToolReport>();
    for (const tool of survey?.tools ?? []) index.set(tool.id, tool);
    return index;
  }, [survey]);

  const entries = diagnostics?.extensions ?? [];

  const shown = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return entries.filter((entry) => {
      const matchesGroup = group === "all" || entry.group === group;
      const matchesQuery = needle === "" || entry.title.toLowerCase().includes(needle) || entry.subtitle.toLowerCase().includes(needle);
      return matchesGroup && matchesQuery;
    });
  }, [entries, group, query]);

  const sections = GROUP_ORDER.map((id) => ({ id, items: shown.filter((entry) => entry.group === id) })).filter((section) => section.items.length > 0);

  const enabledCount = entries.filter((entry) => entry.enabled).length;

  const toggle = async (entry: ExtensionReport) => {
    setBusy(entry.id);
    setToggleErrors(({ [entry.id]: _cleared, ...rest }) => rest);
    try {
      await ipc.settingsSet(entry.defaultsKey, !entry.enabled);
      onChanged();
    } catch (reason) {
      setToggleErrors((current) => ({ ...current, [entry.id]: String(reason) }));
    } finally {
      setBusy(null);
    }
  };

  const install = async (tool: ToolReport) => {
    setInstalling(tool.id);
    setInstallNotices(({ [tool.id]: _cleared, ...rest }) => rest);
    try {
      const outcome = await ipc.toolsInstall(tool.id);
      setInstallNotices((current) => ({ ...current, [tool.id]: installNotice(tool.displayName, outcome) }));
      await ipc.toolsReadiness().then(setSurvey).catch(() => {});
    } catch (reason) {
      setInstallNotices((current) => ({ ...current, [tool.id]: { tone: "critical", text: String(reason) } }));
    } finally {
      setInstalling(null);
    }
  };

  let body: ReactNode;
  if (diagnostics === null) {
    body = (
      <Card>
        <SkeletonRows rows={6} label="Reading the extension catalogue" />
      </Card>
    );
  } else if (entries.length === 0) {
    body = (
      <EmptyState icon={<Blocks />} title="No extensions found">
        The catalogue came back empty. Run <code className="ui-mono">vr diagnose</code> to see what Veronica found.
      </EmptyState>
    );
  } else if (shown.length === 0) {
    body = (
      <EmptyState
        icon={<Blocks />}
        title="Nothing matches"
        action={
          <Button
            onClick={() => {
              setQuery("");
              setGroup("all");
            }}
          >
            Show all extensions
          </Button>
        }
      >
        No extension matches that search in this category.
      </EmptyState>
    );
  } else {
    body = sections.map((section) => (
      <Section
        key={section.id}
        title={GROUP_TITLES[section.id]}
        description={`${section.items.filter((entry) => entry.enabled).length} of ${section.items.length} on`}
      >
        <List label={GROUP_TITLES[section.id]}>
          {section.items.map((entry) => (
            <ExtensionRow
              key={entry.id}
              entry={entry}
              diagnostics={diagnostics}
              unmet={(survey?.unmet[entry.id] ?? []).map((id) => toolsById.get(id)).filter((tool): tool is ToolReport => tool !== undefined)}
              notices={[...toolsById.values()]
                .filter((tool) => tool.wantedBy.includes(entry.id) && installNotices[tool.id])
                .map((tool) => ({ tool, notice: installNotices[tool.id] }))}
              toggleError={toggleErrors[entry.id]}
              busy={busy === entry.id}
              installing={installing}
              onToggle={() => void toggle(entry)}
              onInstall={(tool) => void install(tool)}
            />
          ))}
        </List>
      </Section>
    ));
  }

  return (
    <Page
      header={
        <PageHeader
          title="Extensions"
          subtitle={
            diagnostics && entries.length > 0
              ? `${enabledCount} of ${entries.length} on. Turning one off stops its timers and background work.`
              : "Turning one off stops its timers and background work."
          }
        />
      }
    >
      <Toolbar
        className="exts-toolbar"
        leading={<SearchField value={query} onChange={setQuery} placeholder="Search extensions" />}
        trailing={<Segmented label="Category" options={FILTERS} value={group} onChange={setGroup} />}
      />
      <Stack>{body}</Stack>
    </Page>
  );
}

function ExtensionRow({
  entry,
  diagnostics,
  unmet,
  notices,
  toggleError,
  busy,
  installing,
  onToggle,
  onInstall,
}: {
  entry: ExtensionReport;
  diagnostics: Diagnostics | null;
  unmet: ToolReport[];
  notices: { tool: ToolReport; notice: ToolNotice }[];
  toggleError: string | undefined;
  busy: boolean;
  installing: string | null;
  onToggle: () => void;
  onInstall: (tool: ToolReport) => void;
}) {
  const Icon = ICONS[entry.icon] ?? Blocks;
  const navIcon = NAV_ITEMS.find((item) => item.extensionId === entry.id)?.icon;
  const unavailable = entry.availability === "unavailable";
  const missing = entry.missing && entry.missing.length > 0 ? entry.missing : null;
  const hasDetail = missing !== null || unmet.length > 0 || toggleError !== undefined || notices.length > 0;

  return (
    <ListRow
      className={hasDetail ? "exts-row exts-row-detailed" : "exts-row"}
      leading={
        <span className="exts-icon" data-enabled={entry.enabled || undefined}>
          {navIcon ?? <Icon size={16} strokeWidth={1.75} />}
        </span>
      }
      title={
        <span className="exts-title">
          <span className="exts-name">{entry.title}</span>
          <AvailabilityBadge entry={entry} unmetTools={unmet.length > 0} />
        </span>
      }
      subtitle={
        <span className="exts-subtitle">
          <span className="exts-summary">{entry.subtitle}</span>
          {missing && (
            <span className="exts-note">
              Needs {missing.map(capabilityLabel).join(", ")}. {reasonFor(diagnostics, missing[0])}
            </span>
          )}
          {unmet.length > 0 && (
            <span className="exts-tools">
              <span className="exts-note">Needs {unmet.map((tool) => tool.displayName).join(" or ")}.</span>
              {unmet.map((tool) => (
                <span className="exts-tool" key={tool.id}>
                  <span className="exts-note">{toolNote(tool)}</span>
                  <Button
                    size="small"
                    icon={<Download />}
                    busy={installing === tool.id}
                    disabled={installing !== null}
                    onClick={() => onInstall(tool)}
                  >
                    {installing === tool.id ? "Installing…" : `Install ${tool.displayName}`}
                  </Button>
                </span>
              ))}
            </span>
          )}
          {toggleError && (
            <span className="exts-note exts-note-critical" role="alert">
              Could not turn {entry.title} {entry.enabled ? "off" : "on"}: {toggleError}
            </span>
          )}
          {notices.map(({ tool, notice }) => (
            <span className={`exts-note exts-note-${notice.tone}`} role="status" aria-live="polite" key={tool.id}>
              {notice.text}
            </span>
          ))}
        </span>
      }
      trailing={<Switch label={`${entry.enabled ? "Disable" : "Enable"} ${entry.title}`} checked={entry.enabled} disabled={unavailable || busy} onChange={onToggle} />}
    />
  );
}

function AvailabilityBadge({ entry, unmetTools }: { entry: ExtensionReport; unmetTools: boolean }) {
  if (entry.availability === "unavailable") return <Badge tone="critical">Unavailable</Badge>;
  if (unmetTools) return <Badge tone="warning">Needs setup</Badge>;
  if (entry.availability === "degraded") return <Badge tone="warning">Partial</Badge>;
  return null;
}

function toolNote(tool: ToolReport): string {
  return tool.state === "error" ? tool.detail : `${tool.why} ${tool.instruction}`;
}

function reasonFor(diagnostics: Diagnostics | null, capability: string): string {
  const state = diagnostics?.capabilities.states[capability];
  if (!state) return "";
  return "reason" in state ? state.reason : "";
}
