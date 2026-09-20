import { describe, expect, test } from "bun:test";

import {
  NAV_ITEMS,
  routeFromSearch,
  routeIsVisible,
  searchNav,
  visibleNavGroups,
  type Route,
} from "../src/lib/navigation";

describe("extension-aware navigation", () => {
  test("an on-demand window opens its requested route safely", () => {
    expect(routeFromSearch("?route=emoji")).toBe("emoji");
    expect(routeFromSearch("?route=not-a-page")).toBe("home");
    expect(routeFromSearch("")).toBe("home");
  });

  test("every feature page points at its catalogue extension", () => {
    const expected: Record<Exclude<Route, "home" | "extensions" | "settings" | "about">, string> = {
      usage: "usage",
      herdr: "herdr",
      quinjet: "quinjet",
      system: "system",
      machines: "machines",
      maintenance: "appMaintenance",
      database: "database",
      media: "music",
      calendar: "calendar",
      attention: "attention",
      clipboard: "clipboard",
      color: "colorPicker",
      emoji: "emoji",
      audit: "seoAudit",
      companion: "companion",
    };
    for (const [route, extensionId] of Object.entries(expected)) {
      expect(NAV_ITEMS.find((item) => item.id === route)?.extensionId).toBe(extensionId);
    }
  });

  test("Veronica's own pages can never be hidden", () => {
    const none = new Set<string>();
    for (const route of ["home", "extensions", "settings", "about"] as const) {
      expect(routeIsVisible(route, none)).toBe(true);
    }
  });

  test("one filtered tree feeds both the rail and palette", () => {
    const enabled = new Set(["usage", "machines"]);
    const items = visibleNavGroups(enabled).flatMap((group) => group.items);

    expect(items.some((item) => item.id === "usage")).toBe(true);
    expect(items.some((item) => item.id === "machines")).toBe(true);
    expect(items.some((item) => item.id === "herdr")).toBe(false);
    expect(searchNav("ssh", items).map((item) => item.id)).toEqual(["machines"]);
    expect(searchNav("herdr", items)).toEqual([]);
  });

  test("a disabled current route is no longer visible", () => {
    expect(routeIsVisible("herdr", new Set(["usage"]))).toBe(false);
    expect(routeIsVisible("herdr", new Set(["usage", "herdr"]))).toBe(true);
  });
});
