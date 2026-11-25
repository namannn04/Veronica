import { frameClient } from "@veronica/ui/frame";
import type { ClipRow } from "./types";

export const ipc = {
  clipboardList: (query: string) => frameClient().backend.call<ClipRow[]>("list", { query }),
  clipboardRemove: (id: number) => frameClient().backend.call<null>("remove", { id }),
  clipboardClear: () => frameClient().backend.call<null>("clear", {}),
  copy: (text: string) => frameClient().clipboard.writeText(text),
};

export function errorText(reason: unknown): string {
  return reason instanceof Error ? reason.message : String(reason);
}
