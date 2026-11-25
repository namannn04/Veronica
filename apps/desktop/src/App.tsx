import type { BackendState, Granted } from "@veronica/extension-api";
import { Fragment, useCallback, useEffect, useMemo, useRef, useState } from "react";
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
  NAV_GROUPS,
  paletteItems,
  type Route,
  routeFromSearch,
  routeIsVisible,
} from "./lib/navigation";
import { needsOnboarding } from "./lib/onboarding";
import { applyAppearance, applyPresenter } from "./lib/preferences";
import type { Diagnostics } from "./lib/types";
import { AboutPage } from "./pages/AboutPage";
import { AgentsPage } from "./pages/AgentsPage";
import { HomePage } from "./pages/HomePage";
import { LibraryPage } from "./pages/LibraryPage";
import { OnboardingPage } from "./pages/OnboardingPage";
import { SettingsPage } from "./pages/SettingsPage";
import { Sidebar } from "./shell/Sidebar";
import "./shell/Shell.css";
import "./shell/Presenter.css";

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
    () => mergeNavGroups(NAV_GROUPS, extensionPages ?? []),
    [extensionPages],
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
    if (!routeIsVisible(route, extensionPages)) {
      setRoute("home");
    }
  }, [extensionPages, route]);
  useEffect(() => {
    const resolved = listenEvent("session-resolved", () => void loadDiagnostics());
    const navigate = listenEvent<string>("navigate", (event) => {
      if (isRoute(event.payload)) {
        navigateTo(routeIsVisible(event.payload, extensionPages) ? event.payload : "home");
      }
    });
    return () => {
      void resolved.then((un) => un());
      void navigate.then((un) => un());
    };
  }, [extensionPages, loadDiagnostics, navigateTo]);

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
          {route === "agents" && <AgentsPage />}
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
