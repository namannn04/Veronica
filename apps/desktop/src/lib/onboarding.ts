import { type Invoke, isActive } from "./library";
import type { CatalogShelf, ExtensionJob, ExtensionReport, InstalledExtension } from "./types";

export type Profile = "simple" | "developer";

export const PROFILE_KEY = "profile";
export const STARTERS_KEY = "starterPack";
export const PROFILES: readonly Profile[] = ["simple", "developer"];

export interface Detection {
  suggested: Profile;
  editors: string[];
  agents: string[];
}

export type StarterState = "pending" | "installing" | "installed" | "failed";

export interface StarterStatus {
  state: StarterState;
  message: string | null;
  error: string | null;
}

export function chosenProfile(settings: Record<string, unknown>): Profile | null {
  const value = settings[PROFILE_KEY];
  return PROFILES.find((profile) => profile === value) ?? null;
}

export function needsOnboarding(
  settings: Record<string, unknown>,
  features: readonly ExtensionReport[],
): boolean {
  if (chosenProfile(settings) !== null) {
    return false;
  }
  return !features.some((feature) => Object.hasOwn(settings, feature.defaultsKey));
}

export function storedStarters(settings: Record<string, unknown>): string[] {
  const value = settings[STARTERS_KEY];
  return Array.isArray(value) ? value.filter((id): id is string => typeof id === "string") : [];
}

export function missingStarters(
  shelf: CatalogShelf,
  profile: Profile,
  installed: readonly InstalledExtension[],
): string[] {
  const members = shelf.profiles.find((item) => item.id === profile)?.extensions ?? [];
  const present = new Set(installed.map((row) => row.id));
  const installable = new Set(
    shelf.extensions.filter((offer) => offer.available !== null).map((offer) => offer.id),
  );
  return members.filter((id) => installable.has(id) && !present.has(id));
}

export function starterStatus(
  id: string,
  installed: readonly InstalledExtension[],
  job: ExtensionJob | undefined,
): StarterStatus {
  if (isActive(job)) {
    return { state: "installing", message: job?.message ?? null, error: null };
  }
  if (installed.some((row) => row.id === id)) {
    return { state: "installed", message: null, error: null };
  }
  if (job?.phase === "failed") {
    return { state: "failed", message: null, error: job.error };
  }
  return { state: "pending", message: null, error: null };
}

export async function chooseProfile(
  invoke: Invoke,
  profile: Profile,
  previous: readonly string[],
): Promise<string[]> {
  await invoke("settings_set", { key: PROFILE_KEY, value: profile });
  const [shelf, installed] = await Promise.all([
    invoke<CatalogShelf>("ext_catalog_search", { query: "", category: null }),
    invoke<InstalledExtension[]>("ext_list"),
  ]);
  const missing = missingStarters(shelf, profile, installed);
  const starters = [...new Set([...previous, ...missing])];
  await invoke("settings_set", { key: STARTERS_KEY, value: starters });
  for (const id of missing) {
    await invoke("ext_install", { id }).catch(() => undefined);
  }
  return missing;
}
