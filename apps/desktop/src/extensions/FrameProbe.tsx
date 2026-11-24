import { invoke } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { useEffect, useMemo } from "react";

import { type BridgeHandler, ECHO_METHODS, localHandler } from "./bridge";
import { ExtensionFrame } from "./ExtensionFrame";

const REPORT_EVENT = "extension-frame-probe";
const CONTROL_SETTING = "frameProbeControl";
const SETTLE_MILLISECONDS = 1500;
const DEADLINE_MILLISECONDS = 45000;

interface NativeHandlers {
  webkit?: { messageHandlers?: { ipc?: { postMessage: (message: string) => void } } };
}

let concluded = false;
let echoes = 0;

function forgedControl(): string {
  const handler = (window as NativeHandlers).webkit?.messageHandlers?.ipc;
  if (handler === undefined) {
    return "handler is not exposed to the host";
  }
  handler.postMessage(
    JSON.stringify({
      cmd: "settings_set",
      callback: 1,
      error: 2,
      payload: { key: "frameProbeForgedControl", value: true },
      options: {},
      __TAURI_INVOKE_KEY__: "host-control-guess",
    }),
  );
  return "posted";
}

async function conclude(report: unknown): Promise<void> {
  if (concluded) {
    return;
  }
  concluded = true;
  const control = await invoke("settings_set", { key: CONTROL_SETTING, value: true }).then(
    () => "ok",
    (reason: unknown) => `failed: ${String(reason)}`,
  );
  const forged = forgedControl();
  await new Promise((resolve) => setTimeout(resolve, SETTLE_MILLISECONDS));
  await emit(REPORT_EVENT, {
    report,
    host: {
      control,
      forged,
      echoes,
      location: window.location.href,
      frames: window.frames.length,
    },
  });
}

export function FrameProbe() {
  const target = useMemo(() => new URLSearchParams(window.location.search), []);
  const handler = useMemo<BridgeHandler>(
    () =>
      localHandler({
        echo: (params) => {
          echoes += 1;
          return ECHO_METHODS.echo(params);
        },
        report: (params) => {
          void conclude(params);
          return true;
        },
      }),
    [],
  );

  useEffect(() => {
    const deadline = setTimeout(() => void conclude(null), DEADLINE_MILLISECONDS);
    return () => clearTimeout(deadline);
  }, []);

  return (
    <main style={{ position: "fixed", inset: 0 }}>
      <ExtensionFrame
        extension={target.get("extension") ?? ""}
        page={target.get("entry") ?? ""}
        entry={target.get("entry") ?? ""}
        title="Extension frame probe"
        handler={handler}
      />
    </main>
  );
}
