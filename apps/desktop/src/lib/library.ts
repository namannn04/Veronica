import type {
  AutoUpdatePreferences,
  CatalogCategory,
  CatalogOffer,
  CatalogShelf,
  ExtensionGroup,
  ExtensionJob,
  ExtensionPermissions,
  ExtensionReport,
  ExtensionRequirements,
  InstalledExtension,
} from "./types";

export const LABS = "labs";
export const ALL = "all";

export type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
export type Listen = <T>(
  event: string,
  handler: (event: { payload: T }) => void,
) => Promise<() => void>;

export interface LibraryEntry {
  id: string;
  title: string;
  summary: string;
  icon: string | undefined;
  category: string;
  labs: boolean;
  feature?: ExtensionReport;
  installed?: InstalledExtension;
  offer?: CatalogOffer;
  permissions: ExtensionPermissions;
  requires: ExtensionRequirements;
}

export interface LibraryGroup {
  id: string;
  title: string;
  entries: LibraryEntry[];
}

const FEATURE_CATEGORIES: Record<ExtensionGroup, CatalogCategory> = {
  agent: { id: "agents", title: "Agents" },
  system: { id: "system", title: "System" },
  media: { id: "media", title: "Media" },
  utilities: { id: "utilities", title: "Utilities" },
};

function titleCase(id: string): string {
  return id.charAt(0).toUpperCase() + id.slice(1);
}

export function buildEntries(
  features: readonly ExtensionReport[],
  installed: readonly InstalledExtension[],
  shelf: CatalogShelf | null,
): LibraryEntry[] {
  const rows = new Map(installed.map((row) => [row.id, row]));
  const entries: LibraryEntry[] = [];
  const seen = new Set<string>();
  for (const offer of shelf?.extensions ?? []) {
    seen.add(offer.id);
    entries.push({
      id: offer.id,
      title: offer.title,
      summary: offer.summary,
      icon: offer.icon,
      category: offer.category,
      labs: offer.labs === true,
      installed: rows.get(offer.id),
      offer,
      permissions: offer.permissions,
      requires: offer.requires,
    });
  }
  for (const row of installed) {
    if (seen.has(row.id)) {
      continue;
    }
    seen.add(row.id);
    entries.push({
      id: row.id,
      title: row.manifest?.title ?? row.id,
      summary: row.manifest?.summary ?? "",
      icon: row.manifest?.icon,
      category: row.manifest?.category ?? "other",
      labs: row.manifest?.labs === true,
      installed: row,
      permissions: row.manifest?.permissions ?? {},
      requires: row.manifest?.requires ?? {},
    });
  }
  for (const feature of features) {
    if (seen.has(feature.id)) {
      continue;
    }
    entries.push({
      id: feature.id,
      title: feature.title,
      summary: feature.subtitle,
      icon: feature.icon,
      category: FEATURE_CATEGORIES[feature.group].id,
      labs: false,
      feature,
      permissions: {},
      requires: {},
    });
  }
  return entries;
}

export function libraryCategories(
  shelf: CatalogShelf | null,
  entries: readonly LibraryEntry[],
): CatalogCategory[] {
  const categories = new Map<string, CatalogCategory>();
  for (const category of shelf?.categories ?? []) {
    categories.set(category.id, category);
  }
  for (const category of Object.values(FEATURE_CATEGORIES)) {
    if (!categories.has(category.id)) {
      categories.set(category.id, category);
    }
  }
  for (const entry of entries) {
    if (!categories.has(entry.category)) {
      categories.set(entry.category, { id: entry.category, title: titleCase(entry.category) });
    }
  }
  const labs = categories.get(LABS) ?? { id: LABS, title: "Labs", hidden: true };
  categories.delete(LABS);
  return [...categories.values(), { ...labs, hidden: true }];
}

function inLabs(entry: LibraryEntry, hidden: ReadonlySet<string>): boolean {
  return entry.labs || hidden.has(entry.category);
}

export function groupLibrary(
  entries: readonly LibraryEntry[],
  categories: readonly CatalogCategory[],
  query: string,
  category: string,
): LibraryGroup[] {
  const needle = query.trim().toLowerCase();
  const hidden = new Set(categories.filter((item) => item.hidden).map((item) => item.id));
  const titles = new Map(categories.map((item) => [item.id, item.title]));
  const showLabs = needle !== "" || category === LABS;
  const groups = new Map<string, LibraryEntry[]>();
  for (const entry of entries) {
    const group = inLabs(entry, hidden) ? LABS : entry.category;
    if (group === LABS && !showLabs) {
      continue;
    }
    if (category !== ALL && group !== category) {
      continue;
    }
    const haystack = [entry.id, entry.title, entry.summary, titles.get(entry.category) ?? ""]
      .join(" ")
      .toLowerCase();
    if (needle !== "" && !haystack.includes(needle)) {
      continue;
    }
    groups.set(group, [...(groups.get(group) ?? []), entry]);
  }
  return categories
    .filter((item) => groups.has(item.id))
    .map((item) => ({
      id: item.id,
      title: item.title,
      entries: (groups.get(item.id) ?? []).sort((a, b) => a.title.localeCompare(b.title)),
    }));
}

