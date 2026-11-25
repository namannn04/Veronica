import {
  Badge,
  Button,
  ButtonGroup,
  Checkbox,
  ConfirmDialog,
  EmptyState,
  FilterChips,
  FormGroup,
  FormRow,
  List,
  ListRow,
  ListSkeleton,
  Notice,
  Page,
  PageHeader,
  ProgressBar,
  SearchField,
  Section,
  Select,
  SkeletonText,
  Stack,
  Switch,
  Toolbar,
} from "@veronica/ui";
import {
  ArrowUpCircle,
  Blocks,
  Brain,
  Calendar,
  ChartColumn,
  ClipboardList,
  Columns3,
  Contrast,
  Cpu,
  Database,
  Diamond,
  Download,
  FileSearch,
  Gauge,
  GitBranch,
  Hourglass,
  Inbox,
  Keyboard,
  Laptop,
  type LucideIcon,
  MicOff,
  Music,
  NotebookPen,
  Package,
  PanelTop,
  Pipette,
  RefreshCw,
  Server,
  Smile,
  SquareTerminal,
  Target,
  Theater,
  ToggleRight,
  Wrench,
} from "lucide-react";
import { type ReactNode, useCallback, useEffect, useMemo, useState } from "react";
import { invoke, ipc, listenEvent } from "../lib/ipc";
import {
  ALL,
  availableActions,
  buildEntries,
  bundledOffer,
  groupLibrary,
  isActive,
  type LibraryAction,
  type LibraryEntry,
  libraryCategories,
  libraryCommands,
  needsConfirmation,
  pendingUpdates,
  permissionSummary,
  watchJobs,
} from "../lib/library";
import { type InstallNotice, installNotice } from "../lib/toolInstall";
import type {
  AutoUpdatePreferences,
  CatalogShelf,
  Diagnostics,
  ExtensionJob,
  InstalledExtension,
  ToolReport,
  ToolSurvey,
} from "../lib/types";
import "./Library.css";

const commands = libraryCommands(invoke);
const INTERVALS = [1, 3, 6, 12, 24];
const ACTION_LABELS: Record<LibraryAction, string> = {
  install: "Install",
  update: "Update",
  enable: "Turn on",
  disable: "Turn off",
  remove: "Remove",
  rollback: "Roll back",
  restart: "Restart",
};

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
type Removal = { entry: LibraryEntry; purge: boolean };

function failure(reason: unknown): string {
  return reason instanceof Error ? reason.message : String(reason);
}

