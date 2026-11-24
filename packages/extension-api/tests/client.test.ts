import { afterEach, describe, expect, test } from "bun:test";

import {
  Client,
  connect,
  ExtensionError,
  type FrameContext,
  type FrameEvent,
  type FrameWindow,
  HOST_METHODS,
  isExtensionError,
  type RequestMessage,
} from "../src";

const context: FrameContext = {
  extension: "media",
  page: "main",
  params: { album: 3 },
  hostApi: 1,
  granted: { host: ["clipboard:write"], filesystem: [], network: [] },
  appearance: { theme: "dark", scheme: "dark" },
  presenter: { active: false },
  locale: "en-GB",
};

const open: MessagePort[] = [];

afterEach(() => {
  for (const port of open.splice(0)) {
    port.close();
  }
});

function channel(): MessageChannel {
  const created = new MessageChannel();
  open.push(created.port1, created.port2);
  return created;
}

type Answer = (request: RequestMessage) => unknown;

function host(answer: Answer): { client: Client; host: MessagePort; seen: RequestMessage[] } {
  const { port1, port2 } = channel();
  const seen: RequestMessage[] = [];
  port1.onmessage = (event) => {
    const request = event.data as RequestMessage;
    seen.push(request);
    const reply = answer(request);
    if (reply !== undefined) {
      port1.postMessage({ id: request.id, ...(reply as object) });
    }
  };
  return { client: new Client(port2, context), host: port1, seen };
}

function nextTick(): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, 5));
}

describe("handshake", () => {
  function fakeWindow(
    respond: (target: FrameWindow, listener: (event: FrameEvent) => void) => void,
  ) {
    const listeners = new Set<(event: FrameEvent) => void>();
    const posted: unknown[] = [];
    const scope: FrameWindow = {
      parent: {
        postMessage: (message) => {
          posted.push(message);
          for (const listener of listeners) {
            respond(scope, listener);
          }
        },
      },
      addEventListener: (_type, listener) => listeners.add(listener),
      removeEventListener: (_type, listener) => listeners.delete(listener),
    };
    return { scope, posted, listeners };
  }

  test("a hello is answered by ready with a port from the parent window", async () => {
    const { port2 } = channel();
    const { scope, posted, listeners } = fakeWindow((target, listener) => {
      listener({ source: {}, data: { v: 1, type: "ready", ...context }, ports: [port2] });
      listener({ source: target.parent, data: { type: "ready" }, ports: [port2] });
      listener({ source: target.parent, data: { v: 1, type: "ready", ...context }, ports: [] });
      listener({
        source: target.parent,
        data: { v: 1, type: "ready", ...context },
        ports: [port2],
      });
    });
    const client = await connect({ window: scope });
    expect(posted).toEqual([{ v: 1, type: "hello", apiVersion: 1 }]);
    expect(client.context).toEqual(context);
    expect(listeners.size).toBe(0);
    client.close();
  });

  test("a silent host times out with a typed error", async () => {
    const { scope, listeners } = fakeWindow(() => {});
    const failure = await connect({ window: scope, handshakeTimeoutMs: 10 }).catch((e) => e);
    expect(isExtensionError(failure, "E_TIMEOUT")).toBe(true);
    expect(listeners.size).toBe(0);
  });
});

