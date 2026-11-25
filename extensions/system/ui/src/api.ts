import { frameClient } from "@veronica/ui/frame";
import type {
  AudioStream,
  BluetoothState,
  DockerSnapshot,
  PortsSnapshot,
  RunningProcess,
  SystemSnapshot,
  VolumeState,
} from "./types";

function call<T>(method: string, params: unknown = {}): Promise<T> {
  return frameClient().backend.call<T>(method, params);
}

export const ipc = {
  systemSnapshot: () => call<SystemSnapshot>("snapshot"),
  systemProcesses: () => call<RunningProcess[]>("processes"),
  systemQuitProcess: (pid: number) => call<null>("quitProcess", { pid }),
  microphoneState: () => call<VolumeState>("microphone"),
  microphoneToggle: () => call<VolumeState>("microphoneToggle"),
  audioStreams: () => call<AudioStream[]>("audioStreams"),
  audioStreamVolume: (id: number, volume: number) => call<null>("streamVolume", { id, volume }),
  audioStreamToggleMute: (id: number) => call<boolean>("streamToggleMute", { id }),
  bluetoothState: () => call<BluetoothState>("bluetooth"),
  portsSnapshot: () => call<PortsSnapshot>("ports"),
  portsStop: (request: { pid: number; startedAt: string; inode: string }) =>
    call<string>("portsStop", request),
  dockerSnapshot: () => call<DockerSnapshot>("docker"),
  dockerAction: (endpoint: string, id: string, action: "start" | "stop" | "restart") =>
    call<null>("dockerAction", { endpoint, id, action }),
};

export function errorText(reason: unknown): string {
  return reason instanceof Error ? reason.message : String(reason);
}
