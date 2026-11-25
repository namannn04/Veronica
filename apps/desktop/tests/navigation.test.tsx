import { describe, expect, test } from "bun:test";

import {
  extensionNavItems,
  mergeNavGroups,
  NAV_GROUPS,
  NAV_ITEMS,
  paletteItems,
  parseExtensionRoute,
  routeFromSearch,
  routeIsVisible,
  searchNav,
} from "../src/lib/navigation";
import { isEmojiPickerSurface } from "../src/lib/surfaces";
import type { ExtensionUiPage, InstalledExtension } from "../src/lib/types";

describe("extension-aware navigation", () => {
  test("the shortcut surface is distinct from the full application", () => {
    expect(isEmojiPickerSurface("?surface=emoji-picker")).toBe(true);
    expect(isEmojiPickerSurface("?route=emoji")).toBe(false);
    expect(isEmojiPickerSurface("")).toBe(false);
  });

  test("an on-demand window opens its requested route safely", () => {
    expect(routeFromSearch("?route=settings")).toBe("settings");
    expect(routeFromSearch("?route=ext:usage/main")).toBe("ext:usage/main");
    expect(routeFromSearch("?route=not-a-page")).toBe("home");
    expect(routeFromSearch("")).toBe("home");
  });

  test("Veronica's own pages are always visible", () => {
    for (const item of NAV_ITEMS) {
      expect(routeIsVisible(item.id, [])).toBe(true);
    }
  });
});

function installed(id: string, pages: ExtensionUiPage[], enabled = true): InstalledExtension {
  return {
    id,
    version: "1.0.0",
    origin: "catalog",
    enabled,
    installedAt: "2026-11-25T00:00:00Z",
    previous: null,
    state: "stopped",
    update: null,
    manifest: {
      id,
      category: "utilities",
      title: id.charAt(0).toUpperCase() + id.slice(1),
      summary: `${id} summary`,
      permissions: { host: [], filesystem: [], network: [] },
      ui: { root: "ui", pages },
    },
  };
}

describe("extension pages in navigation", () => {
  const rows = [
    installed("notes", [
      { id: "main", title: "Notes", entry: "index.html", nav: { group: "Tools", order: 2 } },
      { id: "detail", title: "Note", entry: "detail.html" },
    ]),
    installed("radio", [
      { id: "main", title: "Radio", entry: "index.html", nav: { group: "Listening", order: 0 } },
      { id: "bad", title: "Escape", entry: "../index.html", nav: { group: "Tools", order: 0 } },
    ]),
    installed("weather", [
      { id: "main", title: "Weather", entry: "index.html", nav: { group: "tools", order: 1 } },
    ]),
    installed(
      "off",
      [{ id: "main", title: "Off", entry: "index.html", nav: { group: "Tools", order: 0 } }],
      false,
    ),
    { ...installed("broken", []), manifest: null },
  ];

  test("only enabled extensions with a safe entry contribute pages", () => {
    const pages = extensionNavItems(rows);
    expect(pages.map((page) => page.id)).toEqual([
      "ext:notes/main",
      "ext:notes/detail",
      "ext:radio/main",
      "ext:weather/main",
    ]);
    expect(pages[0].frame).toEqual({ extension: "notes", page: "main", entry: "index.html" });
    expect(pages[1].hint).toBe("notes summary");
  });

  test("pages join their named group in order and new groups sit before Veronica", () => {
    const groups = mergeNavGroups(NAV_GROUPS, extensionNavItems(rows));
    const labels = groups.map((group) => group.label);
    expect(labels).toEqual(["Agents", "This computer", "Tools", "Listening", "Veronica"]);
    const tools = groups.find((group) => group.label === "Tools")?.items ?? [];
    expect(tools.slice(-2).map((item) => item.id)).toEqual(["ext:weather/main", "ext:notes/main"]);
    expect(groups.at(-1)?.items.map((item) => item.id)).toEqual(["library", "settings", "about"]);
    expect(NAV_ITEMS.some((item) => item.id.startsWith("ext:"))).toBe(false);
  });

  test("the palette finds every extension page, including ones without a rail entry", () => {
    const pages = extensionNavItems(rows);
    const items = paletteItems(mergeNavGroups(NAV_GROUPS, pages), pages);
    expect(searchNav("note", items).map((item) => item.id)).toEqual([
      "ext:notes/main",
      "ext:notes/detail",
    ]);
    expect(searchNav("radio", items).map((item) => item.id)).toEqual(["ext:radio/main"]);
    expect(new Set(items.map((item) => item.id)).size).toBe(items.length);
  });

  test("extension routes are recognised and hidden once their page goes away", () => {
    expect(routeFromSearch("?route=ext:notes/main")).toBe("ext:notes/main");
    expect(routeFromSearch("?route=ext:Notes/main")).toBe("home");
    expect(parseExtensionRoute("ext:notes/detail")).toEqual({ extension: "notes", page: "detail" });
    expect(parseExtensionRoute("ext:notes/../x")).toBeNull();
    const pages = extensionNavItems(rows);
    expect(routeIsVisible("ext:notes/main", pages)).toBe(true);
    expect(routeIsVisible("ext:off/main", pages)).toBe(false);
    expect(routeIsVisible("ext:off/main")).toBe(true);
  });
});