export function LibraryPage({
  diagnostics,
  onChanged,
}: {
  diagnostics: Diagnostics | null;
  onChanged: () => void;
}) {
  const [query, setQuery] = useState("");
  const [category, setCategory] = useState(ALL);
  const [installed, setInstalled] = useState<InstalledExtension[] | null>(null);
  const [shelf, setShelf] = useState<CatalogShelf | null>(null);
  const [shelfError, setShelfError] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [preferences, setPreferences] = useState<AutoUpdatePreferences | null>(null);
  const [jobs, setJobs] = useState<Map<string, ExtensionJob>>(new Map());
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [removal, setRemoval] = useState<Removal | null>(null);
  const [switching, setSwitching] = useState<string | null>(null);
  const [survey, setSurvey] = useState<ToolSurvey | null>(null);
  const [installing, setInstalling] = useState<string | null>(null);
  const [toolNotices, setToolNotices] = useState<Record<string, ToolNotice>>({});

  const load = useCallback(() => {
    void commands.installed().then(setInstalled, () => setInstalled([]));
    void commands.shelf().then(setShelf, (reason) => setShelfError(failure(reason)));
  }, []);

  useEffect(() => {
    load();
    void commands.autoUpdate().then(setPreferences, () => setPreferences(null));
    void ipc.toolsReadiness().then(setSurvey, () => setSurvey(null));
    const changed = listenEvent("extensions-changed", load);
    const stop = watchJobs(invoke, listenEvent, (next, job) => {
      setJobs(next);
      const error = job.phase === "failed" ? job.error : null;
      setErrors(({ [job.extension]: _previous, ...rest }) =>
        error === null ? rest : { ...rest, [job.extension]: error },
      );
    });
    return () => {
      stop();
      void changed.then((unlisten) => unlisten());
    };
  }, [load]);

  const entries = useMemo(
    () => buildEntries(diagnostics?.extensions ?? [], installed ?? [], shelf),
    [diagnostics, installed, shelf],
  );
  const categories = useMemo(() => libraryCategories(shelf, entries), [shelf, entries]);
  const groups = useMemo(
    () => groupLibrary(entries, categories, query, category),
    [entries, categories, query, category],
  );
  const updates = pendingUpdates(entries);
  const toolsById = useMemo(
    () => new Map((survey?.tools ?? []).map((tool) => [tool.id, tool])),
    [survey],
  );

  const clearError = (id: string) => setErrors(({ [id]: _cleared, ...rest }) => rest);

  const run = async (action: LibraryAction, entry: LibraryEntry, purge = false) => {
    clearError(entry.id);
    try {
      await commands.run(action, entry.id, purge);
      if (action === "restart") {
        load();
      }
    } catch (reason) {
      setErrors((current) => ({ ...current, [entry.id]: failure(reason) }));
    }
  };

  const request = (action: LibraryAction, entry: LibraryEntry) => {
    if (needsConfirmation(action)) {
      setRemoval({ entry, purge: false });
    } else {
      void run(action, entry);
    }
  };

  const refresh = async () => {
    setRefreshing(true);
    setShelfError(null);
    try {
      setShelf(await commands.refresh());
      load();
    } catch (reason) {
      setShelfError(failure(reason));
    } finally {
      setRefreshing(false);
    }
  };

  const updateAll = () => {
    for (const entry of entries.filter((item) => updates.includes(item.id))) {
      void run("update", entry);
    }
  };

  const savePreferences = (next: AutoUpdatePreferences) => {
    setPreferences(next);
    void commands
      .setAutoUpdate(next)
      .then(setPreferences, (reason) => setShelfError(failure(reason)));
  };

  const toggleFeature = async (entry: LibraryEntry) => {
    const feature = entry.feature;
    if (feature === undefined) {
      return;
    }
    setSwitching(entry.id);
    clearError(entry.id);
    try {
      await ipc.settingsSet(feature.defaultsKey, !feature.enabled);
      onChanged();
    } catch (reason) {
      setErrors((current) => ({ ...current, [entry.id]: failure(reason) }));
    } finally {
      setSwitching(null);
    }
  };

  const installTool = async (tool: ToolReport) => {
    setInstalling(tool.id);
    try {
      const outcome = await ipc.toolsInstall(tool.id);
      setToolNotices((current) => ({
        ...current,
        [tool.id]: installNotice(tool.displayName, outcome),
      }));
      await ipc.toolsReadiness().then(setSurvey, () => undefined);
    } catch (reason) {
      setToolNotices((current) => ({
        ...current,
        [tool.id]: { tone: "critical", text: failure(reason) },
      }));
    } finally {
      setInstalling(null);
    }
  };

  const chips = categories
    .filter((item) => !item.hidden || item.id === "labs")
    .filter((item) => item.id === "labs" || entries.some((entry) => entry.category === item.id))
    .map((item) => ({ value: item.id, label: item.title }));

  let body: ReactNode;
  if (diagnostics === null || installed === null) {
    body = [4, 5].map((rows) => (
      <Section key={rows} title={<SkeletonText width={72} />}>
        <ListSkeleton rows={rows} trailing={64} label="Reading the extension library" />
      </Section>
    ));
  } else if (groups.length === 0) {
    body = (
      <EmptyState
        icon={<Blocks />}
        title="Nothing matches"
        action={
          <Button
            onClick={() => {
              setQuery("");
              setCategory(ALL);
            }}
          >
            Show everything
          </Button>
        }
      >
        No extension matches that search in this category.
      </EmptyState>
    );
  } else {
    body = groups.map((group) => (
      <Section key={group.id} title={group.title}>
        <List label={group.title}>
          {group.entries.map((entry) => (
            <LibraryRow
              key={entry.id}
              entry={entry}
              diagnostics={diagnostics}
              job={jobs.get(entry.id)}
              error={errors[entry.id]}
              switching={switching === entry.id}
              unmet={(survey?.unmet[entry.id] ?? [])
                .map((id) => toolsById.get(id))
                .filter((tool): tool is ToolReport => tool !== undefined)}
              toolNotices={[...toolsById.values()]
                .filter((tool) => tool.wantedBy.includes(entry.id) && toolNotices[tool.id])
                .map((tool) => ({ tool, notice: toolNotices[tool.id] }))}
              installing={installing}
              onAction={(action) => request(action, entry)}
              onToggleFeature={() => void toggleFeature(entry)}
              onInstallTool={(tool) => void installTool(tool)}
            />
          ))}
        </List>
      </Section>
    ));
  }

  const installedCount = (installed ?? []).length + (diagnostics?.extensions.length ?? 0);

  return (
    <Page
      header={
        <PageHeader
          title="Library"
          subtitle={`${installedCount} installed${updates.length > 0 ? `, ${updates.length} with updates` : ""}. Extensions run with only the permissions they list.`}
          actions={
            <ButtonGroup>
              <Button icon={<RefreshCw />} busy={refreshing} onClick={() => void refresh()}>
                Check for updates
              </Button>
              {updates.length > 0 && (
                <Button variant="primary" icon={<ArrowUpCircle />} onClick={updateAll}>
                  Update all
                </Button>
              )}
            </ButtonGroup>
          }
        />
      }
    >
      <Stack>
        {shelfError !== null && (
          <Notice tone="critical" title="The catalog could not be read">
            {shelfError}
          </Notice>
        )}
        {shelf?.stale && (
          <Notice tone="warning" title="The catalog is out of date">
            Veronica could not fetch a fresh catalog, so new versions may be missing. Check for
            updates when you are back online.
          </Notice>
        )}
        {shelf?.provenance === "seed" && !shelf.stale && (
          <Notice tone="neutral" title="Showing the catalog bundled with Veronica">
            Check for updates to fetch the latest list.
          </Notice>
        )}
        <Toolbar
          className="library-toolbar"
          leading={
            <SearchField value={query} onChange={setQuery} placeholder="Search extensions" />
          }
          trailing={
            <FilterChips
              label="Category"
              chips={chips}
              selected={category === ALL ? [] : [category]}
              onToggle={(value) => setCategory(value === category ? ALL : value)}
              onClear={() => setCategory(ALL)}
            />
          }
        />
        {body}
        {preferences !== null && (
          <Section title="Automatic updates">
            <FormGroup>
              <FormRow
                label="Update extensions automatically"
                description="Updates wait until the extension has no page open."
                control={
                  <Switch
                    label="Update extensions automatically"
                    checked={preferences.enabled}
                    onChange={(enabled) => savePreferences({ ...preferences, enabled })}
                  />
                }
              />
              <FormRow
                label="Check every"
                control={
                  <Select
                    aria-label="Check every"
                    value={preferences.intervalHours}
                    disabled={!preferences.enabled}
                    onChange={(event) =>
                      savePreferences({
                        ...preferences,
                        intervalHours: Number(event.target.value),
                      })
                    }
                  >
                    {[...new Set([...INTERVALS, preferences.intervalHours])]
                      .sort((a, b) => a - b)
                      .map((hours) => (
                        <option key={hours} value={hours}>
                          {hours === 1 ? "hour" : `${hours} hours`}
                        </option>
                      ))}
                  </Select>
                }
              />
            </FormGroup>
          </Section>
        )}
      </Stack>
      <ConfirmDialog
        open={removal !== null}
        onClose={() => setRemoval(null)}
        onConfirm={() => {
          if (removal !== null) {
            void run("remove", removal.entry, removal.purge);
          }
          setRemoval(null);
        }}
        title={`Remove ${removal?.entry.title ?? ""}?`}
        message="Its pages, cards and commands go away. You can install it again from the Library."
        confirmLabel={removal?.purge ? "Remove and delete data" : "Remove"}
        destructive
      >
        <Checkbox
          checked={removal?.purge ?? false}
          onChange={(purge) => setRemoval((current) => current && { ...current, purge })}
          label="Also delete its data"
          description="Settings, storage and caches kept for this extension are erased."
        />
      </ConfirmDialog>
    </Page>
  );
}

