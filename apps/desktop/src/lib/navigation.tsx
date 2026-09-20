/* Where Veronica can go, in one list.
 *
 * The sidebar and the command palette read the same entries, so a page can
 * never be reachable from one and missing from the other. `keywords` exist for
 * the palette alone: they are the words someone reaches for when they do not
 * know, or do not remember, what the page is called here - "ssh" for Machines,
 * "apt" for App Maintenance, "theme" for Settings.
 */

import type { ReactNode } from "react";

import {
  AboutIcon,
  AttentionIcon,
  AuditIcon,
  CalendarIcon,
  ClipboardIcon,
  ColorIcon,
  CompanionIcon,
  DatabaseIcon,
  EmojiIcon,
  ExtensionsIcon,
  HerdrIcon,
  HomeIcon,
  MachinesIcon,
  MaintenanceIcon,
  MusicIcon,
  QuinjetIcon,
  SettingsIcon,
  SystemIcon,
  UsageIcon,
} from "../components/icons";

export type Route =
  | "home" | "usage" | "herdr" | "quinjet"
  | "system"
  | "machines"
  | "maintenance"
  | "database"
  | "media"
  | "calendar"
  | "attention"
  | "clipboard"
  | "color"
  | "emoji"
  | "audit"
  | "companion" | "extensions" | "settings" | "about";

export type NavItem = {
  id: Route;
  label: string;
  icon: ReactNode;
  /** Extension catalogue id; absent pages are part of Veronica itself. */
  extensionId?: string;
  /** What the page is for, shown in the palette. */
  hint: string;
  keywords: string;
};

export type NavGroup = { label: string; items: NavItem[] };

/**
 * The rail, in sections.
 *
 * Nineteen entries in one undifferentiated column is a list to read rather
 * than a menu to scan, so they are grouped by what a person came to do:
 * everything about agents first, then the machine, then the small tools, then
 * the app itself.
 */
export const NAV_GROUPS: NavGroup[] = [
  {
    label: "Agents",
    items: [
      { id: "home", label: "Home", icon: <HomeIcon />, hint: "The dashboard", keywords: "dashboard start overview clocks" },
      { id: "usage", label: "Agent Usage", icon: <UsageIcon />, extensionId: "usage", hint: "Spend, tokens and rate limits", keywords: "claude codex cursor cost spend tokens limits rings" },
      { id: "herdr", label: "Herdr", icon: <HerdrIcon />, extensionId: "herdr", hint: "Agent sessions and panes", keywords: "sessions panes tmux" },
      { id: "quinjet", label: "Quinjet", icon: <QuinjetIcon />, extensionId: "quinjet", hint: "Review workspaces", keywords: "review worktree git projects" },
      { id: "attention", label: "Attention", icon: <AttentionIcon />, extensionId: "attention", hint: "Focus sessions", keywords: "focus pomodoro timer deep work" },
    ],
  },
  {
    label: "This computer",
    items: [
      { id: "system", label: "System", icon: <SystemIcon />, extensionId: "system", hint: "CPU, memory, disks and audio", keywords: "cpu memory ram disk temperature sensors audio volume bluetooth processes" },
      { id: "machines", label: "Machines", icon: <MachinesIcon />, extensionId: "machines", hint: "This computer and SSH hosts", keywords: "ssh remote servers hosts docker containers wake" },
      { id: "maintenance", label: "App Maintenance", icon: <MaintenanceIcon />, extensionId: "appMaintenance", hint: "Updates, packages and cleanup", keywords: "apt snap flatpak update upgrade packages clean cache trash" },
      { id: "database", label: "Database", icon: <DatabaseIcon />, extensionId: "database", hint: "Browse and query databases", keywords: "sql postgres mysql sqlite redis query tables" },
    ],
  },
  {
    label: "Tools",
    items: [
      { id: "media", label: "Music", icon: <MusicIcon />, extensionId: "music", hint: "Local library and players", keywords: "music player spotify mpris tracks audio playback" },
      { id: "calendar", label: "Calendar", icon: <CalendarIcon />, extensionId: "calendar", hint: "Today and the days ahead", keywords: "meetings events agenda schedule" },
      { id: "clipboard", label: "Clipboard", icon: <ClipboardIcon />, extensionId: "clipboard", hint: "What you have copied", keywords: "copy paste history snippets" },
      { id: "color", label: "Color Picker", icon: <ColorIcon />, extensionId: "colorPicker", hint: "Sample and keep colours", keywords: "colour eyedropper hex rgb swatch pick" },
      { id: "emoji", label: "Emoji", icon: <EmojiIcon />, extensionId: "emoji", hint: "Search and insert emoji", keywords: "emoji symbol character insert" },
      { id: "audit", label: "Site Audit", icon: <AuditIcon />, extensionId: "seoAudit", hint: "Crawl a site for problems", keywords: "seo crawl website meta sitemap links" },
      { id: "companion", label: "Companion", icon: <CompanionIcon />, extensionId: "companion", hint: "Notes and voice memos", keywords: "notes memo record voice scratchpad" },
    ],
  },
  {
    label: "Veronica",
    items: [
      { id: "extensions", label: "Extensions", icon: <ExtensionsIcon />, hint: "The GNOME Shell extension", keywords: "gnome shell notch install enable" },
      { id: "settings", label: "Settings", icon: <SettingsIcon />, hint: "Appearance, alerts and shortcuts", keywords: "preferences theme appearance dark light alerts presenter backup shortcuts updates permissions" },
      { id: "about", label: "About", icon: <AboutIcon />, hint: "Version and licence", keywords: "version licence gpl credits" },
    ],
  },
];

