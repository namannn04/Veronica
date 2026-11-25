import { describe, expect, test } from "bun:test";

import {
  ALL,
  availableActions,
  buildEntries,
  bundledOffer,
  groupLibrary,
  LABS,
  type Listen,
  libraryCategories,
  libraryCommands,
  needsConfirmation,
  pendingUpdates,
  permissionSummary,
  watchJobs,
} from "../src/lib/library";
import type {
  CatalogOffer,
  CatalogShelf,
  ExtensionJob,
  ExtensionReport,
  InstalledExtension,
} from "../src/lib/types";

function offer(id: string, category: string, extra: Partial<CatalogOffer> = {}): CatalogOffer {
  return {
    id,
    title: id.charAt(0).toUpperCase() + id.slice(1),
    summary: `${id} summary`,
    category,
    permissions: {},
    requires: {},
    latest: "1.0.0",
    installed: null,
    available: "1.0.0",
    bundled: null,
    ...extra,
  };
}

function row(id: string, extra: Partial<InstalledExtension> = {}): InstalledExtension {
  return {
    id,
    version: "1.0.0",
    origin: "catalog",
    enabled: true,
    installedAt: "2026-11-25T00:00:00Z",
    previous: null,
    state: "stopped",
    update: null,
    manifest: null,
    ...extra,
  };
}

const feature = {
  id: "emoji",
  title: "Emoji",
  subtitle: "Search and insert emoji",
  icon: "smile",
  defaultsKey: "extEmoji",
  group: "utilities",
  featured: false,
  enabled: true,
  availability: "available",
} as ExtensionReport;

const shelf: CatalogShelf = {
  revision: 1,
  generatedAt: "2026-11-24T12:00:00Z",
  stale: false,
  provenance: "network",
  categories: [
    { id: "media", title: "Media" },
    { id: "agents", title: "Agents" },
    { id: "labs", title: "Labs", hidden: true },
  ],
  extensions: [
    offer("music", "media", { installed: "1.0.0" }),
    offer("radio", "media"),
    offer("usage", "agents"),
    offer("probe", "labs"),
    offer("sketch", "media", { labs: true }),
  ],
};

const installed = [
  row("music", { update: "1.1.0", previous: "0.9.0" }),
  row("local", {
    origin: "sideload",
    manifest: {
      id: "local",
      title: "Local tool",
      summary: "Made here",
      category: "utilities",
      permissions: { host: [], filesystem: [], network: [] },
    },
  }),
];

const entries = buildEntries([feature], installed, shelf);
const categories = libraryCategories(shelf, entries);

function ids(query: string, category = ALL) {
  return groupLibrary(entries, categories, query, category).map((group) => [
    group.id,
    group.entries.map((entry) => entry.id),
  ]);
}

describe("library grouping", () => {
  test("catalog, installed and built-in entries are grouped by category with Labs hidden", () => {
    expect(ids("")).toEqual([
      ["media", ["music", "radio"]],
      ["agents", ["usage"]],
      ["utilities", ["emoji", "local"]],
    ]);
    expect(categories.at(-1)).toEqual({ id: LABS, title: "Labs", hidden: true });
  });

  test("search covers titles, summaries and category names and reveals Labs", () => {
    expect(ids("radio")).toEqual([["media", ["radio"]]]);
    expect(ids("agents")).toEqual([["agents", ["usage"]]]);
    expect(ids("summary")).toEqual([
      ["media", ["music", "radio"]],
      ["agents", ["usage"]],
      ["labs", ["probe", "sketch"]],
    ]);
    expect(ids("nothing at all")).toEqual([]);
  });

  test("selecting Labs shows only Labs, and another category hides it", () => {
    expect(ids("", LABS)).toEqual([["labs", ["probe", "sketch"]]]);
    expect(ids("", "media")).toEqual([["media", ["music", "radio"]]]);
    expect(ids("probe", "media")).toEqual([]);
  });

  test("an installed catalog entry keeps its row and built-in features keep their switch", () => {
    const music = entries.find((entry) => entry.id === "music");
    expect(music?.installed?.update).toBe("1.1.0");
    expect(entries.find((entry) => entry.id === "emoji")?.feature?.defaultsKey).toBe("extEmoji");
    expect(entries.find((entry) => entry.id === "local")?.installed?.origin).toBe("sideload");
    expect(pendingUpdates(entries)).toEqual(["music"]);
  });
});

