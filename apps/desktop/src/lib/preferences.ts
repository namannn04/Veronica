export const APPEARANCES = [
  {
    id: "system",
    label: "System",
    detail: "Follows Ubuntu",
    family: "system",
    swatch: ["#f4f2ee", "#ffffff", "#c25f3c"],
    swatchDark: ["#100f13", "#1e1d23", "#d97757"],
  },
  {
    id: "light",
    label: "Ivory",
    detail: "Warm and bright",
    family: "light",
    swatch: ["#f4f2ee", "#ffffff", "#c25f3c"],
  },
  {
    id: "sandstone",
    label: "Sandstone",
    detail: "Warm paper and clay",
    family: "light",
    swatch: ["#efe7db", "#fffaf3", "#b1552e"],
  },
  {
    id: "mist",
    label: "Mist",
    detail: "Cool and quiet",
    family: "light",
    swatch: ["#eaeef4", "#ffffff", "#2f6fd0"],
  },
  {
    id: "paper",
    label: "Paper",
    detail: "High contrast",
    family: "light",
    swatch: ["#f2f2f2", "#ffffff", "#9a3412"],
  },
  {
    id: "dark",
    label: "Graphite",
    detail: "Edith classic",
    family: "dark",
    swatch: ["#100f13", "#1e1d23", "#d97757"],
  },
  {
    id: "midnight",
    label: "Midnight",
    detail: "Deep navy",
    family: "dark",
    swatch: ["#07101f", "#111d2f", "#78a8f0"],
  },
  {
    id: "aubergine",
    label: "Aubergine",
    detail: "Ubuntu inspired",
    family: "dark",
    swatch: ["#1a0f18", "#301d2b", "#e18abb"],
  },
  {
    id: "forest",
    label: "Forest",
    detail: "Calm green",
    family: "dark",
    swatch: ["#091410", "#14251f", "#79b894"],
  },
  {
    id: "nord",
    label: "Nord",
    detail: "Arctic slate",
    family: "dark",
    swatch: ["#22272f", "#3b4252", "#88c0d0"],
  },
  {
    id: "ocean",
    label: "Ocean",
    detail: "Deep teal",
    family: "dark",
    swatch: ["#05161c", "#0d3340", "#45c2d6"],
  },
  {
    id: "ember",
    label: "Ember",
    detail: "Warm charcoal",
    family: "dark",
    swatch: ["#17110c", "#2c211a", "#e8974a"],
  },
  {
    id: "carbon",
    label: "Carbon",
    detail: "True black, for OLED",
    family: "dark",
    swatch: ["#000000", "#131313", "#ff8a5c"],
  },
] as const;

export type Appearance = (typeof APPEARANCES)[number]["id"];
export type AppearanceFamily = (typeof APPEARANCES)[number]["family"];

export const APPEARANCE_FAMILIES: { id: AppearanceFamily; label: string }[] = [
  { id: "system", label: "Automatic" },
  { id: "light", label: "Light" },
  { id: "dark", label: "Dark" },
];

const APPEARANCE_IDS = new Set<string>(APPEARANCES.map((theme) => theme.id));

export function appearanceOf(value: unknown): Appearance {
  return typeof value === "string" && APPEARANCE_IDS.has(value) ? (value as Appearance) : "system";
}

let appearanceApplied = false;

function setThemeAttribute(appearance: Appearance) {
  if (appearance === "system") {
    delete document.documentElement.dataset.theme;
  } else {
    document.documentElement.dataset.theme = appearance;
  }
}

export function applyAppearance(value: unknown) {
  const appearance = appearanceOf(value);
  const current = document.documentElement.dataset.theme ?? "system";
  const firstApply = !appearanceApplied;
  appearanceApplied = true;
  if (current === appearance) {
    return;
  }
  const calm = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  if (firstApply || calm || typeof document.startViewTransition !== "function") {
    setThemeAttribute(appearance);
    return;
  }
  document.startViewTransition(() => setThemeAttribute(appearance));
}

export const BLUR_CATEGORIES = [
  { id: "money", css: "blur-money", label: "Spend" },
  { id: "usage", css: "blur-usage", label: "Tokens and charts" },
  { id: "agents", css: "blur-agents", label: "Rate limits" },
  { id: "calendar", css: "blur-calendar", label: "Calendar entries" },
  { id: "music", css: "blur-music", label: "Track names" },
] as const;

export type BlurCategoryId = (typeof BLUR_CATEGORIES)[number]["id"];

export function blurKey(id: BlurCategoryId): string {
  return `presenterBlur${id.charAt(0).toUpperCase()}${id.slice(1)}`;
}

export function applyPresenter(active: boolean, blurredClasses: string[]) {
  const root = document.documentElement;
  root.dataset.presenter = active ? "true" : "false";
  for (const category of BLUR_CATEGORIES) {
    const blurred = active && blurredClasses.includes(category.css);
    root.dataset[`blur${category.id.charAt(0).toUpperCase()}${category.id.slice(1)}`] = blurred
      ? "true"
      : "false";
  }
}