function EntryIcon({ entry }: { entry: LibraryEntry }) {
  const Icon = ICONS[entry.icon ?? ""] ?? Blocks;
  const enabled = entry.feature?.enabled ?? entry.installed?.enabled ?? false;
  return (
    <span className="library-icon" data-enabled={enabled || undefined}>
      <Icon size={16} strokeWidth={1.75} />
    </span>
  );
}

function EntryBadges({ entry, unmet }: { entry: LibraryEntry; unmet: boolean }) {
  const row = entry.installed;
  return (
    <>
      {row?.origin === "sideload" && <Badge tone="accent">Development</Badge>}
      {bundledOffer(entry) !== null && <Badge tone="accent">Bundled</Badge>}
      {row?.origin === "bundled" && <Badge>Bundled</Badge>}
      {(entry.feature !== undefined || row?.origin === "builtin") && <Badge>Built in</Badge>}
      {row !== undefined && row.origin !== "builtin" && <Badge>{row.version}</Badge>}
      {row?.update && <Badge tone="good">Update {row.update}</Badge>}
      {row?.state === "crashLooping" && <Badge tone="critical">Keeps crashing</Badge>}
      {entry.feature?.availability === "unavailable" && <Badge tone="critical">Unavailable</Badge>}
      {entry.feature?.availability === "degraded" && <Badge tone="warning">Partial</Badge>}
      {unmet && <Badge tone="warning">Needs setup</Badge>}
      {entry.labs && <Badge tone="warning">Labs</Badge>}
    </>
  );
}

