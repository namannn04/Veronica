import {
  Badge,
  Button,
  FormGroup,
  FormRow,
  Notice,
  Page,
  PageHeader,
  Segmented,
  Stack,
  Tabs,
  type Tone,
} from "@veronica/ui";
import { Check } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { ThemePicker } from "../components/ThemePicker";
import { countdown } from "../lib/format";
import { invoke, ipc, isDesktopApp, listenEvent } from "../lib/ipc";
import { chooseProfile, chosenProfile, type Profile, storedStarters } from "../lib/onboarding";
import { appearanceOf, applyAppearance } from "../lib/preferences";
import type {
  Diagnostics,
  LaunchAtLoginStatus,
  PowerStatus,
  ShellExtensionStatus,
} from "../lib/types";
import { BackupPane } from "./BackupPane";
import { DiagnosticsPage } from "./DiagnosticsPage";
import { PresenterPane } from "./PresenterPane";
import { ShortcutsPane } from "./ShortcutsPane";
import { ChoiceRow, SwitchRow } from "./settingsRows";
import { UpdatePane } from "./UpdatePane";
import "./Settings.css";

type Tab =
  | "general"
  | "presenter"
  | "desktop"
  | "permissions"
  | "shortcuts"
  | "terminal"
  | "backup"
  | "updates";

const TABS: { id: Tab; label: string; subtitle: string }[] = [
  { id: "general", label: "General", subtitle: "Appearance, startup and power" },
  {
    id: "presenter",
    label: "Presenter",
    subtitle: "Blur sensitive figures while you share your screen",
  },
  {
    id: "desktop",
    label: "Desktop",
    subtitle: "Keystroke highlight, emoji and color picking",
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

const SAVED_VISIBLE_MS = 1800;

type Feedback = { tone: "neutral" | "critical"; text: string; load?: boolean } | null;

const TABS_USING_VALUES: Tab[] = ["general", "desktop"];

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
    if (key === "appearance") {
      applyAppearance(value);
    }
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
        if (key === "appearance") {
          applyAppearance(stored.appearance);
        }
      } catch {
        return;
      }
    }
  };

  const switchProfile = async (profile: Profile) => {
    setValues((current) => ({ ...current, profile }));
    try {
      const added = await chooseProfile(invoke, profile, storedStarters(values));
      confirm(
        added.length === 0
          ? "Saved"
          : `Installing ${added.length} starter extension${added.length === 1 ? "" : "s"}`,
      );
    } catch (reason) {
      fail(reason);
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
      if (status.state === "active") {
        confirm("Extension enabled");
      } else {
        setFeedback({ tone: "neutral", text: status.detail });
      }
    } catch (reason) {
      fail(reason);
    }
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
            title="Profile"
            description="Switching adds the other profile's starter extensions and removes nothing."
          >
            <FormRow
              label="How you use Veronica"
              description="Progress shows on Home. Manage everything in the Library."
              control={
                <Segmented
                  label="Profile"
                  options={[
                    { value: "simple", label: "Simple" },
                    { value: "developer", label: "Developer" },
                  ]}
                  value={chosenProfile(values) ?? "simple"}
                  onChange={(profile) => void switchProfile(profile)}
                />
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

      {tab === "presenter" && <PresenterPane />}

      {tab === "desktop" && (
        <Stack>
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
            description="The Emoji extension opens the picker with Ctrl+Alt+E. Copying always works; typing into the app you were in needs the shell extension."
          >
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
            description="Sampling and swatch history live on the Color Picker extension's page."
          >
            <SwitchRow
              label="Color picker"
              description="Adds Pick Color to the top bar."
              checked={Boolean(values.colorPickerEnabled)}
              onChange={(value) => void set("colorPickerEnabled", value)}
            />
            <ChoiceRow
              label="History size"
              description="How many swatches to keep."
              value={Number(values["ext.color.historySize"] ?? 100)}
              options={[10, 25, 50, 100].map((count) => ({
                value: count,
                label: `${count} swatches`,
              }))}
              format={(count) => `${count} swatches`}
              onChange={(count) => void set("ext.color.historySize", count)}
            />
          </FormGroup>
        </Stack>
      )}

      {tab === "permissions" && <DiagnosticsPage diagnostics={diagnostics} />}

      {tab === "shortcuts" && (
        <ShortcutsPane
          footer={
            <FormRow
              label="Desktop backend"
              description={
                diagnostics?.session.hasGlobalShortcutsPortal
                  ? "GNOME portal detected. The X11-compatible backend is also available."
                  : "Veronica uses its XWayland global-hotkey backend in this GNOME session."
              }
              control={<Badge tone="good">Active</Badge>}
            />
          }
        />
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
  if (remainingSecs === null || remainingSecs === undefined) {
    return "";
  }
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
