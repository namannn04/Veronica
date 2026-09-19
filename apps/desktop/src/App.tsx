import { useCallback, useEffect, useMemo, useState } from "react";

import { AboutPage } from "./pages/AboutPage";
import { AttentionPage } from "./pages/AttentionPage";
import { AuditPage } from "./pages/AuditPage";
import { CalendarPage } from "./pages/CalendarPage";
import { ClipboardPage } from "./pages/ClipboardPage";
import { CompanionPage } from "./pages/CompanionPage";
import { DatabasePage } from "./pages/DatabasePage";
import { ColorPickerPage } from "./pages/ColorPickerPage";
import { EmojiPage } from "./pages/EmojiPage";
import { ExtensionsPage } from "./pages/ExtensionsPage";
import { MachinesPage } from "./pages/MachinesPage";
import { MaintenancePage } from "./pages/MaintenancePage";
import { MediaPage } from "./pages/MediaPage";
import { SystemPage } from "./pages/SystemPage";
import { UsagePage } from "./pages/UsagePage";
import { HomePage } from "./pages/HomePage";
import { HerdrPage } from "./pages/HerdrPage";
import { QuinjetPage } from "./pages/QuinjetPage";
import { SettingsPage } from "./pages/SettingsPage";
import { CommandPalette } from "./components/CommandPalette";
import { SearchIcon } from "./components/icons";
import { ipc, listenEvent } from "./lib/ipc";
import { isRoute, routeIsVisible, visibleNavGroups, type Route } from "./lib/navigation";
import { applyAppearance, applyPresenter } from "./lib/preferences";
import type { Diagnostics } from "./lib/types";

export function App() {
  const [route, setRoute] = useState<Route>("home");
  const [diagnostics, setDiagnostics] = useState<Diagnostics | null>(null);
  const [paletteOpen, setPaletteOpen] = useState(false);

  const enabledExtensions = useMemo(() => {
    if (diagnostics === null) return undefined;
    return new Set(
      diagnostics.extensions
        .filter((extension) => extension.enabled)
        .map((extension) => extension.id),
    );
  }, [diagnostics]);
  // One filtered tree feeds both the rail and Ctrl+K, so their reachability
  // cannot drift when a switch changes.
  const navGroups = useMemo(
    () => visibleNavGroups(enabledExtensions),
    [enabledExtensions],
  );
  const navItems = useMemo(
    () => navGroups.flatMap((group) => group.items),
    [navGroups],
  );

  const loadDiagnostics = useCallback(async () => {
    try {
      setDiagnostics(await ipc.diagnostics());
    } catch {
      // Diagnostics are informational; a failure must not blank the app.
    }
  }, []);

  const applySettings = useCallback((settings: Record<string, unknown>) => {
    applyAppearance(settings.appearance);
  }, []);

  // Presenter mode is resolved in the core — enabled, and either the manual
  // switch or a detected screen share — so the interface asks rather than
  // recomputing the gate and risking a different answer.
  const applyPresenterState = useCallback(async () => {
    try {
      const view = await ipc.presenterState();
      applyPresenter(view.active, view.blurredClasses);
    } catch {
      // Not running as the desktop app: nothing to blur.
    }
  }, []);

  useEffect(() => {
    void loadDiagnostics();
    void ipc.settingsAll().then(applySettings).catch(() => {});
    void applyPresenterState();
  }, [applySettings, applyPresenterState, loadDiagnostics]);

  useEffect(() => {
    document.querySelector(".content")?.scrollTo({ top: 0 });
  }, [route]);

  useEffect(() => {
    if (!routeIsVisible(route, enabledExtensions)) setRoute("home");
  }, [enabledExtensions, route]);

  // The portal probe finishes after launch, which can change what is available.
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
      void ipc.settingsAll().then(applySettings).catch(() => {});
      void applyPresenterState();
      void loadDiagnostics();
    });
    return () => {
      void updated.then((un) => un());
    };
  }, [applySettings, applyPresenterState, loadDiagnostics]);

  // Ctrl+K opens the palette from anywhere, including from inside a text field,
  // which is the whole point of a jump-to shortcut.
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
      <nav className="sidebar" aria-label="Sections">
        <div className="brand">
          <div className="brand-mark" aria-hidden="true">
            V
          </div>
          <div>
            <div className="brand-name">Veronica</div>
            <div className="brand-version">{diagnostics?.version ?? ""}</div>
          </div>
        </div>

        <button className="nav-search" onClick={() => setPaletteOpen(true)}>
          <SearchIcon />
          <span>Go to…</span>
          <kbd>Ctrl K</kbd>
        </button>

        <div className="nav-list">
          {navGroups.map((group) => (
            <div key={group.label} className="nav-group">
              <div className="nav-section">{group.label}</div>
              {group.items.map((item) => (
                <button
                  key={item.id}
                  className="nav-item"
                  aria-current={route === item.id ? "page" : undefined}
                  onClick={() => setRoute(item.id)}
                >
                  <span className="nav-glyph">{item.icon}</span>
                  {item.label}
                </button>
              ))}
            </div>
          ))}
        </div>
      </nav>

      <main className="content">
        {route === "home" && <HomePage onNavigate={setRoute} />}
        {route === "usage" && <UsagePage />}
        {route === "herdr" && <HerdrPage />}
        {route === "quinjet" && <QuinjetPage />}
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

      <CommandPalette
        open={paletteOpen}
        onClose={() => setPaletteOpen(false)}
        onNavigate={setRoute}
        items={navItems}
      />
    </div>
  );
}
