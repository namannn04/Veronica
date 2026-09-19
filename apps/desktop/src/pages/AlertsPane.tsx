import { useCallback, useEffect, useState } from "react";

import { ipc, listenEvent } from "../lib/ipc";
import type { AlertsView, WatchedWindow } from "../lib/types";

/**
 * Rate-limit alerts.
 *
 * Every alert is edge-triggered: it fires when a window crosses a level or a
 * pacing zone, and stays quiet while it sits there. That is why the switches are
 * per-crossing rather than per-threshold, and why "Forget current levels" exists
 * — after changing a threshold the old comparison point is meaningless.
 */
const SESSION_OFFSETS = [
  { minutes: 5, label: "5 min" },
  { minutes: 15, label: "15 min" },
  { minutes: 30, label: "30 min" },
  { minutes: 60, label: "1 h" },
];

const WEEKLY_OFFSETS = [
  { minutes: 60, label: "1 h" },
  { minutes: 120, label: "2 h" },
  { minutes: 360, label: "6 h" },
  { minutes: 720, label: "12 h" },
];

const MARGINS = [5, 10, 15, 20, 25];

export function AlertsPane() {
  const [view, setView] = useState<AlertsView | null>(null);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setView(await ipc.alertsView());
      setError(null);
    } catch (reason) {
      setError(String(reason));
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  // A switch flipped from the CLI or the notch reaches this screen too.
  useEffect(() => {
    const changed = listenEvent("settings-updated", () => void load());
    const posted = listenEvent("alerts-posted", () => void load());
    return () => {
      void changed.then((un) => un());
      void posted.then((un) => un());
    };
  }, [load]);

  const set = async (key: string, value: unknown) => {
    setBusy(true);
    try {
      await ipc.settingsSet(key, value);
      await load();
      setError(null);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  };

  const test = async () => {
    try {
      setNotice(await ipc.alertsTest());
      setTimeout(() => setNotice(null), 5000);
    } catch (reason) {
      setError(String(reason));
    }
  };

  const forget = async () => {
    try {
      await ipc.alertsReset();
      setNotice("Levels forgotten — the next poll starts from where you are now.");
      setTimeout(() => setNotice(null), 5000);
      await load();
    } catch (reason) {
      setError(String(reason));
    }
  };

  if (!view) {
    return (
      <section className="settings-info">
        <div className="info-glyph">◔</div>
        <h2>Alerts</h2>
        <p>{error ?? "Reading the alert settings…"}</p>
      </section>
    );
  }

  const s = view.settings;
  const on = s.master;

  return (
    <div className="settings-stack alerts-pane">
      {error && <div className="banner error">{error}</div>}
      {notice && !error && <div className="banner">{notice}</div>}

      <SettingsGroup
        title="Alerts"
        subtitle="Each alert fires once when a window crosses a level or a pacing zone. Staying in the same zone will not page you again."
      >
        <Toggle
          label="Enable alerts"
          detail={
            on
              ? `Claude's limits are re-read every ${view.pollSeconds} s while this is on.`
              : "Off means no banners and no requests to the provider."
          }
          checked={on}
          busy={busy}
          onChange={(value) => void set("notifyMaster", value)}
        />
      </SettingsGroup>

      {on && (
        <>
          <SettingsGroup title="What is being watched" subtitle="The two Claude windows the alerts follow, read live.">
            {view.note && <div className="setting-row"><div><small>{view.note}</small></div></div>}
            {view.session && <WindowRow label="Session (5h)" window={view.session} reminderAt={view.sessionReminderAt} />}
            {view.week && <WindowRow label="Weekly" window={view.week} reminderAt={view.weekReminderAt} />}
            {!view.session && !view.week && !view.note && (
              <div className="setting-row"><div><small>No windows reported yet.</small></div></div>
            )}
          </SettingsGroup>

          <SettingsGroup title="Level crossings" subtitle="One alert when a window's level rises, and one when it falls back to green.">
            <Toggle
              label="Session (5h) alerts"
              detail="Fires once when the session window crosses warn or critical — it will not repeat while you stay in that zone."
              checked={s.trackSession}
              busy={busy}
              onChange={(value) => void set("notifyTrackSession", value)}
            />
            <Toggle
              label="Weekly alerts"
              detail="Same one-shot-per-zone behaviour as the session alerts."
              checked={s.trackWeekly}
              busy={busy}
              onChange={(value) => void set("notifyTrackWeekly", value)}
            />
            <Toggle
              label="Back to green"
              detail="A quiet note when a window resets and you have a fresh slate."
              checked={s.recovery}
              busy={busy}
              onChange={(value) => void set("notifyRecovery", value)}
            />
            <Toggle
              label="Time-aware levels"
              detail="Blend how much time is left into the level, so 60% with minutes to go outranks 80% with a week."
              checked={s.smartColor}
              busy={busy}
              onChange={(value) => void set("smartColor", value)}
            />
            <Choice
              label="Warn at"
              detail="The percentage that counts as the first level, when time-aware levels are off."
              value={s.thresholds.warningPercent}
              options={[40, 50, 60, 70, 80].map((v) => ({ value: v, label: `${v}%` }))}
              busy={busy}
              onChange={(value) => void set("limitsWarnPercent", value)}
            />
            <Choice
              label="Critical at"
              detail="The percentage that counts as the top level."
              value={s.thresholds.criticalPercent}
              options={[70, 80, 85, 90, 95].map((v) => ({ value: v, label: `${v}%` }))}
              busy={busy}
              onChange={(value) => void set("limitsCritPercent", value)}
            />
          </SettingsGroup>

          <SettingsGroup title="Pace" subtitle="A separate signal from the levels above: how far ahead of an even burn you are, whatever the absolute figure.">
            <Choice
              label="Pacing margin"
              detail="How far ahead of a linear burn still counts as on track."
              value={s.pacingMargin}
              options={MARGINS.map((v) => ({ value: v, label: `±${v} pp` }))}
              busy={busy}
              onChange={(value) => void set("limitsPacingMargin", value)}
            />
            <Toggle
              label="Drifting fast"
              detail="A touch faster than ideal — worth an eye, not a panic."
              checked={s.pacingWarning}
              busy={busy}
              onChange={(value) => void set("notifyPacingWarning", value)}
            />
            <Toggle
              label="Burning hot"
              detail="Well ahead of pace."
              checked={s.pacingHot}
              busy={busy}
              onChange={(value) => void set("notifyPacingHot", value)}
            />
          </SettingsGroup>

          <SettingsGroup title="Before a reset" subtitle="A single reminder a fixed time before each window resets.">
            <Toggle
              label="Remind before the session resets"
              detail={
                view.sessionReminderAt
                  ? `Next at ${new Date(view.sessionReminderAt).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })}.`
                  : "Fires once per window."
              }
              checked={s.reminderSession}
              busy={busy}
              onChange={(value) => void set("notifyReminderSession", value)}
            />
            {s.reminderSession && (
              <Choice
                label="Session lead time"
                detail="How long before the reset."
                value={s.reminderSessionOffsetMin}
                options={SESSION_OFFSETS.map((o) => ({ value: o.minutes, label: o.label }))}
                busy={busy}
                onChange={(value) => void set("notifyReminderSessionOffsetMin", value)}
              />
            )}
            <Toggle
              label="Remind before the week resets"
              detail={
                view.weekReminderAt
                  ? `Next at ${new Date(view.weekReminderAt).toLocaleString([], { weekday: "short", hour: "numeric", minute: "2-digit" })}.`
                  : "Fires once per cycle."
              }
              checked={s.reminderWeekly}
              busy={busy}
              onChange={(value) => void set("notifyReminderWeekly", value)}
            />
            {s.reminderWeekly && (
              <Choice
                label="Weekly lead time"
                detail="How long before the reset."
                value={s.reminderWeeklyOffsetMin}
                options={WEEKLY_OFFSETS.map((o) => ({ value: o.minutes, label: o.label }))}
                busy={busy}
                onChange={(value) => void set("notifyReminderWeeklyOffsetMin", value)}
              />
            )}
          </SettingsGroup>

          <SettingsGroup title="Sign-in" subtitle="The one provider failure you can act on.">
            <Toggle
              label="Token expired"
              detail="Tells you when Claude's saved credentials need a fresh login. Debounced to once an hour."
              checked={s.tokenExpired}
              busy={busy}
              onChange={(value) => void set("notifyTokenExpired", value)}
            />
          </SettingsGroup>

          <SettingsGroup title="Checks" subtitle="Confirm banners arrive, and reset the comparison point after changing a threshold.">
            <div className="setting-row">
              <div>
                <strong>Send a test alert</strong>
                <small>Posts one sample banner. It does not consume a real alert.</small>
              </div>
              <button className="button" onClick={() => void test()}>
                Send test
              </button>
            </div>
            <div className="setting-row">
              <div>
                <strong>Forget current levels</strong>
                <small>
                  Alerts compare against the last level seen. After changing a threshold that
                  comparison is against the old scale, so clearing it starts fresh.
                </small>
              </div>
              <button className="button" onClick={() => void forget()}>
                Forget
              </button>
            </div>
            <Choice
              label="Check every"
              detail="How often the limits are re-read while alerts are on."
              value={view.pollSeconds}
              options={[30, 60, 120, 300, 900].map((v) => ({
                value: v,
                label: v >= 60 ? `${v / 60} min` : `${v} s`,
              }))}
              busy={busy}
              onChange={(value) => void set("notifyPollSeconds", value)}
            />
          </SettingsGroup>
        </>
      )}
    </div>
  );
}

function WindowRow({
  label,
  window: watched,
  reminderAt,
}: {
  label: string;
  window: WatchedWindow;
  reminderAt: string | null;
}) {
  return (
    <div className="setting-row watched-row">
      <div>
        <strong>{label}</strong>
        <small>
          {watched.resetsIn ? `Resets in ${watched.resetsIn}` : "No reset time reported"}
          {reminderAt ? " · reminder armed" : ""}
        </small>
      </div>
      <div className="watched-state">
        <span className={`pill level-${watched.level}`}>{Math.round(watched.percent)}%</span>
        <span className="card-note">{watched.zone}</span>
      </div>
    </div>
  );
}

function SettingsGroup({
  title,
  subtitle,
  children,
}: {
  title: string;
  subtitle: string;
  children: React.ReactNode;
}) {
  return (
    <section className="settings-group">
      <div>
        <h2>{title}</h2>
        <p>{subtitle}</p>
      </div>
      <div className="settings-box">{children}</div>
    </section>
  );
}

function Toggle({
  label,
  detail,
  checked,
  busy,
  onChange,
}: {
  label: string;
  detail: string;
  checked: boolean;
  busy: boolean;
  onChange: (value: boolean) => void;
}) {
  return (
    <div className="setting-row">
      <div>
        <strong>{label}</strong>
        <small>{detail}</small>
      </div>
      <button
        className="switch"
        role="switch"
        aria-checked={checked}
        aria-label={label}
        disabled={busy}
        onClick={() => onChange(!checked)}
      >
        <span className="knob" />
      </button>
    </div>
  );
}

function Choice({
  label,
  detail,
  value,
  options,
  busy,
  onChange,
}: {
  label: string;
  detail: string;
  value: number;
  options: { value: number; label: string }[];
  busy: boolean;
  onChange: (value: number) => void;
}) {
  return (
    <div className="setting-row">
      <div>
        <strong>{label}</strong>
        <small>{detail}</small>
      </div>
      <div className="segmented">
        {options.map((option) => (
          <button
            key={option.value}
            aria-pressed={value === option.value}
            disabled={busy}
            onClick={() => onChange(option.value)}
          >
            {option.label}
          </button>
        ))}
      </div>
    </div>
  );
}
