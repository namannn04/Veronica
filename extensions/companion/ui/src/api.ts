import { frameClient } from "@veronica/ui/frame";
import type { ChatMessage, ChatView, ModelConfig } from "./companionChat";
import type { AttentionOverview, CompanionItem, CompanionRecordingStatus } from "./types";

const MODEL_CALL_MS = 600_000;

function call<T>(method: string, params: unknown = {}, timeoutMs?: number): Promise<T> {
  return frameClient().backend.call<T>(method, params, timeoutMs ? { timeoutMs } : {});
}

export const ipc = {
  chatPresence: () => call<null>("chatPresence"),
  chatView: () => call<ChatView>("chatView"),
  chatConfigure: (config: ModelConfig, memory: string) =>
    call<null>("chatConfigure", { config, memory }),
  chatEnable: (enabled: boolean) => call<ChatView>("chatEnable", { enabled }, MODEL_CALL_MS),
  chatSend: (text: string, key: string, includeHistory: boolean, includeMemory: boolean) =>
    call<ChatMessage>("chatSend", { text, key, includeHistory, includeMemory }, MODEL_CALL_MS),
  chatPreview: (text: string, includeHistory: boolean, includeMemory: boolean) =>
    call<unknown>("chatPreview", { text, includeHistory, includeMemory }),
  chatModels: (config: ModelConfig) => call<string[]>("chatModels", { config }, MODEL_CALL_MS),
  chatPull: (config: ModelConfig, confirmed: boolean) =>
    call<null>("chatPull", { config, confirmed }, MODEL_CALL_MS),
  chatClear: (memory: boolean) => call<null>("chatClear", { memory }),
  chatExport: () => call<string>("chatExport"),
  chatJournal: (text: string) => call<ChatMessage>("chatJournal", { text }),
  companionList: (query = "") => call<CompanionItem[]>("list", { query }),
  companionNoteCreate: (title: string, body: string) =>
    call<CompanionItem>("createNote", { title, body }),
  companionUpdate: (id: number, title: string, body: string, pinned: boolean) =>
    call<CompanionItem>("update", { id, title, body, pinned }),
  companionRemove: (id: number) => call<null>("remove", { id }),
  companionRecordingStatus: () => call<CompanionRecordingStatus>("recordingStatus"),
  companionRecordStart: () => call<null>("recordStart"),
  companionRecordStop: () => call<CompanionItem>("recordStop"),
  companionRecordCancel: () => call<null>("recordCancel"),
  companionAudio: (id: number) => call<string>("audioPath", { id }),
  attentionOverview: (days: number) => call<AttentionOverview>("activity", { days }),
};

export async function playableAudio(path: string): Promise<string> {
  const response = await fetch(await frameClient().files.url(path));
  if (!response.ok) {
    throw new Error(`could not read ${path}`);
  }
  return URL.createObjectURL(await response.blob());
}

export function errorText(reason: unknown): string {
  return reason instanceof Error ? reason.message : String(reason);
}
