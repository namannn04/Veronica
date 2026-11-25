import type { BackendState, Granted } from "@veronica/extension-api";
import { Spinner } from "@veronica/ui";
import { Fragment, lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { CommandPalette } from "./components/CommandPalette";
import { ExtensionFrame } from "./extensions/ExtensionFrame";
import { InterfaceHost, type NavRequest } from "./extensions/InterfaceHost";
import { useInstalledExtensions } from "./extensions/useInstalledExtensions";
import { ipc, listenEvent } from "./lib/ipc";
import { frameKey, type LiveFrame, visitFrame } from "./lib/liveFrames";
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
import { ClipboardPage } from "./pages/ClipboardPage";
import { CompanionPage } from "./pages/CompanionPage";
import { DatabasePage } from "./pages/DatabasePage";
import { HerdrPage } from "./pages/HerdrPage";
import { HomePage } from "./pages/HomePage";
import { LibraryPage } from "./pages/LibraryPage";
import { MachinesPage } from "./pages/MachinesPage";
import { MaintenancePage } from "./pages/MaintenancePage";
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
  const [liveFrames, setLiveFrames] = useState<LiveFrame[]>([]);
  const handledLaunch = useRef(0);
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
  useEffect(() => {
    if (currentPage === undefined) {
      return;
    }
    const opened =
      launch.route === currentPage.id && launch.serial !== handledLaunch.current
        ? { serial: launch.serial, params: launch.params }
        : null;
    handledLaunch.current = launch.serial;
    setLiveFrames((current) => visitFrame(current, currentPage.id, opened));
  }, [currentPage, launch]);
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

      <main className="shell-main">
        <Fragment key={route}>
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
          {route === "attention" && <AttentionPage />}
          {route === "clipboard" && <ClipboardPage />}
          {route === "companion" && <CompanionPage />}
          {route === "library" && (
            <LibraryPage diagnostics={diagnostics} onChanged={loadDiagnostics} />
          )}
          {route === "settings" && <SettingsPage diagnostics={diagnostics} />}
          {route === "about" && <AboutPage diagnostics={diagnostics} />}
        </Fragment>
        {liveFrames.map((frame) => {
          const page = extensionPages?.find((candidate) => candidate.id === frame.route);
          if (page === undefined) {
            return null;
          }
          const extension = installed?.find((item) => item.id === page.frame.extension);
          return (
            <ExtensionPage
              key={frameKey(frame)}
              page={page}
              params={frame.params}
              granted={extension?.manifest?.permissions}
              state={extension?.state}
              active={frame.route === route}
            />
          );
        })}
      </main>

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
  active,
}: {
  page: ExtensionNavItem;
  params: unknown;
  granted: Granted | undefined;
  state: BackendState | undefined;
  active: boolean;
}) {
  const wrapper = useRef<HTMLDivElement>(null);
  useEffect(() => {
    wrapper.current?.toggleAttribute("inert", !active);
  }, [active]);
  return (
    <div ref={wrapper} className="shell-extension-page" hidden={!active}>
      <ExtensionFrame
        visible={active}
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
