import { useEffect, useRef } from "react";

import {
  acceptsHello,
  type BridgeMethods,
  ECHO_METHODS,
  frameSource,
  HOST_API,
  serve,
} from "./bridge";
import "./ExtensionFrame.css";

interface ExtensionFrameProps {
  extension: string;
  entry: string;
  title: string;
  methods?: BridgeMethods;
}

export function ExtensionFrame({
  extension,
  entry,
  title,
  methods = ECHO_METHODS,
}: ExtensionFrameProps) {
  const frame = useRef<HTMLIFrameElement>(null);
  const source = frameSource(extension, entry);

  useEffect(() => {
    let port: MessagePort | null = null;
    const onMessage = (event: MessageEvent) => {
      const target = frame.current?.contentWindow ?? null;
      if (target === null || !acceptsHello(event, target)) {
        return;
      }
      port?.close();
      const channel = new MessageChannel();
      port = channel.port1;
      serve(port, methods);
      target.postMessage({ v: 1, type: "ready", hostApi: HOST_API, extension, page: entry }, "*", [
        channel.port2,
      ]);
    };
    window.addEventListener("message", onMessage);
    return () => {
      window.removeEventListener("message", onMessage);
      port?.close();
    };
  }, [extension, entry, methods]);

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