function LibraryRow({
  entry,
  diagnostics,
  job,
  error,
  switching,
  unmet,
  toolNotices,
  installing,
  onAction,
  onToggleFeature,
  onInstallTool,
}: {
  entry: LibraryEntry;
  diagnostics: Diagnostics;
  job: ExtensionJob | undefined;
  error: string | undefined;
  switching: boolean;
  unmet: ToolReport[];
  toolNotices: { tool: ToolReport; notice: ToolNotice }[];
  installing: string | null;
  onAction: (action: LibraryAction) => void;
  onToggleFeature: () => void;
  onInstallTool: (tool: ToolReport) => void;
}) {
  const actions = availableActions(entry, job);
  const working = isActive(job);
  const permissions = permissionSummary(entry.permissions);
  const tools = entry.requires.tools ?? [];
  const missing = entry.feature?.missing ?? [];
  const builtinRow = entry.installed?.origin === "builtin";

  let trailing: ReactNode;
  if (entry.feature !== undefined) {
    trailing = (
      <Switch
        label={`${entry.feature.enabled ? "Disable" : "Enable"} ${entry.title}`}
        checked={entry.feature.enabled}
        disabled={entry.feature.availability === "unavailable" || switching}
        onChange={onToggleFeature}
      />
    );
  } else if (builtinRow && entry.installed !== undefined) {
    const enabled = entry.installed.enabled;
    trailing = (
      <Switch
        label={`${enabled ? "Disable" : "Enable"} ${entry.title}`}
        checked={enabled}
        disabled={working}
        onChange={() => onAction(enabled ? "disable" : "enable")}
      />
    );
  } else if (actions.length > 0) {
    trailing = (
      <ButtonGroup>
        {actions.map((action) => (
          <Button
            key={action}
            size="small"
            variant={
              action === "install" || action === "update"
                ? "primary"
                : action === "remove"
                  ? "destructive-plain"
                  : "secondary"
            }
            aria-label={`${ACTION_LABELS[action]} ${entry.title}`}
            onClick={() => onAction(action)}
          >
            {ACTION_LABELS[action]}
          </Button>
        ))}
      </ButtonGroup>
    );
  }

  return (
    <ListRow
      className="library-row"
      leading={<EntryIcon entry={entry} />}
      title={
        <span className="library-title">
          <span className="library-name">{entry.title}</span>
          <EntryBadges entry={entry} unmet={unmet.length > 0} />
        </span>
      }
      subtitle={
        <span className="library-detail">
          <span>{entry.summary}</span>
          {entry.installed === undefined && permissions.length > 0 && (
            <span className="library-note">Will be allowed to: {permissions.join(", ")}.</span>
          )}
          {bundledOffer(entry) !== null && (
            <span className="library-note">
              Installs offline from the copy bundled with Veronica.
            </span>
          )}
          {entry.installed === undefined && tools.length > 0 && (
            <span className="library-note">Needs {tools.join(", ")} on this computer.</span>
          )}
          {missing.length > 0 && (
            <span className="library-note">
              Needs {missing.join(", ")}. {reasonFor(diagnostics, missing[0])}
            </span>
          )}
          {unmet.map((tool) => (
            <span className="library-tool" key={tool.id}>
              <span className="library-note">
                {tool.state === "error" ? tool.detail : `${tool.why} ${tool.instruction}`}
              </span>
              <Button
                size="small"
                icon={<Download />}
                busy={installing === tool.id}
                disabled={installing !== null}
                onClick={() => onInstallTool(tool)}
              >
                {installing === tool.id ? "Installing…" : `Install ${tool.displayName}`}
              </Button>
            </span>
          ))}
          {toolNotices.map(({ tool, notice }) => (
            <span
              className={`library-note library-note-${notice.tone}`}
              role="status"
              key={tool.id}
            >
              {notice.text}
            </span>
          ))}
          {working && job !== undefined && (
            <span className="library-job" role="status">
              <span className="library-note">
                {job.message ?? (job.phase === "queued" ? "Waiting for another change" : "Working")}
              </span>
              <ProgressBar value={null} size="thin" label={`${entry.title} progress`} />
            </span>
          )}
          {error !== undefined && !working && (
            <span className="library-note library-note-critical" role="alert">
              {error}
            </span>
          )}
        </span>
      }
      trailing={trailing}
    />
  );
}

function reasonFor(diagnostics: Diagnostics, capability: string): string {
  const state = diagnostics.capabilities.states[capability];
  return state !== undefined && "reason" in state ? state.reason : "";
}
