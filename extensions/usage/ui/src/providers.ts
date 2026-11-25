export const LIMIT_PROVIDERS = ["Claude", "Codex"] as const;
export type LimitProvider = (typeof LIMIT_PROVIDERS)[number];

export function limitProviderOf(value: unknown): LimitProvider {
  return typeof value === "string" && value.toLowerCase() === "codex" ? "Codex" : "Claude";
}

export function storedLimitProvider(provider: LimitProvider) {
  return provider.toLowerCase();
}
