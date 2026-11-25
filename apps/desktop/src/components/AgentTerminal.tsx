import { FitAddon } from "@xterm/addon-fit";
import { type ITheme, Terminal } from "@xterm/xterm";
import { useEffect, useRef, useState } from "react";
import "@xterm/xterm/css/xterm.css";

import { ConfirmDialog, Kbd, Notice } from "@veronica/ui";
import { ipc } from "../lib/ipc";
import "./AgentTerminal.css";

const PASTE_LIMIT = 8192;

function resolveColor(token: string, property: "color" | "backgroundColor") {
  const probe = document.createElement("span");
  probe.style.position = "absolute";
  probe.style.visibility = "hidden";
  probe.style[property] = `var(${token})`;
  document.body.appendChild(probe);
  const value = getComputedStyle(probe)[property];
  probe.remove();
  const canvas = document.createElement("canvas");
  canvas.width = 1;
  canvas.height = 1;
  const context = canvas.getContext("2d");
  if (!context) {
    return value;
  }
  context.fillStyle = value;
  context.fillRect(0, 0, 1, 1);
  const [r, g, b, a] = context.getImageData(0, 0, 1, 1).data;
  return `rgba(${r}, ${g}, ${b}, ${(a / 255).toFixed(3)})`;
}

function terminalTheme(): ITheme {
  return {
    background: resolveColor("--bg-inset", "backgroundColor"),
    foreground: resolveColor("--text-1", "color"),
    cursor: resolveColor("--accent", "color"),
    cursorAccent: resolveColor("--bg-inset", "backgroundColor"),
    selectionBackground: resolveColor("--accent-tint", "backgroundColor"),
  };
}

export function AgentTerminal({ id, title }: { id: string; title: string }) {
  const host = useRef<HTMLDivElement>(null);
  const terminalRef = useRef<Terminal | null>(null);
  const [error, setError] = useState("");
  const [pendingPaste, setPendingPaste] = useState<string | null>(null);
  const [status, setStatus] = useState("running");

  useEffect(() => {
    if (!host.current) {
      return;
    }
    const mono = getComputedStyle(document.documentElement).getPropertyValue("--mono").trim();
    const terminal = new Terminal({
      scrollback: 1500,
      fontSize: 13,
      fontFamily: mono || "monospace",
      convertEol: false,
      allowProposedApi: false,
      linkHandler: { activate: () => {} },
      theme: terminalTheme(),
    });
    terminalRef.current = terminal;
    const fit = new FitAddon();
    terminal.loadAddon(fit);
    terminal.open(host.current);
    fit.fit();
    const block = terminal.parser.registerOscHandler(52, () => true);
    let live = true;
    let cursor = 0;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const input = terminal.onData((bytes) => {
      void ipc.agentInput(id, bytes).catch((e) => {
        if (live) {
          setError(String(e));
        }
      });
    });
    const resize = () => {
      if (!live) {
        return;
      }
      try {
        fit.fit();
        void ipc.agentResize(id, terminal.cols, terminal.rows).catch(() => {});
      } catch {
        return;
      }
    };
    const observer = new ResizeObserver(resize);
    observer.observe(host.current);
    const themeObserver = new MutationObserver(() => {
      terminal.options.theme = terminalTheme();
    });
    themeObserver.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["data-theme", "class", "style"],
    });
    const poll = async () => {
      if (!live) {
        return;
      }
      if (document.hidden) {
        timer = setTimeout(() => void poll(), 1000);
        return;
      }
      try {
        const out = await ipc.agentOutput(id, cursor);
        if (!live) {
          return;
        }
        if (cursor < out.base) {
          terminal.write("\r\n[Older output trimmed]\r\n");
        }
        cursor = out.cursor;
        if (out.bytes) {
          await new Promise<void>((resolve) =>
            terminal.write(
              Uint8Array.from(atob(out.bytes), (c) => c.charCodeAt(0)),
              resolve,
            ),
          );
        }
        if (out.status !== "running") {
          terminal.write(`\r\n[${out.status}]\r\n`);
          setStatus(out.status);
          return;
        }
      } catch (e) {
        if (live) {
          setError(String(e));
        }
        return;
      }
      if (live) {
        timer = setTimeout(() => void poll(), 150);
      }
    };
    void poll();
    const paste = (event: ClipboardEvent) => {
      event.preventDefault();
      event.stopImmediatePropagation();
      const text = event.clipboardData?.getData("text/plain") || "";
      if (text.length > PASTE_LIMIT) {
        setError("Paste is limited to 8 KiB.");
        return;
      }
      if (/[\r\n]/.test(text)) {
        setPendingPaste(text);
        return;
      }
      terminal.paste(text);
    };
    const element = host.current;
    element.addEventListener("paste", paste, true);
    return () => {
      live = false;
      clearTimeout(timer);
      element.removeEventListener("paste", paste, true);
      observer.disconnect();
      themeObserver.disconnect();
      input.dispose();
      block.dispose();
      terminal.dispose();
      terminalRef.current = null;
    };
  }, [id]);

  return (
    <section className="agent-terminal-card" aria-label={`${title} terminal`}>
      <header className="agent-terminal-head">
        <span className="agent-terminal-title">{title}</span>
        <span className="agent-terminal-status">
          {status === "running" ? "Interactive" : status}
        </span>
      </header>
      {error && (
        <div className="agent-terminal-notice">
          <Notice tone="critical" compact>
            {error}
          </Notice>
        </div>
      )}
      <div className="agent-terminal" ref={host} />
      <footer className="agent-terminal-foot">
        <span>
          <Kbd>Ctrl C</Kbd> interrupts
        </span>
        <span>Close terminal stops its process group. Output stays in memory.</span>
      </footer>
      <ConfirmDialog
        open={pendingPaste !== null}
        onClose={() => setPendingPaste(null)}
        onConfirm={() => {
          if (pendingPaste !== null) {
            terminalRef.current?.paste(pendingPaste);
          }
          setPendingPaste(null);
          terminalRef.current?.focus();
        }}
        title="Paste multiple lines?"
        message="Each line may run as a command in this agent terminal."
        confirmLabel="Paste"
        destructive
      />
    </section>
  );
}
