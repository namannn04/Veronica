export interface CleanerCategory {
  id: string;
  title: string;
  cost: string;
  family: "cache" | "project";
  onByDefault: boolean;
  paths: string[];
  directoryNames: string[];
}

export interface CleanerItem {
  category: string;
  path: string;
  bytes: number;
}

export interface CleanerScan {
  items: CleanerItem[];
  totals: Record<string, number>;
  totalBytes: number;
}

export interface CleanReport {
  trashed: CleanerItem[];
  failed: [string, string][];
  bytesReclaimed: number;
}

export interface PackageSource {
  source: "apt" | "snap" | "flatpak";
  available: boolean;
  needsRoot: boolean;
}

export interface PackageUpdate {
  source: "apt" | "snap" | "flatpak";
  name: string;
  installedVersion: string | null;
  availableVersion: string;
  origin: string | null;
}

export interface PackageInstalled {
  source: "apt" | "snap" | "flatpak";
  name: string;
  version: string;
  sizeBytes: number | null;
}

export interface RemovalPlan {
  source: "apt" | "snap" | "flatpak";
  package: string;
  removed: string[];
  nowUnused: string[];
  command: string;
  removesMoreThanAsked: boolean;
}

export interface PackageUpdateResult {
  source: string;
  names: string[];
  command: string;
  succeeded: boolean;
  output: string;
}
