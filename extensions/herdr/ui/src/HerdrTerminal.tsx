import type { Client } from "@veronica/extension-api";
import { Button, Notice } from "@veronica/ui";
import { FitAddon } from "@xterm/addon-fit";
import { type ITheme, Terminal } from "@xterm/xterm";
import { useEffect, useRef, useState } from "react";
import "@xterm/xterm/css/xterm.css";
import type { TerminalChunk } from "./types";
import "./HerdrTerminal.css";

function token(name: string, fallback: string): string {
  const value = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  return value || fallback;
}

function theme(): ITheme {
  return {
    background: token("--bg-inset", "#121115"),
    foreground: token("--text-1", "#f4f2ee"),
    cursor: token("--accent", "#d97757"),
  };
}

function decode(base64: string): Uint8Array {
  return Uint8Array.from(atob(base64), (character) => character.charCodeAt(0));
}

export function HerdrTerminal({
  client,
  id,
  title,
  onClose,
}: {
  client: Client;
  id: string;
  title: string;
  onClose: () => void;
}) {
  const host = useRef<HTMLDivElement>(null);
  const [status, setStatus] = useState("running");
  const [error, setError] = useState("");

  useEffect(() => {
    if (host.current === null) {
      return;
    }
    const terminal = new Terminal({
      scrollback: 1500,
      fontSize: 13,
      fontFamily: token("--mono", "monospace"),
      linkHandler: { activate: () => {} },
      theme: theme(),
    });
    const fit = new FitAddon();
    terminal.loadAddon(fit);
    terminal.open(host.current);
    fit.fit();
    let live = true;
    let cursor = 0;
    const write = (chunk: TerminalChunk) => {
      if (!live || chunk.id !== id) {
        return;
      }
      if (chunk.bytes && (chunk.cursor ?? 0) > cursor) {
        cursor = chunk.cursor ?? cursor;
        terminal.write(decode(chunk.bytes));
      }
      setStatus(chunk.status);
    };
    const unsubscribe = client.backend.subscribe<TerminalChunk>("terminal", write);
    void client.backend
      .call<TerminalChunk>("output", { id, cursor: 0 })
      .then((chunk) => write({ ...chunk, id }))
      .catch((reason) => setError(String(reason)));
    const input = terminal.onData((bytes) => {
      void client.backend.call("input", { id, bytes }).catch((reason) => setError(String(reason)));
    });
    const observer = new ResizeObserver(() => {
      fit.fit();
      void client.backend
        .call("resize", { id, cols: terminal.cols, rows: terminal.rows })
        .catch(() => {});
    });
    observer.observe(host.current);
    return () => {
      live = false;
      observer.disconnect();
      input.dispose();
      void unsubscribe.then((stop) => stop());
      terminal.dispose();
    };
  }, [client, id]);

  const stop = () => {
    void client.backend
      .call("stop", { id })
      .then(onClose)
      .catch((reason) => setError(String(reason)));
  };

  return (
    <section className="agent-terminal-card" aria-label={title}>
      <div className="agent-terminal-head">
        <span className="agent-terminal-title">{title}</span>
        <span className="agent-terminal-status">{status}</span>
        <Button size="small" variant="plain" onClick={stop}>
          Stop
        </Button>
      </div>
      {error && (
        <div className="agent-terminal-notice">
          <Notice tone="critical" compact>
            {error}
          </Notice>
        </div>
      )}
      <div className="agent-terminal" ref={host} />
    </section>
  );
}
