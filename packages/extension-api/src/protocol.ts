import { isErrorCode, type WireError } from "./errors";

export const HOST_API = 1;
export const WIRE_VERSION = 1;

export type HostPermission =
  | "notifications"
  | "surfaces"
  | "contracts"
  | "settings:read"
  | "secrets"
  | "openExternal"
  | "clipboard:write"
  | "shortcuts";

export interface FilesystemGrant {
  path: string;
  access: string;
}

export interface Granted {
  host: HostPermission[];
  filesystem: FilesystemGrant[];
  network: string[];
}

export interface Appearance {
  theme: string;
  scheme: "light" | "dark";
}

export interface Presenter {
  active: boolean;
}

export type BackendState = "running" | "stopped" | "crashLooping";

export type ToastKind = "info" | "success" | "warning" | "error";

export interface ConfirmSpec {
  title: string;
  body?: string;
  confirmLabel?: string;
  cancelLabel?: string;
  destructive?: boolean;
}

export interface Hello {
  v: typeof WIRE_VERSION;
  type: "hello";
  apiVersion: typeof HOST_API;
}

export interface FrameContext {
  extension: string;
  page: string;
  params: unknown;
  hostApi: number;
  granted: Granted;
  appearance: Appearance;
  presenter: Presenter;
  locale: string;
}

export interface Ready extends FrameContext {
  v: typeof WIRE_VERSION;
  type: "ready";
}

export interface CallOptions {
  timeoutMs?: number;
}

export interface HostMethods {
  "backend.call": {
    params: { method: string; params?: unknown; timeoutMs?: number };
    result: unknown;
  };
  "backend.subscribe": { params: { topic: string }; result: null };
  "backend.unsubscribe": { params: { topic: string }; result: null };
  "storage.get": { params: { key: string }; result: unknown };
  "storage.set": { params: { key: string; value: unknown }; result: null };
  "storage.remove": { params: { key: string }; result: boolean };
  "storage.keys": { params: Record<string, never>; result: string[] };
  "settings.get": { params: { key: string }; result: unknown };
  "settings.set": { params: { key: string; value: unknown }; result: unknown };
  "contracts.request": {
    params: { id: string; params?: unknown; timeoutMs?: number };
    result: unknown;
  };
  "contracts.subscribe": { params: { id: string }; result: null };
  "nav.open": { params: { page: string; params?: unknown }; result: null };
  "nav.setBadge": { params: { badge: string | number | null }; result: null };
  "nav.setTitle": { params: { title: string }; result: null };
  "ui.toast": { params: { message: string; kind?: ToastKind }; result: null };
  "ui.confirm": { params: ConfirmSpec; result: boolean };
  "ui.openExternal": { params: { url: string }; result: null };
  "clipboard.writeText": { params: { text: string }; result: null };
  "files.url": { params: { path: string }; result: string };
}

export type HostMethod = keyof HostMethods;
export type MethodParams<M extends HostMethod> = HostMethods[M]["params"];
export type MethodResult<M extends HostMethod> = HostMethods[M]["result"];

export const HOST_METHODS: readonly HostMethod[] = [
  "backend.call",
  "backend.subscribe",
  "backend.unsubscribe",
  "storage.get",
  "storage.set",
  "storage.remove",
  "storage.keys",
  "settings.get",
  "settings.set",
  "contracts.request",
  "contracts.subscribe",
  "nav.open",
  "nav.setBadge",
  "nav.setTitle",
  "ui.toast",
  "ui.confirm",
  "ui.openExternal",
  "clipboard.writeText",
  "files.url",
];

export interface TopicEvent {
  topic: string;
  payload: unknown;
}

export interface ProgressEvent {
  message: string | null;
  fraction: number | null;
}

export interface HostEvents {
  "appearance.changed": Appearance;
  "presenter.changed": Presenter;
  "visibility.changed": { visible: boolean };
  "backend.state": { state: BackendState };
  "backend.event": TopicEvent;
  "backend.progress": ProgressEvent;
}

export type HostEvent = keyof HostEvents;

export interface RequestMessage {
  id: number;
  method: string;
  params: unknown;
}

export type ReplyMessage =
  | { id: string | number; result: unknown }
  | { id: string | number; error: WireError };

export interface EventMessage<E extends HostEvent = HostEvent> {
  event: E;
  payload: HostEvents[E];
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

export function contractTopic(id: string): string {
  return `contract:${id}`;
}

export function isReady(data: unknown): data is Ready {
  return (
    isRecord(data) &&
    data.v === WIRE_VERSION &&
    data.type === "ready" &&
    typeof data.extension === "string" &&
    typeof data.page === "string" &&
    typeof data.hostApi === "number"
  );
}

export function isReply(data: unknown): data is ReplyMessage {
  if (!isRecord(data) || (typeof data.id !== "number" && typeof data.id !== "string")) {
    return false;
  }
  if ("result" in data) {
    return true;
  }
  return isRecord(data.error) && isErrorCode(data.error.code);
}

export function isEventMessage(data: unknown): data is EventMessage {
  return isRecord(data) && typeof data.event === "string" && "payload" in data;
}
