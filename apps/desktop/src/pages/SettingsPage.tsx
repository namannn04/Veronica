import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { Check } from "lucide-react";

import { ProviderSelector } from "../components/ProviderSelector";
import { ThemePicker } from "../components/ThemePicker";
import { countdown } from "../lib/format";
import { ipc, isDesktopApp, listenEvent } from "../lib/ipc";
import {
  appearanceOf,
  applyAppearance,
  limitProviderOf,
  storedLimitProvider,
  type LimitProvider,
} from "../lib/preferences";
import type {
  Diagnostics,
  ExtensionReport,
  LaunchAtLoginStatus,
  PowerStatus,
  ShellExtensionStatus,
} from "../lib/types";
import {
  Badge,
  Button,
  FormGroup,
  FormRow,
  Kbd,
  Notice,
  Page,
  PageHeader,
  Segmented,
  Select,
  Stack,
  Tabs,
  type Tone,
} from "../ui";
import { AlertsPane } from "./AlertsPane";
import { BackupPane } from "./BackupPane";
import { DiagnosticsPage } from "./DiagnosticsPage";
import { PresenterPane } from "./PresenterPane";
import { ChoiceRow, SwitchRow } from "./settingsRows";
import { UpdatePane } from "./UpdatePane";
import "./Settings.css";

type Tab =
  | "general"
  | "alerts"
  | "presenter"
  | "desktop"
  | "permissions"
  | "shortcuts"
  | "terminal"
  | "backup"
  | "updates";

const TABS: { id: Tab; label: string; subtitle: string }[] = [
  { id: "general", label: "General", subtitle: "Appearance, usage collection, startup and power" },
  { id: "alerts", label: "Alerts", subtitle: "Rate-limit alerts, pacing and reset reminders" },
  {
    id: "presenter",
    label: "Presenter",
    subtitle: "Blur sensitive figures while you share your screen",
  },
  {
    id: "desktop",
    label: "Desktop",
    subtitle: "Focus dim, keystroke highlight, emoji and color picking",
  },
  {
    id: "permissions",
    label: "Permissions",
    subtitle: "What this session can do and where Veronica keeps its files",
  },
  { id: "shortcuts", label: "Shortcuts", subtitle: "Global shortcuts handled by the desktop" },
  { id: "terminal", label: "Terminal", subtitle: "The vr command line" },
  { id: "backup", label: "Backup", subtitle: "Export, inspect and restore a local backup" },
  { id: "updates", label: "Updates", subtitle: "Check for and install new versions" },
];

const USAGE_REFRESH_CHOICES: [number, string][] = [
  [5, "Every 5 minutes"],
  [15, "Every 15 minutes"],
  [30, "Every 30 minutes"],
  [60, "Every hour"],
  [0, "Only when I press Refresh"],
];

const SAVED_VISIBLE_MS = 1800;

function usageRefreshMinutes(value: unknown): number {
  const minutes = typeof value === "number" ? value : 15;
  return USAGE_REFRESH_CHOICES.some(([choice]) => choice === minutes) ? minutes : 15;
}

type Feedback = { tone: "neutral" | "critical"; text: string; load?: boolean } | null;

const TABS_USING_VALUES: Tab[] = ["general", "desktop", "shortcuts"];

