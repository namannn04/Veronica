import type { ReactNode } from "react";

import {
  AboutIcon,
  CompanionIcon,
  DatabaseIcon,
  ExtensionsIcon,
  HerdrIcon,
  HomeIcon,
  MachinesIcon,
  MaintenanceIcon,
  NamedIcon,
  PortsIcon,
  SettingsIcon,
  SystemIcon,
  WorkspaceIcon,
} from "../components/icons";
import { frameSource } from "../extensions/bridge";
import type { InstalledExtension } from "./types";

export type BuiltinRoute =
  | "home"
  | "agents"
  | "system"
  | "services"
  | "workspace"
  | "machines"
  | "maintenance"
  | "database"
  | "companion"
  | "library"
  | "settings"
  | "about";

export type ExtensionRoute = `ext:${string}/${string}`;
export type Route = BuiltinRoute | ExtensionRoute;

export type ExtensionPageRef = { extension: string; page: string; entry: string };

export type NavItem = {
  id: Route;
  label: string;
  icon: ReactNode;
  extensionId?: string;
  hint: string;
  keywords: string;
  badge?: string | number;
  frame?: ExtensionPageRef;
};

export type ExtensionNavItem = NavItem & { frame: ExtensionPageRef; group?: string; order: number };

export type NavGroup = { label: string; items: NavItem[] };

export const NAV_GROUPS: NavGroup[] = [
  {
    label: "Agents",
    items: [
      {
        id: "home",
        label: "Home",
        icon: <HomeIcon />,
        hint: "The dashboard",
        keywords: "dashboard start overview clocks",
      },
      {
        id: "agents",
        label: "Agents",
        icon: <HerdrIcon />,
        hint: "Agent terminals and providers",
        keywords: "agents terminal claude codex providers sessions",
      },
    ],
  },
  {
    label: "This computer",
    items: [
      {
        id: "system",
        label: "System",
        icon: <SystemIcon />,
        extensionId: "system",
        hint: "CPU, memory, disks and audio",
        keywords: "cpu memory ram disk temperature sensors audio volume bluetooth processes",
      },
      {
        id: "services",
        label: "Ports & Docker",
        icon: <PortsIcon />,
        extensionId: "system",
        hint: "Listening ports and local containers",
        keywords: "ports port localhost 3000 8080 server process stop docker containers daemon",
      },
      {
        id: "workspace",
        label: "Developer Workspace",
        icon: <WorkspaceIcon />,
        hint: "Project Doctor and service playground",
        keywords: "project doctor graftcode mcp playground developer ports",
      },
      {
        id: "machines",
        label: "Machines",
        icon: <MachinesIcon />,
        extensionId: "machines",
        hint: "This computer and SSH hosts",
        keywords: "ssh remote servers hosts docker containers wake",
      },
      {
        id: "maintenance",
        label: "App Maintenance",
        icon: <MaintenanceIcon />,
        extensionId: "appMaintenance",
        hint: "Updates, packages and cleanup",
        keywords: "apt snap flatpak update upgrade packages clean cache trash",
      },
      {
        id: "database",
        label: "Database",
        icon: <DatabaseIcon />,
        extensionId: "database",
        hint: "Browse and query databases",
        keywords: "sql postgres mysql sqlite redis query tables",
      },
    ],
  },
  {
    label: "Tools",
    items: [
      {
        id: "companion",
        label: "Companion",
        icon: <CompanionIcon />,
        extensionId: "companion",
        hint: "Notes and voice memos",
        keywords: "notes memo record voice scratchpad",
      },
    ],
  },
  {
    label: "Veronica",
    items: [
      {
        id: "library",
        label: "Library",
        icon: <ExtensionsIcon />,
        hint: "Install, update and turn extensions on or off",
        keywords:
          "extensions features catalog catalogue enable disable install update remove tools notch",
      },
      {
        id: "settings",
        label: "Settings",
        icon: <SettingsIcon />,
        hint: "Appearance, alerts and shortcuts",
        keywords:
          "preferences theme appearance dark light alerts presenter backup shortcuts updates permissions",
      },
      {
        id: "about",
        label: "About",
        icon: <AboutIcon />,
        hint: "Version and licence",
        keywords: "version licence gpl credits",
      },
    ],
  },
];

export const NAV_ITEMS: NavItem[] = NAV_GROUPS.flatMap((group) => group.items);

