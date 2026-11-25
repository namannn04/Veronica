import { describe, expect, test } from "bun:test";

import { homeCards, parseSurfaces, type Surface } from "../src/lib/homeCards";

function surface(
  id: string,
  template: Surface["template"],
  state: Record<string, unknown>,
  extra: Partial<Surface> = {},
): Surface {
  return {
    extension: "media",
    id,
    placement: "home",
    template,
    title: id,
    priority: 0,
    actions: [],
    state,
    ...extra,
  };
}

describe("home cards from extension surfaces", () => {
  test("only well-formed surfaces are kept", () => {
    const good = surface("a", "text", { text: "hi" });
    expect(parseSurfaces([good, { id: "x" }, { ...good, template: "chart" }, null])).toEqual([
      good,
    ]);
    expect(parseSurfaces({ surfaces: [] })).toEqual([]);
  });

  test("each template becomes a card body", () => {
    const cards = homeCards([
      surface("ring", "ring", { value: 0.25, label: "Session" }),
      surface("meter", "meter", { value: 3, caption: "full" }),
      surface("stat", "stat", { value: 12, label: "Today", caption: "tokens" }),
      surface("text", "text", { text: "All quiet" }),
      surface("list", "list", { items: [{ title: "box", detail: "up" }, { title: "nas" }] }),
      surface("now", "nowPlaying", { title: "Song", artist: "Band", album: "LP", playing: true }),
      surface("zactions", "actions", {}),
    ]);
    expect(cards.map((card) => card.body)).toEqual([
      {
        kind: "list",
        items: [
          { title: "box", detail: "up" },
          { title: "nas", detail: null },
        ],
      },
      { kind: "gauge", label: "", fraction: 1, caption: "full" },
      { kind: "nowPlaying", title: "Song", detail: "Band · LP", playing: true },
      { kind: "gauge", label: "Session", fraction: 0.25, caption: "25%" },
      { kind: "stat", label: "Today", value: "12", caption: "tokens" },
      { kind: "text", text: "All quiet" },
      { kind: "actions", caption: null },
    ]);
  });

  test("notch activities stay out of Home but lend their privacy category", () => {
    const cards = homeCards([
      surface("now", "nowPlaying", { title: "Song" }, { opens: "main" }),
      surface(
        "now",
        "nowPlaying",
        { title: "Song" },
        { placement: "notch", blurCategory: "music" },
      ),
      surface("spend", "stat", { value: "$4" }, { blurCategory: "money" }),
      surface("plain", "text", { text: "x" }),
    ]);
    expect(cards.map((card) => [card.surface, card.blurClass, card.opens])).toEqual([
      ["now", "blur-music", "main"],
      ["plain", null, null],
      ["spend", "blur-money", null],
    ]);
  });

  test("card actions keep their labels and drop empty commands", () => {
    const [card] = homeCards([
      surface(
        "now",
        "actions",
        {},
        {
          actions: [
            { id: "toggle", label: "Play or pause", run: ["media", "toggle"] },
            { id: "next", run: ["media", "next"] },
            { id: "broken", run: [] },
          ],
        },
      ),
    ]);
    expect(card.actions).toEqual([
      { id: "toggle", label: "Play or pause" },
      { id: "next", label: "next" },
    ]);
  });
});

test("a heatmap card keeps well formed days and drops the rest", () => {
  const [card] = homeCards([
    {
      extension: "usage",
      id: "activity",
      placement: "home",
      template: "heatmap",
      priority: 0,
      actions: [],
      state: {
        cells: [
          { period: "2026-11-23", level: 2, cost: 1.5, tokens: 300 },
          { period: "2026-11-24", level: 4 },
          { level: 1 },
        ],
      },
    },
  ]);
  expect(card.body).toEqual({
    kind: "heatmap",
    cells: [
      { period: "2026-11-23", level: 2, cost: 1.5, tokens: 300 },
      { period: "2026-11-24", level: 4, cost: 0, tokens: 0 },
    ],
    caption: null,
  });
});
