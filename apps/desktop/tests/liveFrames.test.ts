import { describe, expect, test } from "bun:test";

import { frameKey, MAX_LIVE_FRAMES, visitFrame } from "../src/lib/liveFrames";
import type { ExtensionRoute } from "../src/lib/navigation";

const route = (id: string): ExtensionRoute => `ext:${id}/main`;

describe("live extension frames", () => {
  test("a revisited frame keeps its launch and moves to the front", () => {
    let frames = visitFrame([], route("media"), { serial: 3, params: { tab: "library" } });
    frames = visitFrame(frames, route("audit"), null);
    frames = visitFrame(frames, route("media"), null);
    expect(frames.map((frame) => frame.route)).toEqual([route("media"), route("audit")]);
    expect(frames[0]).toEqual({ route: route("media"), serial: 3, params: { tab: "library" } });
  });

  test("opening a page again with new parameters gives it a new key", () => {
    const first = visitFrame([], route("media"), null);
    const second = visitFrame(first, route("media"), { serial: 7, params: "again" });
    expect(frameKey(second[0])).not.toBe(frameKey(first[0]));
    expect(second).toHaveLength(1);
  });

  test("only the most recently used frames stay mounted", () => {
    let frames = visitFrame([], route("a1"), null);
    for (const id of ["b1", "c1", "d1", "e1"]) {
      frames = visitFrame(frames, route(id), null);
    }
    expect(frames).toHaveLength(MAX_LIVE_FRAMES);
    expect(frames.map((frame) => frame.route)).not.toContain(route("a1"));
  });
});
