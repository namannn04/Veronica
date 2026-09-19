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
      { id: "usage", label: "Agent Usage", icon: <UsageIcon />, hint: "Spend, tokens and rate limits", keywords: "claude codex cursor cost spend tokens limits rings" },
      { id: "herdr", label: "Herdr", icon: <HerdrIcon />, hint: "Agent sessions and panes", keywords: "sessions panes tmux" },
      { id: "quinjet", label: "Quinjet", icon: <QuinjetIcon />, hint: "Review workspaces", keywords: "review worktree git projects" },
      { id: "attention", label: "Attention", icon: <AttentionIcon />, hint: "Focus sessions", keywords: "focus pomodoro timer deep work" },
    ],
  },
  {
    label: "This computer",
    items: [
      { id: "system", label: "System", icon: <SystemIcon />, hint: "CPU, memory, disks and audio", keywords: "cpu memory ram disk temperature sensors audio volume bluetooth processes" },
      { id: "machines", label: "Machines", icon: <MachinesIcon />, hint: "This computer and SSH hosts", keywords: "ssh remote servers hosts docker containers wake" },
      { id: "maintenance", label: "App Maintenance", icon: <MaintenanceIcon />, hint: "Updates, packages and cleanup", keywords: "apt snap flatpak update upgrade packages clean cache trash" },
      { id: "database", label: "Database", icon: <DatabaseIcon />, hint: "Browse and query databases", keywords: "sql postgres mysql sqlite redis query tables" },
    ],
  },
  {
    label: "Tools",
    items: [
      { id: "media", label: "Music", icon: <MusicIcon />, hint: "Local library and players", keywords: "music player spotify mpris tracks audio playback" },
      { id: "calendar", label: "Calendar", icon: <CalendarIcon />, hint: "Today and the days ahead", keywords: "meetings events agenda schedule" },
      { id: "clipboard", label: "Clipboard", icon: <ClipboardIcon />, hint: "What you have copied", keywords: "copy paste history snippets" },
      { id: "color", label: "Color Picker", icon: <ColorIcon />, hint: "Sample and keep colours", keywords: "colour eyedropper hex rgb swatch pick" },
      { id: "emoji", label: "Emoji", icon: <EmojiIcon />, hint: "Search and insert emoji", keywords: "emoji symbol character insert" },
      { id: "audit", label: "Site Audit", icon: <AuditIcon />, hint: "Crawl a site for problems", keywords: "seo crawl website meta sitemap links" },
      { id: "companion", label: "Companion", icon: <CompanionIcon />, hint: "Notes and voice memos", keywords: "notes memo record voice scratchpad" },
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

/**
 * Entries matching `query`, best first.
 *
 * A label match outranks a keyword match, and a match at the start of the
 * label outranks one in the middle, so typing "cal" puts Calendar above Color
 * Picker rather than leaving the order to chance.
 */
export function searchNav(query: string): NavItem[] {
  const needle = query.trim().toLowerCase();
  if (!needle) return NAV_ITEMS;
  const scored: { item: NavItem; score: number }[] = [];
  for (const item of NAV_ITEMS) {
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
    .sort((a, b) => a.score - b.score || NAV_ITEMS.indexOf(a.item) - NAV_ITEMS.indexOf(b.item))
    .map((entry) => entry.item);
}
