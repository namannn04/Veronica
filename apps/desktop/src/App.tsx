import type { BackendState, Granted } from "@veronica/extension-api";
import { Spinner } from "@veronica/ui";
import { lazy, Suspense, useCallback, useEffect, useMemo, useState } from "react";
import { CommandPalette } from "./components/CommandPalette";
import { ExtensionFrame } from "./extensions/ExtensionFrame";
import { InterfaceHost, type NavRequest } from "./extensions/InterfaceHost";
import { useInstalledExtensions } from "./extensions/useInstalledExtensions";
import { ipc, listenEvent } from "./lib/ipc";
import { LocalPlayerProvider } from "./lib/localPlayer";
import {
  type ExtensionNavItem,
  extensionNavItems,
  extensionRoute,
  isRoute,
  mergeNavGroups,
  paletteItems,
  type Route,
  routeFromSearch,
  routeIsVisible,
  visibleNavGroups,
} from "./lib/navigation";
import { needsOnboarding } from "./lib/onboarding";
import { applyAppearance, applyPresenter } from "./lib/preferences";
import type { Diagnostics } from "./lib/types";
import { AboutPage } from "./pages/AboutPage";
import { AttentionPage } from "./pages/AttentionPage";
import { CalendarPage } from "./pages/CalendarPage";
import { ClipboardPage } from "./pages/ClipboardPage";
import { ColorPickerPage } from "./pages/ColorPickerPage";
import { CompanionPage } from "./pages/CompanionPage";
import { DatabasePage } from "./pages/DatabasePage";
import { EmojiPage } from "./pages/EmojiPage";
import { HerdrPage } from "./pages/HerdrPage";
import { HomePage } from "./pages/HomePage";
import { LibraryPage } from "./pages/LibraryPage";
import { MachinesPage } from "./pages/MachinesPage";
import { MaintenancePage } from "./pages/MaintenancePage";
import { MediaPage } from "./pages/MediaPage";
import { OnboardingPage } from "./pages/OnboardingPage";
import { QuinjetPage } from "./pages/QuinjetPage";
import { ServicesPage } from "./pages/ServicesPage";
import { SettingsPage } from "./pages/SettingsPage";
import { SystemPage } from "./pages/SystemPage";
import { UsagePage } from "./pages/UsagePage";
import { Sidebar } from "./shell/Sidebar";
import "./shell/Shell.css";
import "./shell/Presenter.css";

const WorkspacePage = lazy(() =>
  import("./pages/WorkspacePage").then((module) => ({ default: module.WorkspacePage })),
);

