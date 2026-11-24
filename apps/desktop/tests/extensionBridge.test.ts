import { describe, expect, test } from "bun:test";

import {
  acceptsHello,
  answer,
  appearanceOf,
  ECHO_METHODS,
  forwardTo,
  frameSource,
  type Invoke,
  interfaceRequest,
  isHello,
  localHandler,
  progressMessage,
  readyMessage,
  serve,
  topicMessage,
} from "../src/extensions/bridge";
import { isFrameProbeSurface } from "../src/lib/surfaces";

const hello = { v: 1, type: "hello", apiVersion: 1 };
const echo = localHandler(ECHO_METHODS);

describe("extension frame bridge", () => {
  test("a frame address is built only from a valid id and a plain entry", () => {
    expect(frameSource("media", "index.html")).toBe("veronica-ext://media/index.html");
    expect(frameSource("seo-audit", "pages/main.html")).toBe(
      "veronica-ext://seo-audit/pages/main.html",
    );
    for (const [extension, entry] of [
      ["Media", "index.html"],
      ["m", "index.html"],
      ["media/../other", "index.html"],
      ["media", "../index.html"],
      ["media", "/index.html"],
      ["media", "pages//index.html"],
      ["media", ".hidden"],
      ["media", "index.html?next=1"],
      ["media", "index.html#top"],
      ["media", ""],
      ["media@host", "index.html"],
    ]) {
      expect(frameSource(extension, entry)).toBeNull();
    }
  });

  test("only the documented greeting counts as a hello", () => {
    expect(isHello(hello)).toBe(true);
    expect(isHello({ ...hello, apiVersion: 2 })).toBe(false);
    expect(isHello({ ...hello, type: "ready" })).toBe(false);
    expect(isHello(JSON.stringify(hello))).toBe(false);
    expect(isHello(null)).toBe(false);
  });

  test("a hello is accepted only from the frame's own window and an opaque origin", () => {
    const frame = {};
    expect(acceptsHello({ source: frame, origin: "null", data: hello }, frame)).toBe(true);
    expect(acceptsHello({ source: {}, origin: "null", data: hello }, frame)).toBe(false);
    expect(
      acceptsHello({ source: frame, origin: "veronica-ext://media", data: hello }, frame),
    ).toBe(false);
    expect(acceptsHello({ source: frame, origin: "tauri://localhost", data: hello }, frame)).toBe(
      false,
    );
    expect(acceptsHello({ source: null, origin: "null", data: hello }, null)).toBe(false);
    expect(acceptsHello({ source: frame, origin: "null", data: { type: "hello" } }, frame)).toBe(
      false,
    );
  });

  test("echo returns its parameters and nothing else is callable", async () => {
    expect(await answer(echo, { id: 1, method: "echo", params: { a: [1, 2] } })).toEqual({
      id: 1,
      result: { a: [1, 2] },
    });
    for (const method of ["constructor", "toString", "__proto__", "hasOwnProperty", "invoke"]) {
      expect(await answer(echo, { id: "x", method })).toEqual({
        id: "x",
        error: { code: "E_INVALID", message: `unknown method ${method}` },
      });
    }
  });

  test("malformed requests are dropped and failures are reported", async () => {
    for (const message of [
      null,
      "echo",
      { method: "echo" },
      { id: {}, method: "echo" },
      { id: 1 },
    ]) {
      expect(await answer(echo, message)).toBeNull();
    }
    const failing = {
      fail: () => {
        throw new Error("offline");
      },
    };
    expect(await answer(localHandler(failing), { id: 7, method: "fail" })).toEqual({
      id: 7,
      error: { code: "E_UNAVAILABLE", message: "offline" },
    });
  });

  test("a served port answers over the channel", async () => {
    const channel = new MessageChannel();
    serve(channel.port1, echo);
    const reply = await new Promise((resolve) => {
      channel.port2.onmessage = (event) => resolve(event.data);
      channel.port2.postMessage({ id: 3, method: "echo", params: "ping" });
    });
    expect(reply).toEqual({ id: 3, result: "ping" });
    channel.port1.close();
    channel.port2.close();
  });

  test("the probe surface is opt-in", () => {
    expect(isFrameProbeSurface("?surface=frame-probe&extension=hostile")).toBe(true);
    expect(isFrameProbeSurface("?surface=emoji-picker")).toBe(false);
    expect(isFrameProbeSurface("")).toBe(false);
  });

  test("port requests are forwarded to the host with the frame's own identity", async () => {
    const calls: [string, Record<string, unknown> | undefined][] = [];
    const invoke: Invoke = async <T>(command: string, args?: Record<string, unknown>) => {
      calls.push([command, args]);
      if (args?.method === "ui.openExternal") {
        throw {
          code: "E_PERMISSION",
          message: "media was not granted openExternal",
          data: { permission: "openExternal" },
        };
      }
      if (args?.method === "storage.set") {
        return null as T;
      }
      return { ok: args?.params } as T;
    };
    const channel = new MessageChannel();
    serve(channel.port1, forwardTo(invoke, "media", "main"));
    const replies: unknown[] = [];
    const done = new Promise<void>((resolve) => {
      channel.port2.onmessage = (event) => {
        replies.push(event.data);
        if (replies.length === 3) {
          resolve();
        }
      };
    });
    channel.port2.postMessage({ id: 1, method: "backend.call", params: { method: "tracks" } });
    channel.port2.postMessage({ id: 2, method: "ui.openExternal", params: { url: "https://x" } });
    channel.port2.postMessage({ id: 3, method: "storage.set" });
    await done;
    channel.port1.close();
    channel.port2.close();

    expect(calls).toEqual([
      [
        "ext_bridge",
        { extension: "media", page: "main", method: "backend.call", params: { method: "tracks" } },
      ],
      [
        "ext_bridge",
        {
          extension: "media",
          page: "main",
          method: "ui.openExternal",
          params: { url: "https://x" },
        },
      ],
      ["ext_bridge", { extension: "media", page: "main", method: "storage.set", params: null }],
    ]);
    const byId = new Map(replies.map((reply) => [(reply as { id: number }).id, reply]));
    expect(byId.get(1)).toEqual({ id: 1, result: { ok: { method: "tracks" } } });
    expect(byId.get(2)).toEqual({
      id: 2,
      error: {
        code: "E_PERMISSION",
        message: "media was not granted openExternal",
        data: { permission: "openExternal" },
      },
    });
    expect(byId.get(3)).toEqual({ id: 3, result: null });
  });

  test("a plain host failure reaches the frame as unavailable", async () => {
    const invoke: Invoke = () => Promise.reject("the desktop is still starting");
    expect(await answer(forwardTo(invoke, "media", "main"), { id: 4, method: "ui.toast" })).toEqual(
      { id: 4, error: { code: "E_UNAVAILABLE", message: "the desktop is still starting" } },
    );
  });

  test("the ready reply carries the whole frame context", () => {
    const context = {
      extension: "media",
      page: "main",
      params: { album: 1 },
      hostApi: 1,
      granted: { host: [], filesystem: [], network: [] },
      appearance: { theme: "dark", scheme: "dark" as const },
      presenter: { active: false },
      locale: "en-GB",
    };
    expect(readyMessage(context)).toEqual({ v: 1, type: "ready", ...context });
  });

  test("backend topics reach only the pages that subscribed", () => {
    const event = { extension: "media", pages: ["main"], topic: "tick", payload: 3 };
    expect(topicMessage(event, "media", "main")).toEqual({
      event: "backend.event",
      payload: { topic: "tick", payload: 3 },
    });
    expect(topicMessage(event, "media", "album")).toBeNull();
    expect(topicMessage(event, "usage", "main")).toBeNull();
    expect(topicMessage({ ...event, pages: "main" }, "media", "main")).toBeNull();
    expect(
      progressMessage({ extension: "media", message: "Scanning", fraction: 0.5 }, "media"),
    ).toEqual({ event: "backend.progress", payload: { message: "Scanning", fraction: 0.5 } });
    expect(progressMessage({ extension: "usage", message: "x" }, "media")).toBeNull();
  });

  test("the frame learns the resolved colour scheme", () => {
    const dark = new Set(["dark", "nord"]);
    expect(appearanceOf("nord", false, dark)).toEqual({ theme: "nord", scheme: "dark" });
    expect(appearanceOf("light", true, dark)).toEqual({ theme: "light", scheme: "light" });
    expect(appearanceOf(undefined, true, dark)).toEqual({ theme: "system", scheme: "dark" });
  });

  test("interface requests are validated before the host acts on them", () => {
    const from = { extension: "media", page: "main" };
    expect(interfaceRequest({ ...from, action: "toast", message: "Saved", kind: "nope" })).toEqual({
      ...from,
      action: "toast",
      kind: "info",
      message: "Saved",
    });
    expect(
      interfaceRequest({ ...from, action: "confirm", request: 9, title: "Delete?", body: null }),
    ).toEqual({
      ...from,
      action: "confirm",
      request: 9,
      title: "Delete?",
      body: null,
      confirmLabel: null,
      cancelLabel: null,
      destructive: false,
    });
    expect(interfaceRequest({ ...from, action: "badge", badge: 3 })).toEqual({
      ...from,
      action: "badge",
      badge: 3,
    });
    expect(interfaceRequest({ ...from, action: "open", target: "album" })).toEqual({
      ...from,
      action: "open",
      target: "album",
      params: null,
    });
    for (const payload of [
      null,
      { action: "toast", message: "x" },
      { ...from, action: "toast" },
      { ...from, action: "confirm", title: "x" },
      { ...from, action: "badge", badge: {} },
      { ...from, action: "launch" },
    ]) {
      expect(interfaceRequest(payload)).toBeNull();
    }
  });
});
