import { frameClient } from "@veronica/ui/frame";
import type { ContainerInfo, Machine, MachineDirectory, MachineReport } from "./types";

const LONG_CALL_MS = 120_000;

function call<T>(method: string, params: unknown = {}, timeoutMs?: number): Promise<T> {
  return frameClient().backend.call<T>(method, params, timeoutMs ? { timeoutMs } : {});
}

export const ipc = {
  machinesProbe: () => call<MachineReport[]>("probe", {}, LONG_CALL_MS),
  machinesAdd: (target: string, name: string | null, port: number | null) =>
    call<Machine>("add", { target, name, port }),
  machinesRemove: (id: string) => call<null>("remove", { id }),
  machinesDiscover: () => call<string[]>("discover"),
  machinesTerminal: (id: string) => call<null>("terminal", { id }),
  machinesFiles: (id: string, path: string | null = null) =>
    call<MachineDirectory>("files", { id, path }, LONG_CALL_MS),
  machinesOpenFile: (id: string, path: string) =>
    call<string>("openFile", { id, path }, LONG_CALL_MS),
  machinesContainers: (id: string) => call<ContainerInfo[]>("containers", { id }, LONG_CALL_MS),
  machinesContainerAction: (id: string, engine: string, container: string, action: string) =>
    call<null>("containerAction", { id, engine, container, action }, LONG_CALL_MS),
  machinesContainerLogs: (id: string, engine: string, container: string, lines = 200) =>
    call<string>("containerLogs", { id, engine, container, lines }, LONG_CALL_MS),
  machinesPower: (id: string, action: "restart" | "shutdown") =>
    call<null>("power", { id, action }, LONG_CALL_MS),
  machinesWake: (id: string) => call<null>("wake", { id }),
};

export function errorText(reason: unknown): string {
  return reason instanceof Error ? reason.message : String(reason);
}
