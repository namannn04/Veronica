import { invoke } from "@tauri-apps/api/core";
import type { BackendState, EventMessage, Granted } from "@veronica/extension-api";
import { useEffect, useMemo, useRef } from "react";

import { ipc, isDesktopApp, listenEvent } from "../lib/ipc";
import { APPEARANCES } from "../lib/preferences";
import {
  acceptsHello,
  appearanceOf,
  type BridgeHandler,
  forwardTo,
  frameSource,
  HOST_API,
  progressMessage,
  readyMessage,
  serve,
  stateMessage,
  topicMessage,
} from "./bridge";
import "./ExtensionFrame.css";

const NO_GRANTS: Granted = { host: [], filesystem: [], network: [] };
const DARK_THEMES = new Set<string>(
  APPEARANCES.filter((appearance) => appearance.family === "dark").map(
    (appearance) => appearance.id,
  ),
);
const DARK_QUERY = "(prefers-color-scheme: dark)";

interface ExtensionFrameProps {
  extension: string;
  page: string;
  entry: string;
  title: string;
  params?: unknown;
  granted?: Granted;
  state?: BackendState;
  handler?: BridgeHandler;
  visible?: boolean;
}

function currentAppearance() {
  return appearanceOf(
    document.documentElement.dataset.theme,
    window.matchMedia(DARK_QUERY).matches,
    DARK_THEMES,
  );
}

function currentPresenter() {
  return { active: document.documentElement.dataset.presenter === "true" };
}

export function ExtensionFrame({
  extension,
  page,
  entry,
  title,
  params = null,
  granted = NO_GRANTS,
  state,
  handler,
  visible = true,
}: ExtensionFrameProps) {
  const frame = useRef<HTMLIFrameElement>(null);
  const port = useRef<MessagePort | null>(null);
  const launch = useRef({ params, granted });
  const shown = useRef(visible);
  shown.current = visible;
  launch.current = { params, granted };
  const source = frameSource(extension, entry);
  const hosted = handler === undefined;
  const bridge = useMemo(
    () => handler ?? forwardTo(invoke, extension, page),
    [handler, extension, page],
  );

  useEffect(() => {
    const onMessage = (event: MessageEvent) => {
      const target = frame.current?.contentWindow ?? null;
      if (target === null || !acceptsHello(event, target)) {
        return;
      }
      port.current?.close();
      const channel = new MessageChannel();
      port.current = channel.port1;
      serve(channel.port1, bridge);
      const ready = readyMessage({
        extension,
        page,
        params: launch.current.params,
        hostApi: HOST_API,
        granted: launch.current.granted,
        appearance: currentAppearance(),
        presenter: currentPresenter(),
        locale: navigator.language,
      });
      target.postMessage(ready, "*", [channel.port2]);
    };
    window.addEventListener("message", onMessage);
    return () => {
      window.removeEventListener("message", onMessage);
      port.current?.close();
      port.current = null;
    };
  }, [bridge, extension, page]);

  useEffect(() => {
    if (!hosted || !isDesktopApp()) {
      return;
    }
    void ipc.extPageOpened(extension, page).catch(() => undefined);
    return () => {
      void ipc.extPageClosed(extension, page).catch(() => undefined);
    };
  }, [hosted, extension, page]);

  useEffect(() => {
    if (!hosted) {
      return;
    }
    const post = (message: EventMessage | null) => {
      if (message !== null) {
        port.current?.postMessage(message);
      }
    };
    const events = listenEvent("extension-event", (event) =>
      post(topicMessage(event.payload, extension, page)),
    );
    const progress = listenEvent("extension-progress", (event) =>
      post(progressMessage(event.payload, extension)),
    );
    const sendAppearance = () =>
      post({ event: "appearance.changed", payload: currentAppearance() });
    const sendVisibility = () =>
      post({
        event: "visibility.changed",
        payload: { visible: shown.current && !document.hidden },
      });
    let presenter = currentPresenter().active;
    const observer = new MutationObserver(() => {
      sendAppearance();
      const active = currentPresenter().active;
      if (active !== presenter) {
        presenter = active;
        post({ event: "presenter.changed", payload: { active } });
      }
    });
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["data-theme", "data-presenter"],
    });
    const scheme = window.matchMedia(DARK_QUERY);
    scheme.addEventListener("change", sendAppearance);
    document.addEventListener("visibilitychange", sendVisibility);
    return () => {
      void events.then((stop) => stop());
      void progress.then((stop) => stop());
      observer.disconnect();
      scheme.removeEventListener("change", sendAppearance);
      document.removeEventListener("visibilitychange", sendVisibility);
    };
  }, [hosted, extension, page]);

  useEffect(() => {
    if (hosted) {
      port.current?.postMessage({
        event: "visibility.changed",
        payload: { visible: visible && !document.hidden },
      });
    }
  }, [hosted, visible]);

  useEffect(() => {
    if (state !== undefined) {
      port.current?.postMessage(stateMessage(state));
    }
  }, [state]);

  if (source === null) {
    return null;
  }
  return (
    <iframe
      ref={frame}
      className="extension-frame"
      title={title}
      sandbox="allow-scripts allow-forms"
      referrerPolicy="no-referrer"
      src={source}
    />
  );
}
