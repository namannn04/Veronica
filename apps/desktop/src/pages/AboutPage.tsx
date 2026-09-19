import { ipc } from "../lib/ipc";
import type { Diagnostics } from "../lib/types";

export function AboutPage({ diagnostics }: { diagnostics: Diagnostics | null }) {
  return <div className="about-page">
    <div className="about-mark">V</div>
    <h1>Veronica</h1>
    <div className="about-version">Version {diagnostics?.version ?? "—"} · Ubuntu</div>
    <p className="about-tagline">Your intelligent desktop companion.</p>
    <p>Veronica brings Edith's focused workspace, agent insights, music, calendar, machines, clipboard tools and notch experience to GNOME with Linux-native integrations.</p>
    <button className="button" onClick={() => void ipc.openExternal("https://github.com/namannn04/veronica")}>View source</button>
    <div className="about-foot">Built for Ubuntu · Powered by Rust, React and GNOME</div>
  </div>;
}
