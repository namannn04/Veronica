import {
  Badge,
  Button,
  FormGroup,
  FormRow,
  Notice,
  Skeleton,
  SkeletonText,
  Stack,
  type Tone,
} from "@veronica/ui";
import { useCallback, useEffect, useRef, useState } from "react";
import { ipc, listenEvent } from "../lib/ipc";
import type { AlertsView, PacingZone, UsageLevel, WatchedWindow } from "../lib/types";
import { ChoiceRow, SwitchRow } from "./settingsRows";

const SESSION_OFFSETS = [
  { value: 5, label: "5 minutes before" },
  { value: 15, label: "15 minutes before" },
  { value: 30, label: "30 minutes before" },
  { value: 60, label: "1 hour before" },
];

const WEEKLY_OFFSETS = [
  { value: 60, label: "1 hour before" },
  { value: 120, label: "2 hours before" },
  { value: 360, label: "6 hours before" },
  { value: 720, label: "12 hours before" },
];

const MARGINS = [5, 10, 15, 20, 25];

const POLL_CHOICES = [30, 60, 120, 300, 900].map((seconds) => ({
  value: seconds,
  label: pollLabel(seconds),
}));

const LEVEL_TONES: Record<UsageLevel, Tone> = { green: "good", orange: "warning", red: "critical" };

const ZONE_LABELS: Record<PacingZone, string> = {
  chill: "Relaxed",
  onTrack: "On track",
  warning: "Drifting fast",
  hot: "Burning hot",
};

const CHECK_FEEDBACK_MS = 5000;

function pollLabel(seconds: number) {
  return seconds >= 60 ? `Every ${seconds / 60} min` : `Every ${seconds} s`;
}

function offsetLabel(minutes: number) {
  return minutes >= 60 && minutes % 60 === 0 ? `${minutes / 60} h before` : `${minutes} min before`;
}

type CheckFeedback = { row: "test" | "forget"; text: string } | null;

