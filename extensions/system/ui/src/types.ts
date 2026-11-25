export interface SystemSnapshot {
  hostName: string | null;
  kernel: string | null;
  distribution: string;
  uptimeSecs: number;
  cpu: {
    usagePercent: number;
    perCore: number[];
    physicalCores: number | null;
    logicalCores: number;
    brand: string;
    frequencyMhz: number;
  };
  memory: {
    totalBytes: number;
    usedBytes: number;
    availableBytes: number;
    swapTotalBytes: number;
    swapUsedBytes: number;
  };
  disks: {
    name: string;
    mountPoint: string;
    fileSystem: string;
    totalBytes: number;
    availableBytes: number;
    removable: boolean;
  }[];
  loadAverage: [number, number, number];
  temperatures: { label: string; celsius: number; criticalCelsius: number | null }[];
  battery: { percent: number; charging: boolean; timeToEmptySecs: number | null } | null;
}

export interface RunningProcess {
  pid: number;
  name: string;
  executable: string | null;
  cpuPercent: number;
  memoryBytes: number;
}

export interface VolumeState {
  volume: number;
  muted: boolean;
}

export interface AudioStream {
  id: number;
  application: string;
  mediaName: string | null;
  direction: "playback" | "capture";
  volume: number;
  muted: boolean;
}

export interface BluetoothAdapter {
  id: string;
  name: string;
  address: string;
  powered: boolean;
  discovering: boolean;
  discoverable: boolean;
}

export interface BluetoothDevice {
  address: string;
  name: string;
  paired: boolean;
  trusted: boolean;
  connected: boolean;
  batteryPercent: number | null;
  rssi: number | null;
  icon: string | null;
}

export interface BluetoothState {
  adapters: BluetoothAdapter[];
  devices: BluetoothDevice[];
  unavailable: string | null;
}

export type PortOwner = {
  pid: number;
  startedAt: string;
  uid: number;
  name: string;
  canStop: boolean;
};

export type ListeningPort = {
  address: string;
  port: number;
  protocol: "TCP" | "UDP";
  inode: string;
  owners: PortOwner[];
};

export type PortsSnapshot = { ports: ListeningPort[]; warnings: string[] };

export type DockerContainer = {
  id: string;
  name: string;
  image: string;
  state: string;
  status: string;
  ports: string;
};

export type DockerSnapshot = {
  status: "ready" | "missing" | "unavailable" | "remote";
  detail: string;
  endpoint: string | null;
  version: string | null;
  containers: DockerContainer[];
};