/** Every entry, flattened, in rail order. */
export const NAV_ITEMS: NavItem[] = NAV_GROUPS.flatMap((group) => group.items);

const ROUTES = new Set<string>(NAV_ITEMS.map((item) => item.id));

/** Whether an untrusted string - an event payload, say - names a real page. */
export function isRoute(value: unknown): value is Route {
  return typeof value === "string" && ROUTES.has(value);
}

/** Initial destination supplied by the native on-demand window builder. */
export function routeFromSearch(search: string): Route {
  const candidate = new URLSearchParams(search).get("route");
  return isRoute(candidate) ? candidate : "home";
}

/** Rail groups after applying the extension catalogue's shared switches. */
export function visibleNavGroups(enabledExtensions?: ReadonlySet<string>): NavGroup[] {
  if (enabledExtensions === undefined) return NAV_GROUPS;
  return NAV_GROUPS
    .map((group) => ({
      ...group,
      items: group.items.filter(
        (item) => item.extensionId === undefined || enabledExtensions.has(item.extensionId),
      ),
    }))
    .filter((group) => group.items.length > 0);
}

/** Whether a route can be opened under the current extension switches. */
export function routeIsVisible(route: Route, enabledExtensions?: ReadonlySet<string>): boolean {
  const item = NAV_ITEMS.find((candidate) => candidate.id === route);
  return item !== undefined
    && (item.extensionId === undefined
      || enabledExtensions === undefined
      || enabledExtensions.has(item.extensionId));
}

/**
 * Entries matching `query`, best first.
 *
 * A label match outranks a keyword match, and a match at the start of the
 * label outranks one in the middle, so typing "cal" puts Calendar above Color
 * Picker rather than leaving the order to chance.
 */
export function searchNav(query: string, items: NavItem[] = NAV_ITEMS): NavItem[] {
  const needle = query.trim().toLowerCase();
  if (!needle) return items;
  const scored: { item: NavItem; score: number }[] = [];
  for (const item of items) {
    const label = item.label.toLowerCase();
    const score = label.startsWith(needle)
      ? 0
      : label.includes(needle)
        ? 1
        : item.hint.toLowerCase().includes(needle)
          ? 2
          : item.keywords.includes(needle)
            ? 3
            : -1;
    if (score >= 0) scored.push({ item, score });
  }
  return scored
    .sort((a, b) => a.score - b.score || items.indexOf(a.item) - items.indexOf(b.item))
    .map((entry) => entry.item);
}
