import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";

import { AboutPage } from "./pages/AboutPage";
import { AttentionPage } from "./pages/AttentionPage";
import { CalendarPage } from "./pages/CalendarPage";
import { ClipboardPage } from "./pages/ClipboardPage";
import { ColorPickerPage } from "./pages/ColorPickerPage";
import { ExtensionsPage } from "./pages/ExtensionsPage";
import { MachinesPage } from "./pages/MachinesPage";
import { MediaPage } from "./pages/MediaPage";
import { SystemPage } from "./pages/SystemPage";
import { UsagePage } from "./pages/UsagePage";
import { HomePage } from "./pages/HomePage";
import { HerdrPage } from "./pages/HerdrPage";
import { SettingsPage } from "./pages/SettingsPage";
import { UnavailablePage } from "./pages/UnavailablePage";
import { ipc } from "./lib/ipc";
import { applyAppearance, applyPresenter } from "./lib/preferences";
import type { Diagnostics } from "./lib/types";

type Route =
  | "home" | "usage" | "herdr"
  | "system"
  | "machines"
  | "media"
  | "calendar"
  | "attention"
  | "clipboard"
  | "color"
  | "companion" | "extensions" | "settings" | "about";

const NAV: { id: Route; label: string; icon: string }[] = [
  { id: "home", label: "Home", icon: "⌂" },
  { id: "usage", label: "Agent Usage", icon: "◒" },
  { id: "herdr", label: "Herdr", icon: "◉" },
  { id: "media", label: "Music", icon: "♫" },
  { id: "calendar", label: "Calendar", icon: "▣" },
  { id: "attention", label: "Attention", icon: "◎" },
  { id: "system", label: "System", icon: "⌁" },
  { id: "machines", label: "Machines", icon: "▤" },
  { id: "clipboard", label: "Clipboard", icon: "❐" },
  { id: "color", label: "Color Picker", icon: "◐" },
  { id: "companion", label: "Companion", icon: "✦" },
  { id: "extensions", label: "Extensions", icon: "◇" },
  { id: "settings", label: "Settings", icon: "⚙" },
  { id: "about", label: "About", icon: "ⓘ" },
];

export function App() {
  const [route, setRoute] = useState<Route>("home");
  const [diagnostics, setDiagnostics] = useState<Diagnostics | null>(null);

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

  // The portal probe finishes after launch, which can change what is available.
  useEffect(() => {
    const resolved = listen("session-resolved", () => void loadDiagnostics());
    const navigate = listen<string>("navigate", (event) => {
      if (NAV.some((item) => item.id === event.payload)) setRoute(event.payload as Route);
    });
    return () => {
      void resolved.then((un) => un());
      void navigate.then((un) => un());
    };
  }, [loadDiagnostics]);

  useEffect(() => {
    const updated = listen("settings-updated", () => {
      void ipc.settingsAll().then(applySettings).catch(() => {});
      void applyPresenterState();
    });
    return () => {
      void updated.then((un) => un());
    };
  }, [applySettings, applyPresenterState]);

  return (
    <div className="shell">
      <nav className="sidebar">
        <div className="brand">
          <div className="brand-mark" aria-hidden="true">
            V
          </div>
          <div>
            <div className="brand-name">Veronica</div>
            <div className="brand-version">{diagnostics?.version ?? ""}</div>
          </div>
        </div>

        <div className="nav-list">
          {NAV.map((item) => (
              <button
                key={item.id}
                className="nav-item"
                aria-current={route === item.id ? "page" : undefined}
                onClick={() => setRoute(item.id)}
              >
                <span className="nav-glyph" aria-hidden="true">{item.icon}</span>
                {item.label}
              </button>
          ))}
        </div>
      </nav>

      <main className="content">
        {route === "home" && <HomePage onNavigate={setRoute} />}
        {route === "usage" && <UsagePage />}
        {route === "herdr" && <HerdrPage />}
        {route === "system" && <SystemPage />}
        {route === "machines" && <MachinesPage />}
        {route === "media" && <MediaPage />}
        {route === "calendar" && <CalendarPage />}
        {route === "attention" && <AttentionPage />}
        {route === "clipboard" && <ClipboardPage />}
        {route === "color" && <ColorPickerPage />}
        {route === "companion" && <UnavailablePage title="Companion" description="Notes and voice memos still need recording, storage and playback implementations for Linux." />}
        {route === "extensions" && (
          <ExtensionsPage diagnostics={diagnostics} onChanged={loadDiagnostics} />
        )}
        {route === "settings" && <SettingsPage diagnostics={diagnostics} />}
        {route === "about" && <AboutPage diagnostics={diagnostics} />}
      </main>
    </div>
  );
}
