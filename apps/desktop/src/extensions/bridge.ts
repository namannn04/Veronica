export const HOST_API = 1;

const EXTENSION_ID = /^[a-z][a-z0-9-]{1,31}$/;
const ENTRY_SEGMENT = /^[A-Za-z0-9_-][A-Za-z0-9._-]*$/;

export type BridgeMethod = (params: unknown) => unknown;
export type BridgeMethods = Readonly<Record<string, BridgeMethod>>;

export type BridgeReply =
  | { id: string | number; result: unknown }
  | { id: string | number; error: { code: string; message: string } };

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
  return isRecord(data) && data.v === 1 && data.type === "hello" && data.apiVersion === HOST_API;
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

export async function answer(
  methods: BridgeMethods,
  message: unknown,
): Promise<BridgeReply | null> {
  if (!isRecord(message)) {
    return null;
  }
  const { id, method, params } = message;
  if ((typeof id !== "string" && typeof id !== "number") || typeof method !== "string") {
    return null;
  }
  if (!Object.hasOwn(methods, method)) {
    return { id, error: { code: "E_INVALID", message: `unknown method ${method}` } };
  }
  try {
    return { id, result: await methods[method](params) };
  } catch (reason) {
    const message = reason instanceof Error ? reason.message : String(reason);
    return { id, error: { code: "E_UNAVAILABLE", message } };
  }
}

export function serve(port: MessagePort, methods: BridgeMethods): void {
  port.onmessage = (event) => {
    void answer(methods, event.data).then((reply) => {
      if (reply !== null) {
        port.postMessage(reply);
      }
    });
  };
}