export function App() {
  const [route, setRoute] = useState<Route>(() => routeFromSearch(window.location.search));
  const [diagnostics, setDiagnostics] = useState<Diagnostics | null>(null);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [launch, setLaunch] = useState<{ route: Route; params: unknown; serial: number }>({
    route: "home",
    params: null,
    serial: 0,
  });
  const [pageMarks, setPageMarks] = useState<
    Partial<Record<Route, { badge?: string | number; title?: string }>>
  >({});
  const installed = useInstalledExtensions();
  const [onboarding, setOnboarding] = useState<boolean | null>(null);

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
  const extensionPages = useMemo(() => {
    if (installed === undefined) {
      return undefined;
    }
    return extensionNavItems(installed).map((page) => {
      const marks = pageMarks[page.id];
      return marks === undefined
        ? page
        : { ...page, label: marks.title ?? page.label, badge: marks.badge ?? undefined };
    });
  }, [installed, pageMarks]);
  const navGroups = useMemo(
    () => mergeNavGroups(visibleNavGroups(enabledExtensions), extensionPages ?? []),
    [enabledExtensions, extensionPages],
  );
  const navItems = useMemo(
    () => paletteItems(navGroups, extensionPages ?? []),
    [navGroups, extensionPages],
  );
  const currentPage = extensionPages?.find((page) => page.id === route);
  const currentExtension = installed?.find(
    (extension) => extension.id === currentPage?.frame.extension,
  );
  const extensionTitle = useCallback(
    (id: string) => installed?.find((extension) => extension.id === id)?.manifest?.title ?? id,
    [installed],
  );
  const onExtensionNav = useCallback((request: NavRequest) => {
    const source = extensionRoute(request.extension, request.page);
    if (request.action === "open") {
      const target = extensionRoute(request.extension, request.target);
      setLaunch((current) => ({
        route: target,
        params: request.params,
        serial: current.serial + 1,
      }));
      setRoute(target);
      return;
    }
    setPageMarks((current) => ({
      ...current,
      [source]: {
        ...current[source],
        ...(request.action === "badge"
          ? { badge: request.badge ?? undefined }
          : { title: request.title }),
      },
    }));
  }, []);
  const navigateTo = useCallback((next: Route) => {
    setLaunch((current) => ({ ...current, route: next, params: null }));
    setRoute(next);
  }, []);

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

  useEffect(() => {
    if (diagnostics === null || onboarding !== null) {
      return;
    }
    void ipc.settingsAll().then(
      (settings) =>
        setOnboarding(needsOnboarding(settings, diagnostics.extensions, diagnostics.legacyToggles)),
      () => setOnboarding(false),
    );
  }, [diagnostics, onboarding]);

  useEffect(() => {
    if (!routeIsVisible(route, enabledExtensions, extensionPages)) {
      setRoute("home");
    }
  }, [enabledExtensions, extensionPages, route]);
  useEffect(() => {
    const resolved = listenEvent("session-resolved", () => void loadDiagnostics());
    const navigate = listenEvent<string>("navigate", (event) => {
      if (isRoute(event.payload)) {
        navigateTo(
          routeIsVisible(event.payload, enabledExtensions, extensionPages) ? event.payload : "home",
        );
      }
    });
    return () => {
      void resolved.then((un) => un());
      void navigate.then((un) => un());
    };
  }, [enabledExtensions, extensionPages, loadDiagnostics, navigateTo]);

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

  if (onboarding === true) {
    return (
      <OnboardingPage
        onDone={() => {
          setOnboarding(false);
          navigateTo("home");
        }}
      />
    );
  }

  return (
    <div className="shell">
      <Sidebar
        groups={navGroups}
        route={route}
        version={diagnostics?.version}
        onNavigate={navigateTo}
        onSearch={() => setPaletteOpen(true)}
      />

      <LocalPlayerProvider>
        <main className="shell-main" key={route}>
          {route === "home" && <HomePage onNavigate={navigateTo} />}
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
          {route === "companion" && <CompanionPage />}
          {route === "library" && (
            <LibraryPage diagnostics={diagnostics} onChanged={loadDiagnostics} />
          )}
          {route === "settings" && <SettingsPage diagnostics={diagnostics} />}
          {route === "about" && <AboutPage diagnostics={diagnostics} />}
          {currentPage !== undefined && (
            <ExtensionPage
              key={launch.serial}
              page={currentPage}
              params={launch.route === route ? launch.params : null}
              granted={currentExtension?.manifest?.permissions}
              state={currentExtension?.state}
            />
          )}
        </main>
      </LocalPlayerProvider>

      <CommandPalette
        open={paletteOpen}
        onClose={() => setPaletteOpen(false)}
        onNavigate={navigateTo}
        items={navItems}
      />
      <InterfaceHost titleOf={extensionTitle} onNav={onExtensionNav} />
    </div>
  );
}

function ExtensionPage({
  page,
  params,
  granted,
  state,
}: {
  page: ExtensionNavItem;
  params: unknown;
  granted: Granted | undefined;
  state: BackendState | undefined;
}) {
  return (
    <div className="shell-extension-page">
      <ExtensionFrame
        extension={page.frame.extension}
        page={page.frame.page}
        entry={page.frame.entry}
        title={page.label}
        params={params}
        granted={granted}
        state={state}
      />
    </div>
  );
}