export function SettingsPage({ diagnostics }: { diagnostics: Diagnostics | null }) {
  const [tab, setTab] = useState<Tab>("general");
  const [values, setValues] = useState<Record<string, unknown>>({});
  const [feedback, setFeedback] = useState<Feedback>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const [savedVisible, setSavedVisible] = useState(false);
  const savedTimer = useRef<number | undefined>(undefined);
  const [power, setPower] = useState<PowerStatus | null>(null);
  const [launchAtLogin, setLaunchAtLogin] = useState<LaunchAtLoginStatus | null>(null);
  const [shellExtension, setShellExtension] = useState<ShellExtensionStatus | null>(null);

  const refresh = useCallback(
    () =>
      void ipc
        .settingsAll()
        .then((stored) => {
          setValues(stored);
          setFeedback((current) => (current?.load ? null : current));
        })
        .catch((reason) =>
          setFeedback(
            isDesktopApp()
              ? { tone: "critical", text: String(reason), load: true }
              : {
                  tone: "neutral",
                  text: "Settings connect when Veronica runs as the desktop app.",
                  load: true,
                },
          ),
        ),
    [],
  );

  useEffect(() => {
    refresh();
    void ipc
      .powerStatus()
      .then(setPower)
      .catch(() => {});
    void ipc
      .launchAtLoginStatus()
      .then(setLaunchAtLogin)
      .catch(() => {});
    const refreshShell = () =>
      void ipc
        .shellExtensionStatus()
        .then(setShellExtension)
        .catch(() => {});
    refreshShell();
    const changed = listenEvent("settings-updated", refresh);
    const session = listenEvent("session-resolved", refreshShell);
    return () => {
      void changed.then((unlisten) => unlisten());
      void session.then((unlisten) => unlisten());
    };
  }, [refresh]);

  useEffect(() => () => window.clearTimeout(savedTimer.current), []);

  const confirm = (text: string) => {
    setFeedback(null);
    setSaved(text);
    setSavedVisible(true);
    window.clearTimeout(savedTimer.current);
    savedTimer.current = window.setTimeout(() => setSavedVisible(false), SAVED_VISIBLE_MS);
  };

  const fail = (reason: unknown) => {
    setSavedVisible(false);
    setFeedback({ tone: "critical", text: String(reason) });
  };

  const set = async (key: string, value: unknown) => {
    setValues((current) => ({ ...current, [key]: value }));
    if (key === "appearance") applyAppearance(value);
    try {
      await ipc.settingsSet(key, value);
      confirm("Saved");
    } catch (reason) {
      if (!isDesktopApp()) {
        setFeedback({
          tone: "neutral",
          text: "Preview only. Open the installed desktop app to save.",
        });
        return;
      }
      fail(reason);
      try {
        const stored = await ipc.settingsAll();
        setValues(stored);
        if (key === "appearance") applyAppearance(stored.appearance);
      } catch {
        return;
      }
    }
  };

  const setStartup = async (enabled: boolean) => {
    try {
      setLaunchAtLogin(await ipc.launchAtLoginSet(enabled));
      confirm(enabled ? "Starts after login" : "Launch at login off");
    } catch (reason) {
      fail(reason);
    }
  };

  const enableShellExtension = async () => {
    try {
      const status = await ipc.shellExtensionEnable();
      setShellExtension(status);
      if (status.state === "active") confirm("Extension enabled");
      else setFeedback({ tone: "neutral", text: status.detail });
    } catch (reason) {
      fail(reason);
    }
  };

  const feature = (id: string): ExtensionReport | undefined => {
    const entry = diagnostics?.extensions.find((candidate) => candidate.id === id);
    if (!entry) return undefined;
    const stored = values[entry.defaultsKey];
    return typeof stored === "boolean" ? { ...entry, enabled: stored } : entry;
  };

  const current = TABS.find((item) => item.id === tab) ?? TABS[0];
  const gnome = Boolean(diagnostics?.session.isGnome);

  return (
    <Page
      width="form"
      className="settings-page"
      header={
        <PageHeader
          title="Settings"
          subtitle={current.subtitle}
          actions={
            <span
              className="settings-saved"
              role="status"
              aria-live="polite"
              data-visible={savedVisible || undefined}
            >
              {saved && <Check aria-hidden="true" />}
              {savedVisible ? saved : ""}
            </span>
          }
          accessory={<Tabs label="Settings sections" items={TABS} value={tab} onChange={setTab} />}
        />
      }
    >
      {feedback && (!feedback.load || TABS_USING_VALUES.includes(tab)) && (
        <Notice
          tone={feedback.tone}
          title={
            feedback.load && feedback.tone === "critical" ? "Settings did not load" : undefined
          }
          actions={
            feedback.load && feedback.tone === "critical" ? (
              <Button size="small" onClick={refresh}>
                Retry
              </Button>
            ) : (
              <Button size="small" variant="plain" onClick={() => setFeedback(null)}>
                Dismiss
              </Button>
            )
          }
        >
          {feedback.text}
        </Notice>
      )}

      {tab === "general" && (
        <Stack>
          <FormGroup
            title="Appearance"
            description="One shared theme for the app and the GNOME notch."
          >
            <div className="settings-theme">
              <ThemePicker
                value={appearanceOf(values.appearance)}
                onChange={(appearance) => void set("appearance", appearance)}
              />
            </div>
          </FormGroup>

          <FormGroup
            title="Rate limits"
            description="Which provider the app and the notch show by default."
          >
            <FormRow
              label="Visible provider"
              description="Switch it here, on the Usage page, or inside the notch."
              control={
                <ProviderSelector
                  value={limitProviderOf(values.limitsProvider)}
                  onChange={(provider: LimitProvider) =>
                    void set("limitsProvider", storedLimitProvider(provider))
                  }
                />
              }
            />
            <FormRow
              label="Collect usage"
              description="How often the app reads your agents' history for Home, Agent Usage and the notch."
              control={
                <Select
                  aria-label="Collect usage"
                  value={String(usageRefreshMinutes(values.usageRefreshMinutes))}
                  onChange={(event) => void set("usageRefreshMinutes", Number(event.target.value))}
                >
                  {USAGE_REFRESH_CHOICES.map(([minutes, label]) => (
                    <option key={minutes} value={minutes}>
                      {label}
                    </option>
                  ))}
                </Select>
              }
            />
          </FormGroup>

          <FormGroup
            title="Startup"
            description="Keep alerts, global shortcuts and tray actions available after a reboot."
          >
            <SwitchRow
              label="Launch at login"
              description={
                launchAtLogin?.enabled
                  ? "Starts quietly in the background. Open it from the tray or a shortcut."
                  : "Veronica will not run until you open it yourself."
              }
              checked={Boolean(launchAtLogin?.enabled)}
              onChange={(enabled) => void setStartup(enabled)}
            />
          </FormGroup>

          <FormGroup
            title="Top bar"
            description="GNOME's Wi-Fi, Bluetooth, sound and battery controls stay in their Quick Settings menu."
          >
            <ShellExtensionRow
              status={shellExtension}
              onEnable={() => void enableShellExtension()}
            />
            <SwitchRow
              label="CPU and memory indicator"
              description="Show compact live system usage beside the clock."
              checked={Boolean(values.menuBarSystemStats)}
              onChange={(value) => void set("menuBarSystemStats", value)}
            />
          </FormGroup>

          <FormGroup
            title="Power"
            description={
              <>
                Uses systemd-logind inhibitors. A timed session started with{" "}
                <code className="ui-mono">vr power lid-awake on --for 30m</code> counts down here
                and ends itself.
              </>
            }
          >
            <SwitchRow
              label="Keep awake"
              description={
                power?.preventSleepActive
                  ? `Active. systemd-logind is holding the idle inhibitor${sessionEnds(power.preventSleepRemainingSecs)}.`
                  : "Prevent automatic sleep while this is on."
              }
              checked={Boolean(values.preventSleep)}
              onChange={(value) =>
                void set("preventSleep", value).then(() => ipc.powerStatus().then(setPower))
              }
            />
            <SwitchRow
              label="Lid awake"
              description={
                power && !power.hasLid
                  ? "No laptop lid was detected on this computer."
                  : power?.lidAwakeActive
                    ? `Active. Lid-close and idle sleep are both inhibited${sessionEnds(power.lidAwakeRemainingSecs)}.`
                    : "Keep the session awake when a laptop lid closes."
              }
              checked={Boolean(values.lidAwakeEnabled)}
              onChange={(value) =>
                void set("lidAwakeEnabled", value).then(() => ipc.powerStatus().then(setPower))
              }
            />
          </FormGroup>
        </Stack>
      )}

      {tab === "alerts" && <AlertsPane />}
      {tab === "presenter" && <PresenterPane />}

      {tab === "desktop" && (
        <Stack>
          <FormGroup
            title="Focus dim"
            description="Darkens everything behind the window you are working in. Drawn by the GNOME Shell extension, since only the compositor can place a dim behind another app's window."
          >
            <SwitchRow
              label="Dim behind the focused window"
              description={
                gnome
                  ? "Takes effect immediately."
                  : "This desktop is not GNOME, so nothing draws the overlay."
              }
              checked={Boolean(values.focusDimEnabled)}
              onChange={(value) => void set("focusDimEnabled", value)}
            />
            <ChoiceRow
              label="Intensity"
              description="How dark to go. Capped below opaque, so the desktop never disappears."
              value={Math.round(Number(values.focusDimIntensity ?? 0.45) * 100)}
              options={[15, 30, 45, 60, 75, 90].map((percent) => ({
                value: percent,
                label: `${percent}%`,
              }))}
              format={(percent) => `${percent}%`}
              onChange={(percent) => void set("focusDimIntensity", percent / 100)}
            />
            <ChoiceRow
              label="Fade"
              description="How long the change takes as focus moves."
              value={Number(values.focusDimAnimationDuration ?? 0.25)}
              options={[
                { value: 0.05, label: "Instant" },
                { value: 0.15, label: "Quick" },
                { value: 0.25, label: "Normal" },
                { value: 0.5, label: "Slow" },
              ]}
              format={(seconds) => `${seconds}s`}
              onChange={(seconds) => void set("focusDimAnimationDuration", seconds)}
            />
            <ModeRow
              label="Other displays"
              choices={DISPLAY_MODES}
              value={String(values.focusDimOtherDisplaysMode ?? "perScreenFront")}
              onChange={(mode) => void set("focusDimOtherDisplaysMode", mode)}
            />
          </FormGroup>

          <FormGroup
            title="Keystroke highlight"
            description="Shows a keycap for every key press, for demos and recordings. The GNOME Shell extension reads and draws them, never consumes a press, and records nothing while the lock screen or a password prompt is up."
          >
            <SwitchRow
              label="Keystroke highlight"
              description={
                gnome
                  ? "Adds the feature and its controls."
                  : "This desktop is not GNOME, so nothing reads the presses."
              }
              checked={Boolean(values.keystrokeHighlightEnabled)}
              onChange={(value) => void set("keystrokeHighlightEnabled", value)}
            />
            <SwitchRow
              label="Show key presses now"
              description="Pausing hides the keycaps without turning the feature off."
              checked={Boolean(values.keystrokeHighlightActive)}
              onChange={(value) => void set("keystrokeHighlightActive", value)}
            />
            <ChoiceRow
              label="Duration"
              description="How long one keycap stays on screen."
              value={Number(values.keystrokeHighlightDuration ?? 1.5)}
              options={[0.5, 1, 1.5, 2, 3].map((seconds) => ({
                value: seconds,
                label: `${seconds}s`,
              }))}
              format={(seconds) => `${seconds}s`}
              onChange={(seconds) => void set("keystrokeHighlightDuration", seconds)}
            />
            <ModeRow
              label="Position"
              choices={KEYSTROKE_POSITIONS}
              value={String(values.keystrokeHighlightPosition ?? "bottom")}
              fallback={1}
              onChange={(position) => void set("keystrokeHighlightPosition", position)}
            />
          </FormGroup>

          <FormGroup
            title="Emoji picker"
            description="Search and insert emoji with Ctrl+Alt+E. Copying always works; typing into the app you were in needs the shell extension."
          >
            <SwitchRow
              label="Emoji picker"
              description="Adds the page and its shortcut."
              checked={Boolean(values.emojiEnabled)}
              onChange={(value) => void set("emojiEnabled", value)}
            />
            <SwitchRow
              label="Type into the app I was in"
              description={
                gnome
                  ? "After copying, the picker sends one Ctrl+V through the compositor. Off, it only copies."
                  : "This desktop is not GNOME, so the picker only copies."
              }
              checked={values.emojiInsertInPlace !== false}
              onChange={(value) => void set("emojiInsertInPlace", value)}
            />
          </FormGroup>

          <FormGroup
            title="Color picker"
            description="Sampling and swatch history live on the Color Picker page."
          >
            <SwitchRow
              label="Color picker"
              description="Adds the page and the top bar's Pick Color action."
              checked={Boolean(values.colorPickerEnabled)}
              onChange={(value) => void set("colorPickerEnabled", value)}
            />
            <ChoiceRow
              label="History size"
              description="How many swatches to keep."
              value={Number(values.colorPickerHistorySize ?? 100)}
              options={[10, 25, 50, 100].map((count) => ({
                value: count,
                label: `${count} swatches`,
              }))}
              format={(count) => `${count} swatches`}
              onChange={(count) => void set("colorPickerHistorySize", count)}
            />
          </FormGroup>
        </Stack>
      )}

      {tab === "permissions" && <DiagnosticsPage diagnostics={diagnostics} />}

      {tab === "shortcuts" && (
        <FormGroup
          title="Global shortcuts"
          description="Handled by the desktop shortcut service. Veronica never records arbitrary keys."
        >
          <ShortcutRow
            title="Show Veronica"
            detail="Open, unminimise and focus the app."
            keys={["Ctrl", "Alt", "V"]}
          />
          <ShortcutRow
            feature={feature("clipboard")}
            title="Clipboard"
            detail="Open the notch clipboard, or the app as a fallback."
            keys={["Ctrl", "Alt", "B"]}
          />
          <ShortcutRow
            feature={feature("micMute")}
            title="Microphone mute"
            detail="Toggle the default PipeWire microphone system-wide."
            keys={["Ctrl", "Alt", "M"]}
          />
          <ShortcutRow
            feature={feature("colorPicker")}
            title="Pick color"
            detail="Open GNOME's eyedropper and save the swatch."
            keys={["Ctrl", "Alt", "P"]}
          />
          <ShortcutRow
            title="Clean keys"
            detail="Take a compositor grab until you click Done."
            keys={["Ctrl", "Alt", "K"]}
          />
          <ShortcutRow
            feature={feature("emoji")}
            title="Emoji"
            detail="Open the picker. Clicking an emoji copies it."
            keys={["Ctrl", "Alt", "E"]}
          />
          <FormRow
            label="Desktop backend"
            description={
              diagnostics?.session.hasGlobalShortcutsPortal
                ? "GNOME portal detected. The X11-compatible backend is also available."
                : "Veronica uses its XWayland global-hotkey backend in this GNOME session."
            }
            control={<Badge tone="good">Active</Badge>}
          />
        </FormGroup>
      )}

      {tab === "terminal" && (
        <FormGroup
          title="Command line"
          description="The vr command reads and writes the same data and settings as this app."
        >
          <pre className="settings-code ui-mono ui-selectable">{`vr diagnose\nvr config list\nvr usage summary\nvr machines list\nvr clipboard list`}</pre>
        </FormGroup>
      )}

      {tab === "backup" && <BackupPane />}
      {tab === "updates" && <UpdatePane current={diagnostics?.version ?? ""} />}
    </Page>
  );
}

