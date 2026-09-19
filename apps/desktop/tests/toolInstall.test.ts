import { describe, expect, test } from "bun:test";

import { installNotice } from "../src/lib/toolInstall";

describe("tool install result copy", () => {
  test("a completed install names the version and location", () => {
    expect(installNotice("OpenSSH", {
      outcome: "installed",
      path: "/usr/bin/ssh",
      version: "OpenSSH_9.6",
    })).toEqual({
      tone: "good",
      text: "Installed OpenSSH OpenSSH_9.6 at /usr/bin/ssh.",
    });
  });

  test("a command Veronica did not run is guidance rather than an error", () => {
    expect(installNotice("Codex", {
      outcome: "notRun",
      command: "npm install -g @openai/codex",
      instruction: "Install Codex.",
      reason: "The npm prefix is not writable.",
    })).toEqual({
      tone: "warn",
      text: "The npm prefix is not writable. npm install -g @openai/codex",
    });
  });

  test("a manual route shows its standalone instruction", () => {
    expect(installNotice("Herdr", {
      outcome: "notRun",
      command: null,
      instruction: "Install Herdr and put it on PATH.",
      reason: "Herdr has no install Veronica can drive.",
    }).text).toContain("Install Herdr and put it on PATH.");
  });
});