export function AlertsPane() {
  const [view, setView] = useState<AlertsView | null>(null);
  const [busy, setBusy] = useState(false);
  const [checkFeedback, setCheckFeedback] = useState<CheckFeedback>(null);
  const [error, setError] = useState<string | null>(null);
  const feedbackTimer = useRef<number | undefined>(undefined);

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

  useEffect(() => {
    const changed = listenEvent("settings-updated", () => void load());
    const posted = listenEvent("alerts-posted", () => void load());
    return () => {
      void changed.then((un) => un());
      void posted.then((un) => un());
    };
  }, [load]);

  useEffect(() => () => window.clearTimeout(feedbackTimer.current), []);

  const showFeedback = (row: "test" | "forget", text: string) => {
    setCheckFeedback({ row, text });
    window.clearTimeout(feedbackTimer.current);
    feedbackTimer.current = window.setTimeout(() => setCheckFeedback(null), CHECK_FEEDBACK_MS);
  };

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
      showFeedback("test", await ipc.alertsTest());
    } catch (reason) {
      setError(String(reason));
    }
  };

  const forget = async () => {
    try {
      await ipc.alertsReset();
      showFeedback("forget", "Levels forgotten. The next check starts from where you are now.");
      await load();
    } catch (reason) {
      setError(String(reason));
    }
  };

  const errorNotice = error && (
    <Notice
      tone="critical"
      title={view ? "That change was not saved" : "Alert settings did not load"}
      actions={
        <Button size="small" onClick={() => void load()}>
          Retry
        </Button>
      }
    >
      {error}
    </Notice>
  );

  if (!view) {
    return error ? (
      errorNotice
    ) : (
      <FormGroup title="Alerts">
        {[0, 1, 2].map((slot) => (
          <FormRow
            key={slot}
            label={<SkeletonText width={slot === 0 ? 140 : 180} />}
            description={<SkeletonText width={260} />}
            control={<Skeleton width={36} height={20} radius={10} />}
          />
        ))}
      </FormGroup>
    );
  }

  const s = view.settings;
  const on = s.master;

  return (
    <Stack>
      {errorNotice}

      <FormGroup
        title="Alerts"
        description="Each alert fires once when a window crosses a level or a pacing zone. Staying in the same zone will not page you again."
      >
        <SwitchRow
          label="Enable alerts"
          description={
            on
              ? `Claude's limits are re-read every ${view.pollSeconds} s while this is on.`
              : "Off means no banners and no requests to the provider."
          }
          checked={on}
          disabled={busy}
          onChange={(value) => void set("notifyMaster", value)}
        />
      </FormGroup>

      {on && (
        <>
          <FormGroup
            title="Watched windows"
            description="The two Claude windows the alerts follow, read live."
          >
            {view.note && <FormRow label="No live reading" description={view.note} />}
            {view.session && (
              <WindowRow
                label="Session (5h)"
                window={view.session}
                reminderAt={view.sessionReminderAt}
              />
            )}
            {view.week && (
              <WindowRow label="Weekly" window={view.week} reminderAt={view.weekReminderAt} />
            )}
            {!view.session && !view.week && !view.note && (
              <FormRow
                label="No windows reported yet"
                description="They appear after the first successful check."
              />
            )}
          </FormGroup>

          <FormGroup
            title="Level crossings"
            description="One alert when a window's level rises, and one when it falls back to green."
          >
            <SwitchRow
              label="Session (5h) alerts"
              description="Fires once when the session window crosses warn or critical, then stays quiet while you remain in that zone."
              checked={s.trackSession}
              disabled={busy}
              onChange={(value) => void set("notifyTrackSession", value)}
            />
            <SwitchRow
              label="Weekly alerts"
              description="The same once-per-zone behaviour as the session alerts."
              checked={s.trackWeekly}
              disabled={busy}
              onChange={(value) => void set("notifyTrackWeekly", value)}
            />
            <SwitchRow
              label="Back to green"
              description="A quiet note when a window resets and you have a fresh slate."
              checked={s.recovery}
              disabled={busy}
              onChange={(value) => void set("notifyRecovery", value)}
            />
            <SwitchRow
              label="Time-aware levels"
              description="Blend the time left into the level, so 60% with minutes to go outranks 80% with a week."
              checked={s.smartColor}
              disabled={busy}
              onChange={(value) => void set("smartColor", value)}
            />
            <ChoiceRow
              label="Warn at"
              description="The first level, used when time-aware levels are off."
              value={s.thresholds.warningPercent}
              options={[40, 50, 60, 70, 80]
                .filter((v) => v < s.thresholds.criticalPercent)
                .map((v) => ({ value: v, label: `${v}%` }))}
              format={(v) => `${v}%`}
              disabled={busy}
              onChange={(value) => void set("limitsWarnPercent", value)}
            />
            <ChoiceRow
              label="Critical at"
              description="The top level."
              value={s.thresholds.criticalPercent}
              options={[70, 80, 85, 90, 95]
                .filter((v) => v > s.thresholds.warningPercent)
                .map((v) => ({ value: v, label: `${v}%` }))}
              format={(v) => `${v}%`}
              disabled={busy}
              onChange={(value) => void set("limitsCritPercent", value)}
            />
          </FormGroup>

          <FormGroup
            title="Pace"
            description="A separate signal from the levels: how far ahead of an even burn you are, whatever the absolute figure."
          >
            <ChoiceRow
              label="Pacing margin"
              description="How far ahead of a linear burn still counts as on track."
              value={s.pacingMargin}
              options={MARGINS.map((v) => ({ value: v, label: `±${v} points` }))}
              format={(v) => `±${v} points`}
              disabled={busy}
              onChange={(value) => void set("limitsPacingMargin", value)}
            />
            <SwitchRow
              label="Drifting fast"
              description="A little faster than ideal. Worth a glance, not a panic."
              checked={s.pacingWarning}
              disabled={busy}
              onChange={(value) => void set("notifyPacingWarning", value)}
            />
            <SwitchRow
              label="Burning hot"
              description="Well ahead of pace."
              checked={s.pacingHot}
              disabled={busy}
              onChange={(value) => void set("notifyPacingHot", value)}
            />
          </FormGroup>

          <FormGroup
            title="Before a reset"
            description="A single reminder a fixed time before each window resets."
          >
            <SwitchRow
              label="Remind before the session resets"
              description={
                view.sessionReminderAt
                  ? `Next at ${new Date(view.sessionReminderAt).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })}.`
                  : "Fires once per window."
              }
              checked={s.reminderSession}
              disabled={busy}
              onChange={(value) => void set("notifyReminderSession", value)}
            />
            {s.reminderSession && (
              <ChoiceRow
                label="Session lead time"
                description="How long before the reset."
                value={s.reminderSessionOffsetMin}
                options={SESSION_OFFSETS}
                format={offsetLabel}
                disabled={busy}
                onChange={(value) => void set("notifyReminderSessionOffsetMin", value)}
              />
            )}
            <SwitchRow
              label="Remind before the week resets"
              description={
                view.weekReminderAt
                  ? `Next at ${new Date(view.weekReminderAt).toLocaleString([], { weekday: "short", hour: "numeric", minute: "2-digit" })}.`
                  : "Fires once per cycle."
              }
              checked={s.reminderWeekly}
              disabled={busy}
              onChange={(value) => void set("notifyReminderWeekly", value)}
            />
            {s.reminderWeekly && (
              <ChoiceRow
                label="Weekly lead time"
                description="How long before the reset."
                value={s.reminderWeeklyOffsetMin}
                options={WEEKLY_OFFSETS}
                format={offsetLabel}
                disabled={busy}
                onChange={(value) => void set("notifyReminderWeeklyOffsetMin", value)}
              />
            )}
          </FormGroup>

          <FormGroup title="Sign-in" description="The one provider failure you can act on.">
            <SwitchRow
              label="Token expired"
              description="Tells you when Claude's saved credentials need a fresh login. At most once an hour."
              checked={s.tokenExpired}
              disabled={busy}
              onChange={(value) => void set("notifyTokenExpired", value)}
            />
          </FormGroup>

          <FormGroup
            title="Checks"
            description="Confirm banners arrive, and reset the comparison point after changing a threshold."
          >
            <FormRow
              label="Send a test alert"
              description={
                checkFeedback?.row === "test"
                  ? checkFeedback.text
                  : "Posts one sample banner. It does not use up a real alert."
              }
              control={
                <Button size="small" onClick={() => void test()}>
                  Send test
                </Button>
              }
            />
            <FormRow
              label="Forget current levels"
              description={
                checkFeedback?.row === "forget"
                  ? checkFeedback.text
                  : "Alerts compare against the last level seen. After changing a threshold, clearing it starts fresh."
              }
              control={
                <Button size="small" onClick={() => void forget()}>
                  Forget levels
                </Button>
              }
            />
            <ChoiceRow
              label="Check every"
              description="How often the limits are re-read while alerts are on."
              value={view.pollSeconds}
              options={POLL_CHOICES}
              format={pollLabel}
              disabled={busy}
              onChange={(value) => void set("notifyPollSeconds", value)}
            />
          </FormGroup>
        </>
      )}
    </Stack>
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
    <FormRow
      label={label}
      description={`${watched.resetsIn ? `Resets in ${watched.resetsIn}` : "No reset time reported"}${reminderAt ? " · reminder armed" : ""}`}
      control={
        <span className="settings-watched">
          <span className="settings-watched-zone">{ZONE_LABELS[watched.zone] ?? watched.zone}</span>
          <Badge tone={LEVEL_TONES[watched.level] ?? "neutral"}>
            {Math.round(watched.percent)}%
          </Badge>
        </span>
      }
    />
  );
}
