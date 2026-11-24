import {
  type Appearance,
  type BackendState,
  type EventMessage,
  ExtensionError,
  type FrameContext,
  HOST_API,
  type Ready,
  type ReplyMessage,
  type ToastKind,
  WIRE_VERSION,
} from "@veronica/extension-api";

export { HOST_API };

const EXTENSION_ID = /^[a-z][a-z0-9-]{1,31}$/;
const ENTRY_SEGMENT = /^[A-Za-z0-9_-][A-Za-z0-9._-]*$/;
const TOAST_KINDS: readonly ToastKind[] = ["info", "success", "warning", "error"];

export type BridgeHandler = (method: string, params: unknown) => unknown;
export type BridgeMethod = (params: unknown) => unknown;
export type BridgeMethods = Readonly<Record<string, BridgeMethod>>;
export type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

export const ECHO_METHODS: BridgeMethods = { echo: (params) => params };

export function frameSource(extension: string, entry: string): string | null {
  const segments = entry.split("/");
  if (!EXTENSION_ID.test(extension) || !segments.every((segment) => ENTRY_SEGMENT.test(segment))) {
    return null;
  }
  return `veronica-ext://${extension}/${segments.join("/")}`;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

export function isHello(data: unknown): boolean {
  return (
    isRecord(data) &&
    data.v === WIRE_VERSION &&
    data.type === "hello" &&
    data.apiVersion === HOST_API
  );
}

export function acceptsHello(
  event: { source: unknown; origin: string; data: unknown },
  frame: unknown,
): boolean {
  return (
    frame !== null &&
    frame !== undefined &&
    event.source === frame &&
    event.origin === "null" &&
    isHello(event.data)
  );
}

export function readyMessage(context: FrameContext): Ready {
  return { v: WIRE_VERSION, type: "ready", ...context };
}

export function localHandler(methods: BridgeMethods): BridgeHandler {
  return (method, params) => {
    if (!Object.hasOwn(methods, method)) {
      throw new ExtensionError("E_INVALID", `unknown method ${method}`);
    }
    return methods[method](params);
  };
}

export function forwardTo(invoke: Invoke, extension: string, page: string): BridgeHandler {
  return (method, params) =>
    invoke("ext_bridge", { extension, page, method, params: params ?? null });
}

export async function answer(
  handler: BridgeHandler,
  message: unknown,
): Promise<ReplyMessage | null> {
  if (!isRecord(message)) {
    return null;
  }
  const { id, method, params } = message;
  if ((typeof id !== "string" && typeof id !== "number") || typeof method !== "string") {
    return null;
  }
  try {
    return { id, result: (await handler(method, params)) ?? null };
  } catch (reason) {
    return { id, error: ExtensionError.from(reason).toWire() };
  }
}

export function serve(port: MessagePort, handler: BridgeHandler): void {
  port.onmessage = (event) => {
    void answer(handler, event.data).then((reply) => {
      if (reply !== null) {
        port.postMessage(reply);
      }
    });
  };
}

export function topicMessage(
  payload: unknown,
  extension: string,
  page: string,
): EventMessage<"backend.event"> | null {
  if (
    !isRecord(payload) ||
    payload.extension !== extension ||
    !Array.isArray(payload.pages) ||
    !payload.pages.includes(page) ||
    typeof payload.topic !== "string"
  ) {
    return null;
  }
  return { event: "backend.event", payload: { topic: payload.topic, payload: payload.payload } };
}

export function progressMessage(
  payload: unknown,
  extension: string,
): EventMessage<"backend.progress"> | null {
  if (!isRecord(payload) || payload.extension !== extension) {
    return null;
  }
  return {
    event: "backend.progress",
    payload: {
      message: typeof payload.message === "string" ? payload.message : null,
      fraction: typeof payload.fraction === "number" ? payload.fraction : null,
    },
  };
}

export function stateMessage(state: BackendState): EventMessage<"backend.state"> {
  return { event: "backend.state", payload: { state } };
}

export function appearanceOf(
  theme: string | undefined,
  prefersDark: boolean,
  darkThemes: ReadonlySet<string>,
): Appearance {
  const name = theme ?? "system";
  const dark = name === "system" ? prefersDark : darkThemes.has(name);
  return { theme: name, scheme: dark ? "dark" : "light" };
}

interface FrameAddress {
  extension: string;
  page: string;
}

export type InterfaceRequest = FrameAddress &
  (
    | { action: "open"; target: string; params: unknown }
    | { action: "badge"; badge: string | number | null }
    | { action: "title"; title: string }
    | { action: "toast"; kind: ToastKind; message: string }
    | {
        action: "confirm";
        request: number;
        title: string;
        body: string | null;
        confirmLabel: string | null;
        cancelLabel: string | null;
        destructive: boolean;
      }
  );

function text(value: unknown): string | null {
  return typeof value === "string" ? value : null;
}

export function interfaceRequest(payload: unknown): InterfaceRequest | null {
  if (
    !isRecord(payload) ||
    typeof payload.extension !== "string" ||
    typeof payload.page !== "string"
  ) {
    return null;
  }
  const address = { extension: payload.extension, page: payload.page };
  switch (payload.action) {
    case "open":
      return typeof payload.target === "string"
        ? { ...address, action: "open", target: payload.target, params: payload.params ?? null }
        : null;
    case "badge": {
      const badge = payload.badge;
      return typeof badge === "string" || typeof badge === "number" || badge === null
        ? { ...address, action: "badge", badge }
        : null;
    }
    case "title":
      return typeof payload.title === "string"
        ? { ...address, action: "title", title: payload.title }
        : null;
    case "toast": {
      const kind = TOAST_KINDS.find((candidate) => candidate === payload.kind) ?? "info";
      return typeof payload.message === "string"
        ? { ...address, action: "toast", kind, message: payload.message }
        : null;
    }
    case "confirm":
      return typeof payload.request === "number" && typeof payload.title === "string"
        ? {
            ...address,
            action: "confirm",
            request: payload.request,
            title: payload.title,
            body: text(payload.body),
            confirmLabel: text(payload.confirmLabel),
            cancelLabel: text(payload.cancelLabel),
            destructive: payload.destructive === true,
          }
        : null;
    default:
      return null;
  }
}
