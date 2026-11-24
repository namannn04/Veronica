import { describe, expect, test } from "bun:test";

import {
  acceptsHello,
  answer,
  ECHO_METHODS,
  frameSource,
  isHello,
  serve,
} from "../src/extensions/bridge";
import { isFrameProbeSurface } from "../src/lib/surfaces";

const hello = { v: 1, type: "hello", apiVersion: 1 };

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
    expect(await answer(ECHO_METHODS, { id: 1, method: "echo", params: { a: [1, 2] } })).toEqual({
      id: 1,
      result: { a: [1, 2] },
    });
    for (const method of ["constructor", "toString", "__proto__", "hasOwnProperty", "invoke"]) {
      expect(await answer(ECHO_METHODS, { id: "x", method })).toEqual({
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
      expect(await answer(ECHO_METHODS, message)).toBeNull();
    }
    const failing = {
      fail: () => {
        throw new Error("offline");
      },
    };
    expect(await answer(failing, { id: 7, method: "fail" })).toEqual({
      id: 7,
      error: { code: "E_UNAVAILABLE", message: "offline" },
    });
  });

  test("a served port answers over the channel", async () => {
    const channel = new MessageChannel();
    serve(channel.port1, ECHO_METHODS);
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
});