const ROUTES = new Set<string>(NAV_ITEMS.map((item) => item.id));
const EXTENSION_ROUTE = /^ext:([a-z][a-z0-9-]{1,31})\/([A-Za-z0-9._-]{1,64})$/;

export function extensionRoute(extension: string, page: string): ExtensionRoute {
  return `ext:${extension}/${page}`;
}

export function parseExtensionRoute(route: string): { extension: string; page: string } | null {
  const match = EXTENSION_ROUTE.exec(route);
  return match === null ? null : { extension: match[1], page: match[2] };
}

export function isRoute(value: unknown): value is Route {
  return typeof value === "string" && (ROUTES.has(value) || EXTENSION_ROUTE.test(value));
}

export function extensionNavItems(installed: readonly InstalledExtension[]): ExtensionNavItem[] {
  const items: ExtensionNavItem[] = [];
  for (const extension of installed) {
    const manifest = extension.manifest;
    if (!extension.enabled || manifest === null) {
      continue;
    }
    for (const page of manifest.ui?.pages ?? []) {
      const id = extensionRoute(extension.id, page.id);
      if (!EXTENSION_ROUTE.test(id) || frameSource(extension.id, page.entry) === null) {
        continue;
      }
      items.push({
        id,
        label: page.title,
        icon: <NamedIcon name={page.icon} />,
        hint: page.hint ?? manifest.summary,
        keywords: [manifest.title, extension.id, ...(page.keywords ?? [])].join(" ").toLowerCase(),
        frame: { extension: extension.id, page: page.id, entry: page.entry },
        group: page.nav?.group,
        order: page.nav?.order ?? 0,
      });
    }
  }
  return items;
}

export function mergeNavGroups(groups: NavGroup[], pages: readonly ExtensionNavItem[]): NavGroup[] {
  const merged = groups.map((group) => ({ ...group, items: [...group.items] }));
  const hostGroup = merged.at(-1);
  const added: NavGroup[] = [];
  const ranked = pages
    .filter((page) => page.group !== undefined)
    .sort((a, b) => a.order - b.order || a.label.localeCompare(b.label));
  for (const page of ranked) {
    const label = page.group ?? "";
    const wanted = label.toLowerCase();
    const existing = [...merged, ...added].find(
      (group) => group !== hostGroup && group.label.toLowerCase() === wanted,
    );
    if (existing !== undefined) {
      existing.items.push(page);
    } else {
      added.push({ label, items: [page] });
    }
  }
  return hostGroup === undefined ? added : [...merged.slice(0, -1), ...added, hostGroup];
}

export function paletteItems(groups: NavGroup[], pages: readonly ExtensionNavItem[]): NavItem[] {
  const items = groups.flatMap((group) => group.items);
  return [...items, ...pages.filter((page) => !items.includes(page))];
}

export function routeFromSearch(search: string): Route {
  const candidate = new URLSearchParams(search).get("route");
  return isRoute(candidate) ? candidate : "home";
}

export function visibleNavGroups(enabledExtensions?: ReadonlySet<string>): NavGroup[] {
  if (enabledExtensions === undefined) {
    return NAV_GROUPS;
  }
  return NAV_GROUPS.map((group) => ({
    ...group,
    items: group.items.filter(
      (item) => item.extensionId === undefined || enabledExtensions.has(item.extensionId),
    ),
  })).filter((group) => group.items.length > 0);
}

export function routeIsVisible(
  route: Route,
  enabledExtensions?: ReadonlySet<string>,
  extensionPages?: readonly NavItem[],
): boolean {
  if (parseExtensionRoute(route) !== null) {
    return extensionPages === undefined || extensionPages.some((page) => page.id === route);
  }
  const item = NAV_ITEMS.find((candidate) => candidate.id === route);
  return (
    item !== undefined &&
    (item.extensionId === undefined ||
      enabledExtensions === undefined ||
      enabledExtensions.has(item.extensionId))
  );
}

export function searchNav(query: string, items: NavItem[] = NAV_ITEMS): NavItem[] {
  const needle = query.trim().toLowerCase();
  if (!needle) {
    return items;
  }
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
    if (score >= 0) {
      scored.push({ item, score });
    }
  }
  return scored
    .sort((a, b) => a.score - b.score || items.indexOf(a.item) - items.indexOf(b.item))
    .map((entry) => entry.item);
}
