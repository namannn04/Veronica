import { ExtensionError } from "./errors";
import {
  type Appearance,
  type CallOptions,
  type ConfirmSpec,
  contractTopic,
  type FrameContext,
  HOST_API,
  type HostEvent,
  type HostEvents,
  type HostMethod,
  isEventMessage,
  isReady,
  isReply,
  type MethodParams,
  type MethodResult,
  type Presenter,
  type ToastKind,
  WIRE_VERSION,
} from "./protocol";

export const DEFAULT_TIMEOUT_MS = 15_000;
export const BACKEND_TIMEOUT_MS = 60_000;
export const CONFIRM_TIMEOUT_MS = 300_000;
export const HANDSHAKE_TIMEOUT_MS = 10_000;
const HOST_GRACE_MS = 5_000;

export type Unsubscribe = () => void;
type Handler<T> = (payload: T) => void;

interface Pending {
  resolve: (value: unknown) => void;
  reject: (reason: ExtensionError) => void;
  timer: ReturnType<typeof setTimeout>;
}

export interface Port {
  postMessage(message: unknown): void;
  onmessage: ((event: MessageEvent) => void) | null;
  close(): void;
}

export interface ClientOptions {
  timeoutMs?: number;
}

function timeoutFor(method: HostMethod, params: unknown, fallback: number): number {
  if (method === "ui.confirm") {
    return CONFIRM_TIMEOUT_MS + HOST_GRACE_MS;
  }
  if (method === "backend.call" || method === "contracts.request") {
    const requested = (params as CallOptions).timeoutMs;
    return (requested ?? BACKEND_TIMEOUT_MS) + HOST_GRACE_MS;
  }
  return fallback;
}

export class Client {
  readonly context: FrameContext;
  private readonly port: Port;
  private readonly timeoutMs: number;
  private readonly pending = new Map<number, Pending>();
  private readonly listeners = new Map<string, Set<Handler<unknown>>>();
  private readonly topics = new Map<string, Set<Handler<unknown>>>();
  private nextId = 1;
  private closed = false;

  constructor(port: Port, context: FrameContext, options: ClientOptions = {}) {
    this.port = port;
    this.context = { ...context };
    this.timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
    port.onmessage = (event) => this.receive(event.data);
  }