function sessionEnds(remainingSecs: number | null | undefined) {
  if (remainingSecs === null || remainingSecs === undefined) return "";
  return `, for another ${countdown(remainingSecs)}`;
}

const SHELL_STATES: Record<ShellExtensionStatus["state"], { label: string; tone: Tone }> = {
  active: { label: "Active", tone: "good" },
  disabled: { label: "Disabled", tone: "warning" },
  restartRequired: { label: "Log out once", tone: "warning" },
  notInstalled: { label: "Not installed", tone: "warning" },
  unsupported: { label: "Needs GNOME", tone: "warning" },
  error: { label: "Error", tone: "critical" },
};

function ShellExtensionRow({
  status,
  onEnable,
}: {
  status: ShellExtensionStatus | null;
  onEnable: () => void;
}) {
  const state = status ? SHELL_STATES[status.state] : undefined;
  return (
    <FormRow
      label="Top bar notch"
      description={status?.detail ?? "Checking the GNOME extension…"}
      control={
        status?.canEnable ? (
          <Button size="small" onClick={onEnable}>
            Enable
          </Button>
        ) : (
          <Badge tone={state?.tone ?? "neutral"}>{state?.label ?? "Checking…"}</Badge>
        )
      }
    />
  );
}

type ModeChoice = { id: string; label: string; detail: string };

