import { useEffect, useState, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";

import { ProviderSelector } from "../components/ProviderSelector";
import { ipc } from "../lib/ipc";
import {
  APPEARANCES,
  applyAppearance,
  limitProviderOf,
  storedLimitProvider,
  type LimitProvider,
} from "../lib/preferences";
import type { Diagnostics, PowerStatus, UpdateInfo } from "../lib/types";
import { AlertsPane } from "./AlertsPane";
import { BackupPane } from "./BackupPane";
import { PresenterPane } from "./PresenterPane";
import { DiagnosticsPage } from "./DiagnosticsPage";

type Tab = "general" | "alerts" | "presenter" | "desktop" | "permissions" | "shortcuts" | "terminal" | "backup" | "updates";
const TABS: { id: Tab; label: string }[] = [
  { id: "general", label: "General" }, { id: "alerts", label: "Alerts" },
  { id: "presenter", label: "Presenter" },
  { id: "desktop", label: "Desktop" },
  { id: "permissions", label: "Permissions" },
  { id: "shortcuts", label: "Shortcuts" }, { id: "terminal", label: "Terminal" },
  { id: "backup", label: "Backup" }, { id: "updates", label: "Updates" },
];

export function SettingsPage({ diagnostics }: { diagnostics: Diagnostics | null }) {
  const [tab, setTab] = useState<Tab>("general");
  const [values, setValues] = useState<Record<string, unknown>>({});
  const [notice, setNotice] = useState("");
  const [power, setPower] = useState<PowerStatus | null>(null);

  useEffect(() => {
    const refresh = () => void ipc.settingsAll().then(setValues).catch(() => setNotice("Settings connect when Veronica runs as the desktop app."));
    refresh();
    void ipc.powerStatus().then(setPower).catch(() => {});
    const changed = listen("settings-updated", refresh);
    return () => { void changed.then((unlisten) => unlisten()); };
  }, []);

  const set = async (key: string, value: unknown) => {
    setValues((current) => ({ ...current, [key]: value }));
    if (key === "appearance") applyAppearance(value);
    try { await ipc.settingsSet(key, value); setNotice("Saved"); } catch { setNotice("Preview only — open the installed desktop app to save."); }
  };

  return <>
    <div className="page-head"><div><h1>Settings</h1><div className="page-sub">Make Veronica work the way you do</div></div>{notice && <span className="save-note">{notice}</span>}</div>
    <div className="settings-tabs" role="tablist">{TABS.map((item) => <button key={item.id} role="tab" aria-selected={tab === item.id} onClick={() => setTab(item.id)}>{item.label}</button>)}</div>
    {tab === "general" && <div className="settings-stack">
      <SettingsGroup title="Appearance" subtitle="One shared theme for the Veronica app and the GNOME notch.">
        <div className="theme-picker">{APPEARANCES.map((theme) => <button key={theme.id} aria-pressed={(values.appearance ?? "system") === theme.id} onClick={() => void set("appearance", theme.id)}><i className={`theme-preview ${theme.id}`} /><strong>{theme.label}</strong><small>{theme.detail}</small></button>)}</div>
      </SettingsGroup>
      <SettingsGroup title="Rate limits" subtitle="Choose which provider the app and notch show by default.">
        <div className="setting-row"><div><strong>Visible provider</strong><small>Switch it here, on the Usage page, or directly inside the notch.</small></div><ProviderSelector value={limitProviderOf(values.limitsProvider)} onChange={(provider: LimitProvider) => void set("limitsProvider", storedLimitProvider(provider))} /></div>
      </SettingsGroup>
      <SettingsGroup title="Ubuntu top bar" subtitle="Keep GNOME's Wi-Fi, Bluetooth, sound and battery controls in their original Quick Settings menu.">
        <StatusRow label="Compact Edith notch" detail="Enabled while the Veronica GNOME extension is active." />
        <Toggle label="CPU and memory indicator" detail="Show compact live system usage beside the clock." checked={Boolean(values.menuBarSystemStats)} onChange={(value) => void set("menuBarSystemStats", value)} />
      </SettingsGroup>
      <SettingsGroup title="Power" subtitle="Linux-native systemd/logind inhibitors.">
        <Toggle label="Keep Awake" detail={power?.preventSleepActive ? "Active — systemd-logind is holding the idle inhibitor." : "Prevent automatic sleep while enabled."} checked={Boolean(values.preventSleep)} onChange={(value) => void set("preventSleep", value).then(() => ipc.powerStatus().then(setPower))} />
        <Toggle label="Lid Awake" detail={power && !power.hasLid ? "No laptop lid was detected on this computer." : power?.lidAwakeActive ? "Active — lid-close and idle sleep are both inhibited." : "Keep the session awake when a laptop lid closes."} checked={Boolean(values.lidAwakeEnabled)} onChange={(value) => void set("lidAwakeEnabled", value).then(() => ipc.powerStatus().then(setPower))} />
      </SettingsGroup>
    </div>}
    {tab === "alerts" && <div className="settings-embedded"><AlertsPane /></div>}
    {tab === "presenter" && <div className="settings-embedded"><PresenterPane /></div>}
    {tab === "desktop" && <div className="settings-stack">
      <SettingsGroup title="Focus Dim" subtitle="Darkens everything behind the window you are working in. Drawn by Veronica's GNOME Shell extension, because only the compositor can place a dim behind another app's window.">
        <Toggle label="Dim behind the focused window" detail={diagnostics?.session.isGnome ? "Takes effect immediately." : "This desktop is not GNOME, so nothing draws the overlay."} checked={Boolean(values.focusDimEnabled)} onChange={(value) => void set("focusDimEnabled", value)} />
        <Choice label="Intensity" detail="How dark to go. Capped below opaque, so the desktop is never hidden completely." value={Math.round(Number(values.focusDimIntensity ?? 0.45) * 100)} options={[15, 30, 45, 60, 75, 90].map((percent) => ({ value: percent, label: `${percent}%` }))} onChange={(percent) => void set("focusDimIntensity", percent / 100)} />
        <Choice label="Fade" detail="How long the change takes as focus moves." value={Number(values.focusDimAnimationDuration ?? 0.25)} options={[{ value: 0.05, label: "Instant" }, { value: 0.15, label: "Quick" }, { value: 0.25, label: "Normal" }, { value: 0.5, label: "Slow" }]} onChange={(seconds) => void set("focusDimAnimationDuration", seconds)} />
        <Modes value={String(values.focusDimOtherDisplaysMode ?? "perScreenFront")} onChange={(mode) => void set("focusDimOtherDisplaysMode", mode)} />
      </SettingsGroup>
      <SettingsGroup title="Color Picker" subtitle="Sampling and the swatch history live on the Color Picker page; these are the switches behind them.">
        <Toggle label="Color Picker" detail="Adds the page and the top bar's Pick Color action." checked={Boolean(values.colorPickerEnabled)} onChange={(value) => void set("colorPickerEnabled", value)} />
        <Choice label="History size" detail="How many swatches to keep." value={Number(values.colorPickerHistorySize ?? 100)} options={[10, 25, 50, 100].map((count) => ({ value: count, label: String(count) }))} onChange={(count) => void set("colorPickerHistorySize", count)} />
      </SettingsGroup>
    </div>}
    {tab === "permissions" && <div className="settings-embedded"><DiagnosticsPage diagnostics={diagnostics} /></div>}
    {tab === "shortcuts" && <section className="settings-group"><div><h2>Global shortcuts</h2><p>Handled by the desktop shortcut service; Veronica never records arbitrary keys.</p></div><div className="settings-box"><ShortcutRow title="Show Veronica" detail="Open, unminimize and focus the app." keys="Ctrl + Alt + V"/><ShortcutRow title="Clipboard" detail="Open the notch clipboard, or the app as fallback." keys="Ctrl + Alt + B"/><ShortcutRow title="Microphone mute" detail="Toggle the default PipeWire microphone system-wide." keys="Ctrl + Alt + M"/><ShortcutRow title="Pick color" detail="Open GNOME's eyedropper and save the swatch." keys="Ctrl + Alt + P"/><ShortcutRow title="Clean keys" detail="Take a compositor modal grab until you click Done." keys="Ctrl + Alt + K"/><div className="setting-row"><div><strong>Desktop backend</strong><small>{diagnostics?.session.hasGlobalShortcutsPortal ? "GNOME portal detected; X11-compatible backend is also available." : "Veronica uses its XWayland global-hotkey backend in this GNOME session."}</small></div><span className="pill good">Active</span></div></div></section>}
    {tab === "terminal" && <Info title="Command line" body="The vr command uses the same Veronica data and settings as this app." code={`vr diagnose\nvr config list\nvr usage dashboard\nvr machines probe\nvr clipboard list`} />}
    {tab === "backup" && <div className="settings-embedded"><BackupPane /></div>}
    {tab === "updates" && <UpdatePane current={diagnostics?.version ?? ""} />}
  </>;
}

function SettingsGroup({ title, subtitle, children }: { title: string; subtitle: string; children: ReactNode }) { return <section className="settings-group"><div><h2>{title}</h2><p>{subtitle}</p></div><div className="settings-box">{children}</div></section>; }
function Toggle({ label, detail, checked, onChange }: { label: string; detail: string; checked: boolean; onChange: (value: boolean) => void }) { return <div className="setting-row"><div><strong>{label}</strong><small>{detail}</small></div><button className="switch" role="switch" aria-checked={checked} onClick={() => onChange(!checked)}><span className="knob" /></button></div>; }
function StatusRow({ label, detail }: { label: string; detail: string }) { return <div className="setting-row"><div><strong>{label}</strong><small>{detail}</small></div><span className="pill good">Active</span></div>; }
function Choice({ label, detail, value, options, onChange }: { label: string; detail: string; value: number; options: { value: number; label: string }[]; onChange: (value: number) => void }) {
  return <div className="setting-row"><div><strong>{label}</strong><small>{detail}</small></div><div className="segmented">{options.map((option) => <button key={option.value} aria-pressed={value === option.value} onClick={() => onChange(option.value)}>{option.label}</button>)}</div></div>;
}

/** Matches `DisplayMode` in the core; the keys are what the extension reads. */
const DISPLAY_MODES = [
  { id: "perScreenFront", label: "Per display", detail: "Each display keeps its own front window bright." },
  { id: "dimUnfocused", label: "Focused only", detail: "Every display but the focused window dims." },
];

function Modes({ value, onChange }: { value: string; onChange: (mode: string) => void }) {
  const active = DISPLAY_MODES.find((mode) => mode.id === value) ?? DISPLAY_MODES[0];
  return <div className="setting-row"><div><strong>Other displays</strong><small>{active.detail}</small></div><div className="segmented">{DISPLAY_MODES.map((mode) => <button key={mode.id} aria-pressed={value === mode.id} onClick={() => onChange(mode.id)}>{mode.label}</button>)}</div></div>;
}

function Info({ title, body, code }: { title: string; body: string; code?: string }) { return <section className="settings-info"><div className="info-glyph">V</div><h2>{title}</h2><p>{body}</p>{code && <pre>{code}</pre>}</section>; }
function ShortcutRow({ title, detail, keys }: { title: string; detail: string; keys: string }) { return <div className="setting-row"><div><strong>{title}</strong><small>{detail}</small></div><kbd>{keys}</kbd></div>; }

function UpdatePane({ current }: { current: string }) {
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const check = async () => {
    setBusy(true); setError("");
    try { setUpdate(await ipc.updateCheck()); } catch (reason) { setError(String(reason)); }
    finally { setBusy(false); }
  };
  return <section className="settings-group"><div><h2>Software updates</h2><p>Checks Veronica's official GitHub releases. Nothing downloads or installs without your click.</p></div><div className="settings-box"><div className="setting-row"><div><strong>Installed version</strong><small>{update ? update.updateAvailable ? `Version ${update.latestVersion} is available.` : "You are up to date." : "Run a check when you want; no background tracking."}</small></div><span className={`pill ${update?.updateAvailable ? "warning" : "good"}`}>v{current}</span></div>{error && <div className="banner error">{error}</div>}<div className="setting-row"><div><strong>{update?.updateAvailable ? `Download Veronica ${update.latestVersion}` : "Check GitHub releases"}</strong><small>{update?.publishedAt ? `Published ${new Date(update.publishedAt).toLocaleDateString()}` : "Uses a 12-second timeout and reports network errors."}</small></div>{update?.updateAvailable ? <button className="button primary" onClick={() => void ipc.openExternal(update.packageUrl ?? update.releaseUrl)}>Download update</button> : <button className="button" disabled={busy} onClick={() => void check()}>{busy ? "Checking…" : "Check now"}</button>}</div>{update?.updateAvailable && <div className="setting-row"><div><strong>Install after downloading</strong><small>Your settings and history remain in your home directory.</small></div><code>sudo apt install --reinstall ./Veronica_{update.latestVersion}_amd64.deb</code></div>}</div></section>;
}
