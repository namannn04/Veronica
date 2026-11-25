export const SURFACE_TEMPLATES = [
  "ring",
  "meter",
  "stat",
  "text",
  "list",
  "nowPlaying",
  "actions",
] as const;

export type SurfaceTemplate = (typeof SURFACE_TEMPLATES)[number];

export interface SurfaceAction {
  id: string;
  label?: string;
  run: string[];
}

export interface Surface {
  extension: string;
  id: string;
  placement: "home" | "notch";
  template: SurfaceTemplate;
  title?: string;
  opens?: string;
  priority: number;
  blurCategory?: string;
  actions: SurfaceAction[];
  state: Record<string, unknown>;
}

export type CardBody =
  | { kind: "gauge"; label: string; fraction: number; caption: string }
  | { kind: "stat"; label: string; value: string; caption: string | null }
  | { kind: "text"; text: string }
  | { kind: "list"; items: { title: string; detail: string | null }[] }
  | { kind: "nowPlaying"; title: string; detail: string; playing: boolean }
  | { kind: "actions"; caption: string | null };

export interface HomeCardView {
  key: string;
  extension: string;
  surface: string;
  title: string;
  opens: string | null;
  blurClass: string | null;
  body: CardBody;
  actions: { id: string; label: string }[];
}

const BLUR_CLASSES: Record<string, string> = {
  money: "blur-money",
  usage: "blur-usage",
  agents: "blur-agents",
  calendar: "blur-calendar",
  music: "blur-music",
};

function text(value: unknown): string {
  return typeof value === "string" ? value : "";
}

function fraction(value: unknown): number {
  const number = Number(value);
  return Number.isFinite(number) ? Math.max(0, Math.min(1, number)) : 0;
}

function isSurface(value: unknown): value is Surface {
  if (typeof value !== "object" || value === null) {
    return false;
  }
  const candidate = value as Record<string, unknown>;
  return (
    typeof candidate.extension === "string" &&
    typeof candidate.id === "string" &&
    (candidate.placement === "home" || candidate.placement === "notch") &&
    SURFACE_TEMPLATES.includes(candidate.template as SurfaceTemplate) &&
    typeof candidate.state === "object" &&
    candidate.state !== null
  );
}

export function parseSurfaces(value: unknown): Surface[] {
  return Array.isArray(value) ? value.filter(isSurface) : [];
}

function body(surface: Surface): CardBody {
  const state = surface.state;
  switch (surface.template) {
    case "ring":
    case "meter": {
      const value = fraction(state.value);
      return {
        kind: "gauge",
        label: text(state.label),
        fraction: value,
        caption: text(state.caption) || `${Math.round(value * 100)}%`,
      };
    }
    case "stat":
      return {
        kind: "stat",
        label: text(state.label),
        value: typeof state.value === "number" ? String(state.value) : text(state.value),
        caption: text(state.caption) || null,
      };
    case "text":
      return { kind: "text", text: text(state.text) };
    case "list":
      return {
        kind: "list",
        items: (Array.isArray(state.items) ? state.items : []).map((item) => {
          const entry = (item ?? {}) as Record<string, unknown>;
          return { title: text(entry.title), detail: text(entry.detail) || null };
        }),
      };
    case "nowPlaying":
      return {
        kind: "nowPlaying",
        title: text(state.title),
        detail: [text(state.artist), text(state.album)].filter(Boolean).join(" · "),
        playing: state.playing === true,
      };
    default:
      return { kind: "actions", caption: text(state.caption) || null };
  }
}

export function homeCards(surfaces: readonly Surface[]): HomeCardView[] {
  const notchBlur = new Map(
    surfaces
      .filter((surface) => surface.placement === "notch" && surface.blurCategory)
      .map((surface) => [`${surface.extension}/${surface.id}`, surface.blurCategory ?? ""]),
  );
  return surfaces
    .filter((surface) => surface.placement === "home")
    .sort((a, b) => a.extension.localeCompare(b.extension) || a.id.localeCompare(b.id))
    .map((surface) => {
      const key = `${surface.extension}/${surface.id}`;
      const category = surface.blurCategory ?? notchBlur.get(key);
      return {
        key,
        extension: surface.extension,
        surface: surface.id,
        title: surface.title ?? surface.id,
        opens: surface.opens ?? null,
        blurClass: category ? (BLUR_CLASSES[category] ?? null) : null,
        body: body(surface),
        actions: surface.actions
          .filter((action) => action.run.length > 0)
          .map((action) => ({ id: action.id, label: action.label ?? action.id })),
      };
    });
}
