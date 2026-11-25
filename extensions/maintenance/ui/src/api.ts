import { frameClient } from "@veronica/ui/frame";
import type {
  CleanerCategory,
  CleanerItem,
  CleanerScan,
  CleanReport,
  PackageInstalled,
  PackageSource,
  PackageUpdate,
  PackageUpdateResult,
  RemovalPlan,
} from "./types";

const LONG_CALL_MS = 600_000;

function call<T>(method: string, params: unknown = {}, timeoutMs?: number): Promise<T> {
  return frameClient().backend.call<T>(method, params, timeoutMs ? { timeoutMs } : {});
}

export const ipc = {
  packagesSources: () => call<PackageSource[]>("sources"),
  packagesUpdates: () => call<PackageUpdate[]>("updates", {}, LONG_CALL_MS),
  packagesInventory: () => call<PackageInstalled[]>("inventory", {}, LONG_CALL_MS),
  packagesUpdate: (source: string, names: string[]) =>
    call<PackageUpdateResult>("update", { source, names }, LONG_CALL_MS),
  packagesRemovalPlan: (packageName: string) =>
    call<RemovalPlan>("removalPlan", { package: packageName }),
  cleanerCategories: () => call<CleanerCategory[]>("cleanerCategories"),
  cleanerScan: (categories: string[]) =>
    call<CleanerScan>("cleanerScan", { categories }, LONG_CALL_MS),
  cleanerClean: (items: CleanerItem[]) =>
    call<CleanReport>("cleanerClean", { items }, LONG_CALL_MS),
};

export function errorText(reason: unknown): string {
  return reason instanceof Error ? reason.message : String(reason);
}
