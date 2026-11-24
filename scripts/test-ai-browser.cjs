const { chromium } = require(process.env.VERONICA_PLAYWRIGHT || "playwright");
const { spawn } = require("node:child_process");
const { createServer } = require("node:http");
const fs = require("node:fs");
const path = require("node:path");
const assert = require("node:assert/strict");
(async () => {
  const child = spawn(path.resolve(__dirname, "../target/debug/examples/desktop_controls"), [], {
    stdio: ["pipe", "pipe", "pipe"],
  });
  let serial = 0,
    buffer = "";
  const pending = new Map();
  child.stdout.on("data", (chunk) => {
    buffer += chunk;
    for (let end = buffer.indexOf("\n"); end >= 0; end = buffer.indexOf("\n")) {
      const data = JSON.parse(buffer.slice(0, end));
      buffer = buffer.slice(end + 1);
      const task = pending.get(data.id);
      if (task) {
        pending.delete(data.id);
        data.error ? task.reject(new Error(data.error)) : task.resolve(data.result);
      }
    }
  });
  const rpc = (command, args = {}) =>
    new Promise((resolve, reject) => {
      const id = ++serial;
      pending.set(id, { resolve, reject });
      child.stdin.write(`${JSON.stringify({ id, command, args })}\n`);
    });
  const requests = [];
  const api = createServer((req, res) => {
    let body = "";
    req.on("data", (chunk) => (body += chunk));
    req.on("end", () => {
      requests.push(JSON.parse(body));
      setTimeout(() => {
        res.writeHead(200, { "content-type": "application/json" });
        res.end(
          JSON.stringify({
            choices: [{ message: { content: "Fixture reply: short Hinglish, as requested." } }],
          }),
        );
      }, 500);
    });
  });
  await new Promise((resolve) => api.listen(0, "127.0.0.1", resolve));
  const endpoint = `http://127.0.0.1:${api.address().port}/v1`;
  const browser = await chromium.launch({
    executablePath: process.env.VERONICA_CHROME || "/usr/bin/google-chrome",
    headless: true,
    args: ["--disable-dev-shm-usage"],
  });
  const page = await browser.newPage({ viewport: { width: 1280, height: 960 } });
  const errors = [];
  page.on("pageerror", (error) => errors.push(String(error)));
  try {
    await page.exposeFunction("__controlBridge", async (command, args = {}) => {
      if (command === "settings_all") {
        return { appearance: "dark" };
      }
      if (command === "presenter_state") {
        return { active: false, blurredClasses: [] };
      }
      if (command === "herdr_board") {
        return { installed: false, sessions: [], agents: [], executable: null, error: null };
      }
      if (command === "companion_list") {
        return [];
      }
      if (command === "companion_recording_status") {
        return { recording: false, elapsedSeconds: 0 };
      }
      if (command.startsWith("plugin:")) {
        return 0;
      }
      if (command.startsWith("agent_") || command.startsWith("chat_")) {
        return rpc(command, args);
      }
      throw new Error(`No fixture for ${command}`);
    });
    await page.addInitScript(() => {
      window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
      window.__TAURI_INTERNALS__ = {
        invoke: (command, args) => window.__controlBridge(command, args),
        transformCallback: () => 1,
        unregisterCallback: () => {},
      };
    });
    const base = process.env.VERONICA_UI_URL || "http://127.0.0.1:5183";
    await page.goto(`${base}/?route=herdr`);
    await page.getByLabel("Project directory", { exact: true }).fill("/tmp");
    await page.getByRole("button", { name: "Start agent", exact: true }).click();
    await page.locator(".xterm-helper-textarea").waitFor({ state: "attached" });
    await page.locator(".xterm-helper-textarea").focus();
    await page.keyboard.type("printf 'PTY_UI_OK\\n'", { delay: 10 });
    await page.keyboard.press("Enter");
    await page.waitForFunction(() =>
      document.querySelector(".xterm-screen")?.textContent?.includes("PTY_UI_OK"),
    );
    const owned = await rpc("agent_list");
    assert.equal(owned.length, 1);
    fs.mkdirSync("/tmp/veronica-ai-preview", { recursive: true });
    await page.screenshot({ path: "/tmp/veronica-ai-preview/agents.png", fullPage: true });
    await page.getByRole("button", { name: "Close terminal", exact: true }).click();
    assert.equal((await rpc("agent_list")).length, 0);
    assert.equal(fs.existsSync(`/proc/${owned[0].pid}`), false);
    await page.goto(`${base}/?route=companion`);
    await page.getByRole("button", { name: "AI conversation", exact: true }).click();
    await page.getByRole("button", { name: "Enable for this window", exact: true }).waitFor();
    assert.equal((await rpc("chat_view")).enabled, false);
    await page.getByRole("combobox").first().selectOption("compatible");
    await page.getByLabel("API base URL", { exact: true }).fill(endpoint);
    await page.getByLabel("Model identifier", { exact: true }).fill("fixture-model");
    await page.getByLabel("API key (session only)", { exact: true }).fill("ephemeral-test-key");
    await page.getByLabel("I understand this provider receives", { exact: false }).check();
    await page.locator("summary").filter({ hasText: "Local memory & data controls" }).click();
    await page
      .getByLabel("Saved preferences and context", { exact: true })
      .fill("I prefer short Hinglish replies.");
    await page.getByRole("button", { name: "Save connection", exact: true }).click();
    await page.getByRole("button", { name: "Enable for this window", exact: true }).click();
    await page.getByRole("button", { name: "Stop & switch off", exact: true }).waitFor();
    await page
      .getByLabel("Message Companion", { exact: true })
      .fill("My city in this test is Bhopal.");
    await page.getByRole("button", { name: "Save privately, no reply", exact: true }).click();
    await page.getByText("Saved privately. No model request was made.", { exact: true }).waitFor();
    assert.equal(requests.length, 0);
    await page.getByLabel("Message Companion", { exact: true }).fill("Please remember my city.");
    await page.getByRole("button", { name: "Preview exact context", exact: true }).click();
    await page.locator(".chat-context-preview").filter({ hasText: "Bhopal" }).waitFor();
    assert.equal(requests.length, 0);
    await page.getByRole("button", { name: "Send message", exact: true }).click();
    await page.getByText("Fixture reply: short Hinglish, as requested.", { exact: true }).waitFor();
    assert.equal(requests.length, 1);
    assert.ok(JSON.stringify(requests[0]).includes("Bhopal"));
    assert.ok(JSON.stringify(requests[0]).includes("short Hinglish"));
    await page.screenshot({ path: "/tmp/veronica-ai-preview/companion.png", fullPage: true });
    const exported = await rpc("chat_export");
    assert.ok(!fs.readFileSync(exported, "utf8").includes("ephemeral-test-key"));
    await page
      .getByLabel("Message Companion", { exact: true })
      .fill("This request will be stopped.");
    await page.getByRole("button", { name: "Send message", exact: true }).click();
    await page.getByRole("button", { name: "Stop & switch off", exact: true }).click();
    await page.waitForTimeout(700);
    const after = await rpc("chat_view");
    assert.equal(after.enabled, false);
    assert.equal(after.document.messages.filter((m) => m.role === "assistant").length, 1);
    await page.getByRole("button", { name: "Notes", exact: true }).click();
    await page.getByRole("button", { name: "AI conversation", exact: true }).click();
    await page.getByRole("button", { name: "Enable for this window", exact: true }).waitFor();
    assert.equal(await page.getByLabel("API key (session only)", { exact: true }).inputValue(), "");
    await rpc("close_window");
    assert.equal((await rpc("agent_list")).length, 0);
    assert.equal((await rpc("chat_view")).enabled, false);
    assert.deepEqual(errors, []);
    process.stdout.write(
      "Passed real native-backed terminal input/close and Companion configure/journal/context/send/cancel/export/off-on-reopen browser flows. Cloud API was a loopback fixture.\n",
    );
  } finally {
    await browser.close();
    api.close();
    child.stdin.end();
    setTimeout(() => {
      if (child.exitCode === null) {
        child.kill("SIGKILL");
      }
    }, 2000).unref();
  }
})().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