export function isActive(job: ExtensionJob | undefined): boolean {
  return job !== undefined && (job.phase === "queued" || job.phase === "running");
}

export function applyJob(
  jobs: ReadonlyMap<string, ExtensionJob>,
  job: ExtensionJob,
): Map<string, ExtensionJob> {
  const next = new Map(jobs);
  const current = next.get(job.extension);
  if (current === undefined || current.id <= job.id) {
    next.set(job.extension, job);
  }
  return next;
}

export function watchJobs(
  invoke: Invoke,
  listen: Listen,
  onChange: (jobs: Map<string, ExtensionJob>, job: ExtensionJob) => void,
): () => void {
  let jobs = new Map<string, ExtensionJob>();
  let live = true;
  const take = (job: ExtensionJob) => {
    jobs = applyJob(jobs, job);
    if (live && jobs.get(job.extension) === job) {
      onChange(jobs, job);
    }
  };
  const stop = listen<ExtensionJob>("extension-job", (event) => take(event.payload));
  void invoke<ExtensionJob[]>("ext_jobs").then(
    (list) => {
      for (const job of list.filter((item) => isActive(item))) {
        take(job);
      }
    },
    () => undefined,
  );
  return () => {
    live = false;
    void stop.then((unlisten) => unlisten());
  };
}

export type LibraryAction =
  | "install"
  | "update"
  | "enable"
  | "disable"
  | "remove"
  | "rollback"
  | "restart";

export function availableActions(
  entry: LibraryEntry,
  job: ExtensionJob | undefined,
): LibraryAction[] {
  if (entry.feature !== undefined || isActive(job)) {
    return [];
  }
  const row = entry.installed;
  if (row === undefined) {
    return entry.offer?.available ? ["install"] : [];
  }
  const actions: LibraryAction[] = [];
  if (row.update !== null && row.origin === "catalog") {
    actions.push("update");
  }
  actions.push(row.enabled ? "disable" : "enable");
  if (row.enabled && row.state !== "stopped") {
    actions.push("restart");
  }
  if (row.previous !== null && row.origin === "catalog") {
    actions.push("rollback");
  }
  if (row.origin !== "builtin") {
    actions.push("remove");
  }
  return actions;
}

export function needsConfirmation(action: LibraryAction): boolean {
  return action === "remove";
}

export function pendingUpdates(entries: readonly LibraryEntry[]): string[] {
  return entries
    .filter((entry) => entry.installed?.origin === "catalog" && entry.installed.update !== null)
    .map((entry) => entry.id);
}

export function libraryCommands(invoke: Invoke) {
  return {
    installed: () => invoke<InstalledExtension[]>("ext_list"),
    shelf: () => invoke<CatalogShelf>("ext_catalog_search", { query: "", category: null }),
    refresh: () => invoke<CatalogShelf>("ext_catalog_refresh"),
    autoUpdate: () => invoke<AutoUpdatePreferences>("ext_auto_update"),
    setAutoUpdate: (preferences: AutoUpdatePreferences) =>
      invoke<AutoUpdatePreferences>("ext_auto_update_set", { preferences }),
    run: (action: LibraryAction, id: string, purge = false): Promise<unknown> => {
      switch (action) {
        case "remove":
          return invoke("ext_remove", { id, purge });
        case "restart":
          return invoke("ext_restart", { id });
        default:
          return invoke(`ext_${action}`, { id });
      }
    },
  };
}

export function permissionSummary(permissions: ExtensionPermissions): string[] {
  const lines: string[] = [];
  for (const host of permissions.network ?? []) {
    lines.push(`Connects to ${host}`);
  }
  for (const grant of permissions.filesystem ?? []) {
    lines.push(`${grant.access === "readWrite" ? "Reads and writes" : "Reads"} ${grant.path}`);
  }
  for (const name of [...(permissions.dbus?.session ?? []), ...(permissions.dbus?.system ?? [])]) {
    lines.push(`Talks to ${name}`);
  }
  for (const program of permissions.spawn ?? []) {
    lines.push(`Runs ${program}`);
  }
  for (const service of permissions.shell ?? []) {
    lines.push(`Uses the desktop's ${service}`);
  }
  const host: Record<string, string> = {
    notifications: "Shows notifications",
    surfaces: "Adds cards to Home and the notch",
    contracts: "Shares data with other extensions",
    "settings:read": "Reads Veronica settings",
    secrets: "Stores secrets in the keyring",
    openExternal: "Opens web links",
    "clipboard:write": "Writes to the clipboard",
    shortcuts: "Registers keyboard shortcuts",
  };
  for (const permission of permissions.host ?? []) {
    lines.push(host[permission] ?? permission);
  }
  return lines;
}