describe("library actions", () => {
  const find = (id: string) => {
    const entry = entries.find((item) => item.id === id);
    if (entry === undefined) {
      throw new Error(id);
    }
    return entry;
  };
  const job = (phase: ExtensionJob["phase"]): ExtensionJob => ({
    id: 1,
    kind: "update",
    extension: "music",
    phase,
    message: "Downloading the update",
    error: null,
    startedAt: "",
    finishedAt: null,
  });

  test("each entry offers only the actions that apply", () => {
    expect(availableActions(find("radio"), undefined)).toEqual(["install"]);
    expect(availableActions(find("music"), undefined)).toEqual([
      "update",
      "disable",
      "rollback",
      "remove",
    ]);
    expect(availableActions(find("local"), undefined)).toEqual(["disable", "remove"]);
    expect(availableActions(find("emoji"), undefined)).toEqual([]);
    expect(availableActions(find("music"), job("running"))).toEqual([]);
    expect(availableActions(find("music"), job("failed"))).toContain("update");
    const builtin = {
      ...find("music"),
      installed: row("system", { origin: "builtin", enabled: false }),
    };
    expect(availableActions(builtin, undefined)).toEqual(["enable"]);
    const running = {
      ...find("local"),
      installed: row("local", { origin: "sideload", state: "running" }),
    };
    expect(availableActions(running, undefined)).toContain("restart");
  });

  test("removal asks first and purge is passed through only after confirming", async () => {
    const calls: [string, unknown][] = [];
    const commands = libraryCommands(async <T>(command: string, args?: Record<string, unknown>) => {
      calls.push([command, args]);
      return null as T;
    });
    expect(needsConfirmation("remove")).toBe(true);
    for (const action of [
      "install",
      "update",
      "enable",
      "disable",
      "rollback",
      "restart",
    ] as const) {
      expect(needsConfirmation(action)).toBe(false);
    }
    await commands.run("remove", "music", true);
    await commands.run("remove", "music");
    await commands.run("install", "radio");
    await commands.run("restart", "local");
    await commands.refresh();
    expect(calls).toEqual([
      ["ext_remove", { id: "music", purge: true }],
      ["ext_remove", { id: "music", purge: false }],
      ["ext_install", { id: "radio" }],
      ["ext_restart", { id: "local" }],
      ["ext_catalog_refresh", undefined],
    ]);
  });

  test("permissions and needs are described before installing", () => {
    expect(
      permissionSummary({
        network: ["api.example.org"],
        filesystem: [{ path: "~/Music", access: "read" }],
        host: ["openExternal", "notifications"],
      }),
    ).toEqual([
      "Connects to api.example.org",
      "Reads ~/Music",
      "Opens web links",
      "Shows notifications",
    ]);
  });
});

describe("job progress", () => {
  test("active jobs are loaded, older events are ignored and stop detaches", async () => {
    let handler: ((event: { payload: ExtensionJob }) => void) | undefined;
    let unlistened = false;
    const listen = (async (event: string, callback: (event: { payload: ExtensionJob }) => void) => {
      expect(event).toBe("extension-job");
      handler = callback;
      return () => {
        unlistened = true;
      };
    }) as Listen;
    const base: ExtensionJob = {
      id: 3,
      kind: "install",
      extension: "radio",
      phase: "running",
      message: "Downloading and verifying",
      error: null,
      startedAt: "",
      finishedAt: null,
    };
    const finished = { ...base, id: 2, extension: "music", phase: "succeeded" as const };
    const invoke = async <T>(command: string) => {
      expect(command).toBe("ext_jobs");
      return [base, finished] as T;
    };
    const seen: [string, string | undefined][] = [];
    const stop = watchJobs(invoke, listen, (jobs, job) => {
      seen.push([job.phase, jobs.get("radio")?.phase]);
    });
    await new Promise((resolve) => setTimeout(resolve, 0));
    handler?.({ payload: { ...base, phase: "failed", error: "checksum mismatch" } });
    handler?.({ payload: { ...base, id: 1, phase: "succeeded" } });
    expect(seen).toEqual([
      ["running", "running"],
      ["failed", "failed"],
    ]);
    stop();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(unlistened).toBe(true);
  });
});

describe("bundled extensions", () => {
  const offline: CatalogShelf = {
    ...shelf,
    extensions: [
      offer("music", "media", { installed: "1.0.0", bundled: "1.0.0" }),
      offer("lamp", "desk", { bundled: "0.3.0", available: "0.3.0", latest: "0.3.0" }),
    ],
  };
  const listed = buildEntries([], [row("music")], offline);
  const lamp = listed.find((entry) => entry.id === "lamp");

  test("a bundled-only extension is listed, grouped and installable", () => {
    const groups = groupLibrary(listed, libraryCategories(offline, listed), "", ALL);
    expect(groups.map((group) => [group.id, group.entries.map((entry) => entry.id)])).toEqual([
      ["media", ["music"]],
      ["desk", ["lamp"]],
    ]);
    expect(groups[1].title).toBe("Desk");
    expect(lamp && availableActions(lamp, undefined)).toEqual(["install"]);
  });

  test("the Bundled badge shows only until the extension is installed", () => {
    expect(lamp && bundledOffer(lamp)).toBe("0.3.0");
    expect(bundledOffer(listed.find((entry) => entry.id === "music") ?? listed[0])).toBeNull();
    expect(bundledOffer(entries.find((entry) => entry.id === "radio") ?? entries[0])).toBeNull();
  });
});