  request<M extends HostMethod>(method: M, params: MethodParams<M>): Promise<MethodResult<M>> {
    if (this.closed) {
      return Promise.reject(new ExtensionError("E_UNAVAILABLE", "the host connection is closed"));
    }
    const id = this.nextId++;
    const limit = timeoutFor(method, params, this.timeoutMs);
    return new Promise<MethodResult<M>>((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new ExtensionError("E_TIMEOUT", `${method} did not answer within ${limit} ms`));
      }, limit);
      this.pending.set(id, { resolve: resolve as (value: unknown) => void, reject, timer });
      this.port.postMessage({ id, method, params });
    });
  }

  on<E extends HostEvent>(event: E, handler: Handler<HostEvents[E]>): Unsubscribe {
    return this.listen(this.listeners, event, handler as Handler<unknown>);
  }

  close(): void {
    if (this.closed) {
      return;
    }
    this.closed = true;
    for (const [id, pending] of this.pending) {
      clearTimeout(pending.timer);
      pending.reject(new ExtensionError("E_UNAVAILABLE", "the host connection is closed"));
      this.pending.delete(id);
    }
    this.port.onmessage = null;
    this.port.close();
  }

  readonly backend = {
    call: <T = unknown>(method: string, params?: unknown, options: CallOptions = {}) =>
      this.request("backend.call", { method, params, ...options }) as Promise<T>,
    subscribe: <T = unknown>(topic: string, handler: Handler<T>) =>
      this.subscribeTopic(
        topic,
        handler as Handler<unknown>,
        () => this.request("backend.subscribe", { topic }),
        () => this.request("backend.unsubscribe", { topic }),
      ),
  };

  readonly storage = {
    get: <T = unknown>(key: string) => this.request("storage.get", { key }) as Promise<T | null>,
    set: (key: string, value: unknown) => this.request("storage.set", { key, value }),
    remove: (key: string) => this.request("storage.remove", { key }),
    keys: () => this.request("storage.keys", {}),
  };

  readonly settings = {
    get: <T = unknown>(key: string) => this.request("settings.get", { key }) as Promise<T | null>,
    set: (key: string, value: unknown) => this.request("settings.set", { key, value }),
  };

  readonly contracts = {
    request: <T = unknown>(id: string, params?: unknown, options: CallOptions = {}) =>
      this.request("contracts.request", { id, params, ...options }) as Promise<T>,
    subscribe: <T = unknown>(id: string, handler: Handler<T>) =>
      this.subscribeTopic(contractTopic(id), handler as Handler<unknown>, () =>
        this.request("contracts.subscribe", { id }),
      ),
  };

  readonly nav = {
    open: (page: string, params?: unknown) => this.request("nav.open", { page, params }),
    setBadge: (badge: string | number | null) => this.request("nav.setBadge", { badge }),
    setTitle: (title: string) => this.request("nav.setTitle", { title }),
  };

  readonly ui = {
    toast: (message: string, kind: ToastKind = "info") =>
      this.request("ui.toast", { message, kind }),
    confirm: (spec: ConfirmSpec) => this.request("ui.confirm", spec),
    openExternal: (url: string) => this.request("ui.openExternal", { url }),
  };

  readonly clipboard = {
    writeText: (text: string) => this.request("clipboard.writeText", { text }),
  };

  readonly files = {
    url: (path: string) => this.request("files.url", { path }),
  };

  private listen(
    registry: Map<string, Set<Handler<unknown>>>,
    key: string,
    handler: Handler<unknown>,
  ): Unsubscribe {
    let handlers = registry.get(key);
    if (handlers === undefined) {
      handlers = new Set();
      registry.set(key, handlers);
    }
    handlers.add(handler);
    return () => {
      handlers.delete(handler);
      if (handlers.size === 0) {
        registry.delete(key);
      }
    };
  }

  private async subscribeTopic(
    topic: string,
    handler: Handler<unknown>,
    register: () => Promise<unknown>,
    release?: () => Promise<unknown>,
  ): Promise<Unsubscribe> {
    const first = !this.topics.has(topic);
    const stop = this.listen(this.topics, topic, handler);
    if (first) {
      try {
        await register();
      } catch (reason) {
        stop();
        throw reason;
      }
    }
    return () => {
      stop();
      if (release !== undefined && !this.topics.has(topic) && !this.closed) {
        void release().catch(() => undefined);
      }
    };
  }

  private emit(registry: Map<string, Set<Handler<unknown>>>, key: string, payload: unknown) {
    for (const handler of [...(registry.get(key) ?? [])]) {
      try {
        handler(payload);
      } catch (reason) {
        console.error(reason);
      }
    }
  }

  private receive(data: unknown): void {
    if (isReply(data)) {
      const pending = typeof data.id === "number" ? this.pending.get(data.id) : undefined;
      if (pending === undefined || typeof data.id !== "number") {
        return;
      }
      this.pending.delete(data.id);
      clearTimeout(pending.timer);
      if ("error" in data) {
        pending.reject(ExtensionError.from(data.error));
      } else {
        pending.resolve(data.result);
      }
      return;
    }
    if (!isEventMessage(data)) {
      return;
    }
    if (data.event === "appearance.changed") {
      this.context.appearance = data.payload as Appearance;
    } else if (data.event === "presenter.changed") {
      this.context.presenter = data.payload as Presenter;
    } else if (data.event === "backend.event") {
      const { topic, payload } = data.payload as HostEvents["backend.event"];
      this.emit(this.topics, topic, payload);
    }
    this.emit(this.listeners, data.event, data.payload);
  }
}

export interface FrameEvent {
  source: unknown;
  data: unknown;
  ports: readonly MessagePort[];
}

export interface FrameWindow {
  parent: { postMessage(message: unknown, targetOrigin: string): void };
  addEventListener(type: "message", listener: (event: FrameEvent) => void): void;
  removeEventListener(type: "message", listener: (event: FrameEvent) => void): void;
}

export interface ConnectOptions extends ClientOptions {
  handshakeTimeoutMs?: number;
  window?: FrameWindow;
}

function frameWindow(): FrameWindow {
  return window as unknown as FrameWindow;
}

export function connect(options: ConnectOptions = {}): Promise<Client> {
  const scope = options.window ?? frameWindow();
  const limit = options.handshakeTimeoutMs ?? HANDSHAKE_TIMEOUT_MS;
  return new Promise<Client>((resolve, reject) => {
    const finish = () => {
      clearTimeout(timer);
      scope.removeEventListener("message", onMessage);
    };
    const onMessage = (event: FrameEvent) => {
      const port = event.ports[0];
      if (event.source !== scope.parent || !isReady(event.data) || port === undefined) {
        return;
      }
      finish();
      const context: FrameContext = {
        extension: event.data.extension,
        page: event.data.page,
        params: event.data.params,
        hostApi: event.data.hostApi,
        granted: event.data.granted,
        appearance: event.data.appearance,
        presenter: event.data.presenter,
        locale: event.data.locale,
      };
      if (context.hostApi < HOST_API) {
        port.close();
        reject(new ExtensionError("E_VERSION", `the host provides API ${context.hostApi}`));
        return;
      }
      resolve(new Client(port, context, options));
    };
    const timer = setTimeout(() => {
      scope.removeEventListener("message", onMessage);
      reject(new ExtensionError("E_TIMEOUT", `the host did not answer within ${limit} ms`));
    }, limit);
    scope.addEventListener("message", onMessage);
    scope.parent.postMessage({ v: WIRE_VERSION, type: "hello", apiVersion: HOST_API }, "*");
  });
}
