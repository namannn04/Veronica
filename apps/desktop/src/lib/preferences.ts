export const APPEARANCES = [
  { id: "system", label: "System", detail: "Follows Ubuntu" },
  { id: "light", label: "Ivory", detail: "Warm and bright" },
  { id: "dark", label: "Graphite", detail: "Edith classic" },
  { id: "midnight", label: "Midnight", detail: "Deep navy" },
  { id: "aubergine", label: "Aubergine", detail: "Ubuntu inspired" },
  { id: "forest", label: "Forest", detail: "Calm green" },
] as const;

export type Appearance = (typeof APPEARANCES)[number]["id"];

const APPEARANCE_IDS = new Set<string>(APPEARANCES.map((theme) => theme.id));

export function appearanceOf(value: unknown): Appearance {
  return typeof value === "string" && APPEARANCE_IDS.has(value)
    ? value as Appearance
    : "system";
}

export function applyAppearance(value: unknown) {
  const appearance = appearanceOf(value);
  if (appearance === "system") delete document.documentElement.dataset.theme;
  else document.documentElement.dataset.theme = appearance;
}

export const LIMIT_PROVIDERS = ["Claude", "Codex"] as const;
export type LimitProvider = (typeof LIMIT_PROVIDERS)[number];

export function limitProviderOf(value: unknown): LimitProvider {
  return typeof value === "string" && value.toLowerCase() === "codex"
    ? "Codex"
    : "Claude";
}

export function storedLimitProvider(provider: LimitProvider) {
  return provider.toLowerCase();
}

/**
 * Categories presenter mode can blur, matching `BlurCategory` in the core.
 *
 * The document attribute names are derived from the class names so the two
 * cannot drift: `blur-money` is switched by `data-blur-money`.
 */
export const BLUR_CATEGORIES = [
  { id: "money", css: "blur-money", label: "Spend" },
  { id: "usage", css: "blur-usage", label: "Tokens and charts" },
  { id: "agents", css: "blur-agents", label: "Rate limits" },
  { id: "calendar", css: "blur-calendar", label: "Calendar entries" },
  { id: "music", css: "blur-music", label: "Track names" },
] as const;

export type BlurCategoryId = (typeof BLUR_CATEGORIES)[number]["id"];

/** The settings key for one category, as Edith names it. */
export function blurKey(id: BlurCategoryId): string {
  return `presenterBlur${id.charAt(0).toUpperCase()}${id.slice(1)}`;
}

/**
 * Apply the resolved presenter state to the document.
 *
 * `active` is the gate the core computed; `blurredClasses` are the categories
 * still switched on. Both are written as attributes so the stylesheet holds the
 * decisions rather than the components.
 */
export function applyPresenter(active: boolean, blurredClasses: string[]) {
  const root = document.documentElement;
  root.dataset.presenter = active ? "true" : "false";
  for (const category of BLUR_CATEGORIES) {
    const blurred = active && blurredClasses.includes(category.css);
    root.dataset[`blur${category.id.charAt(0).toUpperCase()}${category.id.slice(1)}`] =
      blurred ? "true" : "false";
  }
}
