import { lazy, Suspense, useCallback, useEffect, useMemo, useState } from "react";
import { CommandPalette } from "./components/CommandPalette";
import { ipc, listenEvent } from "./lib/ipc";
import { LocalPlayerProvider } from "./lib/localPlayer";
import {
  isRoute,
  type Route,
  routeFromSearch,
  routeIsVisible,
  visibleNavGroups,
} from "./lib/navigation";
import { applyAppearance, applyPresenter } from "./lib/preferences";
import type { Diagnostics } from "./lib/types";
import { AboutPage } from "./pages/AboutPage";
import { AttentionPage } from "./pages/AttentionPage";
import { AuditPage } from "./pages/AuditPage";
import { CalendarPage } from "./pages/CalendarPage";
import { ClipboardPage } from "./pages/ClipboardPage";
import { ColorPickerPage } from "./pages/ColorPickerPage";
import { CompanionPage } from "./pages/CompanionPage";
import { DatabasePage } from "./pages/DatabasePage";
import { EmojiPage } from "./pages/EmojiPage";
import { ExtensionsPage } from "./pages/ExtensionsPage";
import { HerdrPage } from "./pages/HerdrPage";
import { HomePage } from "./pages/HomePage";
import { MachinesPage } from "./pages/MachinesPage";
import { MaintenancePage } from "./pages/MaintenancePage";
import { MediaPage } from "./pages/MediaPage";
import { QuinjetPage } from "./pages/QuinjetPage";
import { ServicesPage } from "./pages/ServicesPage";
import { SettingsPage } from "./pages/SettingsPage";
import { SystemPage } from "./pages/SystemPage";
import { UsagePage } from "./pages/UsagePage";
import { Sidebar } from "./shell/Sidebar";
import { Spinner } from "./ui";
import "./shell/Shell.css";
import "./shell/Presenter.css";

const WorkspacePage = lazy(() =>
  import("./pages/WorkspacePage").then((module) => ({ default: module.WorkspacePage })),
);

export function App() {
  const [route, setRoute] = useState<Route>(() => routeFromSearch(window.location.search));
  const [diagnostics, setDiagnostics] = useState<Diagnostics | null>(null);
  const [paletteOpen, setPaletteOpen] = useState(false);

  const enabledExtensions = useMemo(() => {
    if (diagnostics === null) {
      return undefined;
    }
    return new Set(
      diagnostics.extensions
        .filter((extension) => extension.enabled)
        .map((extension) => extension.id),
    );
  }, [diagnostics]);
  const navGroups = useMemo(() => visibleNavGroups(enabledExtensions), [enabledExtensions]);
  const navItems = useMemo(() => navGroups.flatMap((group) => group.items), [navGroups]);

  const loadDiagnostics = useCallback(async () => {
    try {
      setDiagnostics(await ipc.diagnostics());
    } catch {
      return;
    }
  }, []);

  const applySettings = useCallback((settings: Record<string, unknown>) => {
    applyAppearance(settings.appearance);
  }, []);
  const applyPresenterState = useCallback(async () => {
    try {
      const view = await ipc.presenterState();
      applyPresenter(view.active, view.blurredClasses);
    } catch {
      return;
    }
  }, []);

  useEffect(() => {
    void loadDiagnostics();
    void ipc
      .settingsAll()
      .then(applySettings)
      .catch(() => {});
    void applyPresenterState();
  }, [applySettings, applyPresenterState, loadDiagnostics]);

  // biome-ignore lint/correctness/useExhaustiveDependencies: route is the trigger: every navigation starts the main view at the top
  useEffect(() => {
    if (!routeIsVisible(route, enabledExtensions)) {
      setRoute("home");
    }
  }, [enabledExtensions, route]);
  useEffect(() => {
    const resolved = listenEvent("session-resolved", () => void loadDiagnostics());
    const navigate = listenEvent<string>("navigate", (event) => {
      if (isRoute(event.payload)) {
        setRoute(routeIsVisible(event.payload, enabledExtensions) ? event.payload : "home");
      }
    });
    return () => {
      void resolved.then((un) => un());
      void navigate.then((un) => un());
    };
  }, [enabledExtensions, loadDiagnostics]);

  useEffect(() => {
    const updated = listenEvent("settings-updated", () => {
      void ipc
        .settingsAll()
        .then(applySettings)
        .catch(() => {});
      void applyPresenterState();
      void loadDiagnostics();
    });
    return () => {
      void updated.then((un) => un());
    };
  }, [applySettings, applyPresenterState, loadDiagnostics]);
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setPaletteOpen((open) => !open);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  return (
    <div className="shell">
      <Sidebar
        groups={navGroups}
        route={route}
        version={diagnostics?.version}
        onNavigate={setRoute}
        onSearch={() => setPaletteOpen(true)}
      />

      <LocalPlayerProvider>
        <main className="shell-main" key={route}>
          {route === "home" && <HomePage onNavigate={setRoute} />}
          {route === "usage" && <UsagePage />}
          {route === "herdr" && <HerdrPage />}
          {route === "quinjet" && <QuinjetPage />}
          {route === "services" && <ServicesPage />}
          {route === "workspace" && (
            <Suspense
              fallback={
                <div className="shell-loading">
                  <Spinner size={20} label="Opening Developer Workspace" />
                </div>
              }
            >
              <WorkspacePage />
            </Suspense>
          )}
          {route === "system" && <SystemPage />}
          {route === "machines" && <MachinesPage />}
          {route === "maintenance" && <MaintenancePage />}
          {route === "database" && <DatabasePage />}
          {route === "media" && <MediaPage />}
          {route === "calendar" && <CalendarPage />}
          {route === "attention" && <AttentionPage />}
          {route === "clipboard" && <ClipboardPage />}
          {route === "color" && <ColorPickerPage />}
          {route === "emoji" && <EmojiPage />}
          {route === "audit" && <AuditPage />}
          {route === "companion" && <CompanionPage />}
          {route === "extensions" && (
            <ExtensionsPage diagnostics={diagnostics} onChanged={loadDiagnostics} />
          )}
          {route === "settings" && <SettingsPage diagnostics={diagnostics} />}
          {route === "about" && <AboutPage diagnostics={diagnostics} />}
        </main>
      </LocalPlayerProvider>

      <CommandPalette
        open={paletteOpen}
        onClose={() => setPaletteOpen(false)}
        onNavigate={setRoute}
        items={navItems}
      />
    </div>
  );
}
