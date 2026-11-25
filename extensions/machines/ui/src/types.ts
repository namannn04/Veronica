export type MachineReach = { kind: "local" } | { kind: "ssh"; target: string; port?: number };

export interface Machine {
  id: string;
  name: string;
  reach: MachineReach;
  mac?: string | null;
}
export interface MachineFile {
  name: string;
  path: string;
  kind: "directory" | "file" | "link" | "other";
  sizeBytes: number;
  modifiedUnix: number;
}
export interface MachineDirectory {
  path: string;
  parent: string | null;
  entries: MachineFile[];
}
export interface ContainerInfo {
  id: string;
  name: string;
  image: string;
  status: string;
  state: string;
  engine: "docker" | "podman";
  ports: string;
}

export interface MachineDisk {
  mountPoint: string;
  totalBytes: number;
  availableBytes: number;
}

export interface MachineStats {
  hostName: string;
  kernel: string;
  os: string;
  uptimeSecs: number;
  loadAverage: [number, number, number];
  cpuPercent: number;
  memoryTotalBytes: number;
  memoryAvailableBytes: number;
  swapTotalBytes: number;
  swapFreeBytes: number;
  disks: MachineDisk[];
  temperatures: { label: string; celsius: number }[];
  fans: { label: string; rpm: number }[];
  gpus: MachineGpu[];
  processes: { pid: number; name: string; cpuPercent: number; memoryBytes: number }[];
}

export interface MachineGpu {
  name: string;
  utilizationPercent: number;
  memoryUsedBytes: number;
  memoryTotalBytes: number;
  temperatureCelsius: number | null;
}

export interface MachineReport {
  machine: Machine;
  stats: MachineStats | null;
  error: string | null;
}
