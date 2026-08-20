import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";

import { App } from "./App";
import { Notch } from "./Notch";
import "./styles.css";

/**
 * One bundle serves both windows.
 *
 * The surface is chosen by window label, which is set when the window is created
 * and does not depend on how the URL resolved - a query string would differ
 * between the dev server and the embedded bundle. The query string is still
 * honoured as a fallback for opening a surface in a plain browser.
 */
function resolveSurface(): "main" | "notch" {
  try {
    if (getCurrentWindow().label === "notch") return "notch";
  } catch {
    // Not running inside Tauri; fall through to the query string.
  }
  return new URLSearchParams(window.location.search).get("surface") === "notch"
    ? "notch"
    : "main";
}

const surface = resolveSurface();
if (surface === "notch") {
  document.body.classList.add("surface-notch");
}

const root = document.getElementById("root");
if (!root) throw new Error("missing #root");

ReactDOM.createRoot(root).render(
  <React.StrictMode>{surface === "notch" ? <Notch /> : <App />}</React.StrictMode>,
);