const DISPLAY_MODES: ModeChoice[] = [
  {
    id: "perScreenFront",
    label: "Per display",
    detail: "Each display keeps its own front window bright.",
  },
  {
    id: "dimUnfocused",
    label: "Focused only",
    detail: "Every display but the focused window dims.",
  },
];

const KEYSTROKE_POSITIONS: ModeChoice[] = [
  { id: "top", label: "Top", detail: "The row sits below the top bar." },
  { id: "bottom", label: "Bottom", detail: "The row sits above the bottom of the work area." },
];

function ModeRow({
  label,
  choices,
  value,
  fallback = 0,
  onChange,
}: {
  label: string;
  choices: ModeChoice[];
  value: string;
  fallback?: number;
  onChange: (value: string) => void;
}) {
  const active = choices.find((choice) => choice.id === value) ?? choices[fallback];
  return (
    <FormRow
      label={label}
      description={active.detail}
      control={
        <Segmented
          label={label}
          size="small"
          value={active.id}
          options={choices.map((choice) => ({ value: choice.id, label: choice.label }))}
          onChange={onChange}
        />
      }
    />
  );
}

function ShortcutRow({
  title,
  detail,
  keys,
  feature,
}: {
  title: string;
  detail: string;
  keys: string[];
  feature?: ExtensionReport;
}) {
  const off = feature !== undefined && !feature.enabled;
  return (
    <FormRow
      label={title}
      description={off ? `Turn on ${feature.title} in Extensions to use this shortcut.` : detail}
      control={off ? <Badge>Off</Badge> : <KeyCombo keys={keys} />}
    />
  );
}

function KeyCombo({ keys }: { keys: string[] }): ReactNode {
  return (
    <span className="settings-keys" aria-label={keys.join(" + ")}>
      {keys.map((key) => (
        <Kbd key={key}>{key}</Kbd>
      ))}
    </span>
  );
}
