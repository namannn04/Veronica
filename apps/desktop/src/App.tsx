import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";

import { ClipboardPage } from "./pages/ClipboardPage";
import { DiagnosticsPage } from "./pages/DiagnosticsPage";
import { CalendarPage } from "./pages/CalendarPage";
import { ExtensionsPage } from "./pages/ExtensionsPage";
import { MachinesPage } from "./pages/MachinesPage";
import { MediaPage } from "./pages/MediaPage";
import { SystemPage } from "./pages/SystemPage";
import { UsagePage } from "./pages/UsagePage";
import { ipc } from "./lib/ipc";
import type { Diagnostics } from "./lib/types";

type Route =
  | "usage"
  | "system"
  | "machines"
  | "media"
  | "calendar"
  | "clipboard"
  | "extensions"
  | "diagnostics";

const NAV: { id: Route; label: string; section: string }[] = [
  { id: "usage", label: "Agent Usage", section: "Agent" },
  { id: "system", label: "System", section: "System" },
  { id: "machines", label: "Machines", section: "System" },
  { id: "media", label: "Media", section: "Media" },
  { id: "calendar", label: "Calendar", section: "Media" },
  { id: "clipboard", label: "Clipboard", section: "Utilities" },
  { id: "extensions", label: "Extensions", section: "Veronica" },
  { id: "diagnostics", label: "Diagnostics", section: "Veronica" },
];

export function App() {
  const [route, setRoute] = useState<Route>("usage");
  const [diagnostics, setDiagnostics] = useState<Diagnostics | null>(null);

  const loadDiagnostics = useCallback(async () => {
    try {
      setDiagnostics(await ipc.diagnostics());
    } catch {
      // Diagnostics are informational; a failure must not blank the app.
    }
  }, []);

  useEffect(() => {
    void loadDiagnostics();
  }, [loadDiagnostics]);

  // The portal probe finishes after launch, which can change what is available.
  useEffect(() => {
    const resolved = listen("session://resolved", () => void loadDiagnostics());
    return () => {
      void resolved.then((un) => un());
    };
  }, [loadDiagnostics]);

  let sectionSeen = "";

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

        {NAV.map((item) => {
          const heading = item.section !== sectionSeen ? item.section : null;
          sectionSeen = item.section;
          const entry = diagnostics?.extensions.find((e) => e.id === item.id);
          return (
            <div key={item.id}>
              {heading && <div className="nav-section">{heading}</div>}
              <button
                className="nav-item"
                aria-current={route === item.id ? "page" : undefined}
                onClick={() => setRoute(item.id)}
              >
                {entry ? (
                  <span
                    className={`dot ${entry.availability === "unavailable" ? "off" : "on"}`}
                    aria-hidden="true"
                  />
                ) : (
                  <span className="dot" aria-hidden="true" />
                )}
                {item.label}
              </button>
            </div>
          );
        })}
      </nav>

      <main className="content">
        {route === "usage" && <UsagePage />}
        {route === "system" && <SystemPage />}
        {route === "machines" && <MachinesPage />}
        {route === "media" && <MediaPage />}
        {route === "calendar" && <CalendarPage />}
        {route === "clipboard" && <ClipboardPage />}
        {route === "extensions" && (
          <ExtensionsPage diagnostics={diagnostics} onChanged={loadDiagnostics} />
        )}
        {route === "diagnostics" && <DiagnosticsPage diagnostics={diagnostics} />}
      </main>
    </div>
  );
}
