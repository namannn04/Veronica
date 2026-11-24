import { describe, expect, test } from "bun:test";

import {
  chooseProfile,
  chosenProfile,
  missingStarters,
  needsOnboarding,
  starterStatus,
  storedStarters,
} from "../src/lib/onboarding";
import type {
  CatalogOffer,
  CatalogShelf,
  ExtensionJob,
  ExtensionReport,
  InstalledExtension,
} from "../src/lib/types";

const features = [{ id: "usage", defaultsKey: "tabUsageEnabled" }] as ExtensionReport[];

function offer(id: string, available: string | null = "1.0.0"): CatalogOffer {
  return {
    id,
    title: id,
    summary: "",
    category: "media",
    permissions: {},
    requires: {},
    latest: "1.0.0",
    installed: null,
    available,
  };
}

const shelf: CatalogShelf = {
  revision: 1,
  generatedAt: "",
  stale: false,
  provenance: "seed",
  categories: [],
  profiles: [
    { id: "simple", title: "Simple", description: "", extensions: ["media", "clipboard"] },
    {
      id: "developer",
      title: "Developer",
      description: "",
      extensions: ["usage", "machines", "clipboard", "ghost", "broken"],
    },
  ],
  extensions: [
    offer("media"),
    offer("clipboard"),
    offer("usage"),
    offer("machines"),
    offer("broken", null),
  ],
};

function row(id: string): InstalledExtension {
  return {
    id,
    version: "1.0.0",
    origin: "catalog",
    enabled: true,
    installedAt: "",
    previous: null,
    state: "stopped",
    update: null,
    manifest: null,
  };
}

function job(phase: ExtensionJob["phase"], error: string | null = null): ExtensionJob {
  return {
    id: 1,
    kind: "install",
    extension: "media",
    phase,
    message: "Downloading and verifying",
    error,
    startedAt: "",
    finishedAt: null,
  };
}

describe("first run", () => {
  test("a fresh install onboards, a chosen profile or legacy toggles skip it", () => {
    expect(needsOnboarding({ appearance: "dark" }, features)).toBe(true);
    expect(needsOnboarding({ profile: "developer" }, features)).toBe(false);
    expect(needsOnboarding({ tabUsageEnabled: false }, features)).toBe(false);
    expect(needsOnboarding({ profile: "expert" }, features)).toBe(true);
    expect(chosenProfile({ profile: "simple" })).toBe("simple");
    expect(chosenProfile({})).toBeNull();
  });

  test("the starter pack is the profile's installable members not yet installed", () => {
    expect(missingStarters(shelf, "simple", [])).toEqual(["media", "clipboard"]);
    expect(missingStarters(shelf, "developer", [row("clipboard")])).toEqual(["usage", "machines"]);
    expect(storedStarters({ starterPack: ["media", 3, "clipboard"] })).toEqual([
      "media",
      "clipboard",
    ]);
    expect(storedStarters({ starterPack: null })).toEqual([]);
  });

  test("each starter reports progress, success, failure or nothing yet", () => {
    expect(starterStatus("media", [], job("running")).state).toBe("installing");
    expect(starterStatus("media", [], job("queued")).message).toBe("Downloading and verifying");
    expect(starterStatus("media", [row("media")], job("succeeded")).state).toBe("installed");
    expect(starterStatus("media", [], job("failed", "checksum mismatch"))).toEqual({
      state: "failed",
      message: null,
      error: "checksum mismatch",
    });
    expect(starterStatus("media", [], undefined).state).toBe("pending");
  });

  test("choosing a profile saves it, records the starters and installs only what is missing", async () => {
    const calls: [string, unknown][] = [];
    const invoke = async <T>(command: string, args?: Record<string, unknown>) => {
      calls.push([command, args]);
      if (command === "ext_catalog_search") {
        return shelf as T;
      }
      if (command === "ext_list") {
        return [row("clipboard")] as T;
      }
      if (command === "ext_install" && args?.id === "usage") {
        throw "offline";
      }
      return null as T;
    };
    const added = await chooseProfile(invoke, "developer", ["media"]);
    expect(added).toEqual(["usage", "machines"]);
    expect(calls).toEqual([
      ["settings_set", { key: "profile", value: "developer" }],
      ["ext_catalog_search", { query: "", category: null }],
      ["ext_list", undefined],
      ["settings_set", { key: "starterPack", value: ["media", "usage", "machines"] }],
      ["ext_install", { id: "usage" }],
      ["ext_install", { id: "machines" }],
    ]);
  });

  test("switching to a profile whose starters are installed removes and installs nothing", async () => {
    const commands: string[] = [];
    const invoke = async <T>(command: string) => {
      commands.push(command);
      if (command === "ext_catalog_search") {
        return shelf as T;
      }
      if (command === "ext_list") {
        return [row("media"), row("clipboard")] as T;
      }
      return null as T;
    };
    expect(await chooseProfile(invoke, "simple", [])).toEqual([]);
    expect(commands.some((command) => command.startsWith("ext_remove"))).toBe(false);
    expect(commands.filter((command) => command === "ext_install")).toEqual([]);
  });
});