describe("requests", () => {
  test("every typed method sends the documented wire shape", async () => {
    const { client, seen } = host((request) => ({ result: request.method }));
    await client.backend.call("tracks", { limit: 2 }, { timeoutMs: 500 });
    await client.storage.get("k");
    await client.storage.set("k", 1);
    await client.storage.remove("k");
    await client.storage.keys();
    await client.settings.get("volume");
    await client.settings.set("volume", 3);
    await client.contracts.request("media.player@1", { verb: "play" });
    await client.nav.open("album", { id: 9 });
    await client.nav.setBadge(4);
    await client.nav.setTitle("Albums");
    await client.ui.toast("Saved", "success");
    await client.ui.confirm({ title: "Delete?", destructive: true });
    await client.ui.openExternal("https://example.org");
    await client.clipboard.writeText("copied");
    await client.files.url("/home/u/Music/a.flac");
    const unsubscribe = await client.backend.subscribe("tick", () => {});
    await client.contracts.subscribe("media.player@1", () => {});
    unsubscribe();
    await nextTick();

    expect(seen.map((request) => [request.method, request.params])).toEqual([
      ["backend.call", { method: "tracks", params: { limit: 2 }, timeoutMs: 500 }],
      ["storage.get", { key: "k" }],
      ["storage.set", { key: "k", value: 1 }],
      ["storage.remove", { key: "k" }],
      ["storage.keys", {}],
      ["settings.get", { key: "volume" }],
      ["settings.set", { key: "volume", value: 3 }],
      ["contracts.request", { id: "media.player@1", params: { verb: "play" } }],
      ["nav.open", { page: "album", params: { id: 9 } }],
      ["nav.setBadge", { badge: 4 }],
      ["nav.setTitle", { title: "Albums" }],
      ["ui.toast", { message: "Saved", kind: "success" }],
      ["ui.confirm", { title: "Delete?", destructive: true }],
      ["ui.openExternal", { url: "https://example.org" }],
      ["clipboard.writeText", { text: "copied" }],
      ["files.url", { path: "/home/u/Music/a.flac" }],
      ["backend.subscribe", { topic: "tick" }],
      ["contracts.subscribe", { id: "media.player@1" }],
      ["backend.unsubscribe", { topic: "tick" }],
    ]);
    expect(new Set(seen.map((request) => request.method))).toEqual(new Set(HOST_METHODS));
    expect(new Set(seen.map((request) => request.id)).size).toBe(seen.length);
    client.close();
  });

  test("replies resolve their own request even out of order", async () => {
    const { client, host: port, seen } = host(() => undefined);
    const first = client.storage.get("a");
    const second = client.storage.get("b");
    await nextTick();
    port.postMessage({ id: seen[1].id, result: "B" });
    port.postMessage({ id: seen[0].id, result: "A" });
    expect(await Promise.all([first, second])).toEqual(["A", "B"]);
    client.close();
  });

  test("host failures arrive as typed errors with their data", async () => {
    const { client } = host(() => ({
      error: {
        code: "E_PERMISSION",
        message: "media was not granted openExternal",
        data: { permission: "openExternal" },
      },
    }));
    const failure = await client.ui.openExternal("https://example.org").catch((e) => e);
    expect(failure).toBeInstanceOf(ExtensionError);
    expect(failure.code).toBe("E_PERMISSION");
    expect(failure.data).toEqual({ permission: "openExternal" });
    client.close();
  });

  test("an unanswered request times out and a late reply is ignored", async () => {
    const { port1, port2 } = channel();
    const client = new Client(port2, context, { timeoutMs: 10 });
    const failure = await client.storage.keys().catch((e) => e);
    expect(isExtensionError(failure, "E_TIMEOUT")).toBe(true);
    port1.postMessage({ id: 1, result: [] });
    await nextTick();
    client.close();
  });

  test("closing rejects what is in flight and refuses new calls", async () => {
    const { client } = host(() => undefined);
    const inflight = client.storage.keys();
    client.close();
    expect(isExtensionError(await inflight.catch((e) => e), "E_UNAVAILABLE")).toBe(true);
    expect(isExtensionError(await client.storage.keys().catch((e) => e), "E_UNAVAILABLE")).toBe(
      true,
    );
  });
});

describe("events", () => {
  test("host events reach listeners and keep the context current", async () => {
    const { client, host: port } = host(() => ({ result: null }));
    const scheme: string[] = [];
    const stop = client.on("appearance.changed", (appearance) => scheme.push(appearance.scheme));
    port.postMessage({ event: "appearance.changed", payload: { theme: "light", scheme: "light" } });
    await nextTick();
    stop();
    port.postMessage({ event: "appearance.changed", payload: { theme: "dark", scheme: "dark" } });
    port.postMessage({ event: "presenter.changed", payload: { active: true } });
    await nextTick();
    expect(scheme).toEqual(["light"]);
    expect(client.context.appearance.theme).toBe("dark");
    expect(client.context.presenter.active).toBe(true);
    client.close();
  });

  test("topic events go only to their own subscribers", async () => {
    const { client, host: port, seen } = host(() => ({ result: null }));
    const ticks: unknown[] = [];
    const contract: unknown[] = [];
    await client.backend.subscribe("tick", (payload) => ticks.push(payload));
    await client.backend.subscribe("tick", (payload) => ticks.push(payload));
    await client.contracts.subscribe("media.player@1", (payload) => contract.push(payload));
    port.postMessage({ event: "backend.event", payload: { topic: "tick", payload: 1 } });
    port.postMessage({ event: "backend.event", payload: { topic: "other", payload: 2 } });
    port.postMessage({
      event: "backend.event",
      payload: { topic: "contract:media.player@1", payload: "playing" },
    });
    await nextTick();
    expect(ticks).toEqual([1, 1]);
    expect(contract).toEqual(["playing"]);
    expect(seen.filter((request) => request.method === "backend.subscribe")).toHaveLength(1);
    client.close();
  });

  test("a refused subscription leaves no handler behind", async () => {
    const { client, host: port } = host(() => ({
      error: { code: "E_PERMISSION", message: "not used" },
    }));
    const seen: unknown[] = [];
    const failure = await client.contracts.subscribe("x.y@1", (p) => seen.push(p)).catch((e) => e);
    expect(isExtensionError(failure, "E_PERMISSION")).toBe(true);
    port.postMessage({ event: "backend.event", payload: { topic: "contract:x.y@1", payload: 1 } });
    await nextTick();
    expect(seen).toEqual([]);
    client.close();
  });
});

describe("errors", () => {
  test("anything thrown becomes a typed error", () => {
    expect(ExtensionError.from({ code: "E_BACKEND", message: "boom" }).code).toBe("E_BACKEND");
    expect(ExtensionError.from({ code: "E_NOPE", message: "x" }).code).toBe("E_UNAVAILABLE");
    expect(ExtensionError.from(new Error("gone")).message).toBe("gone");
    expect(ExtensionError.from("plain").message).toBe("plain");
    expect(new ExtensionError("E_INVALID", "bad", { field: "key" }).toWire()).toEqual({
      code: "E_INVALID",
      message: "bad",
      data: { field: "key" },
    });
  });
});
