import { useCallback, useEffect, useState } from "react";
import { Activity, CircleStop, History, Play, RefreshCw } from "lucide-react";

import { ipc } from "../lib/ipc";
import type {
  AttentionFocusSession,
  AttentionOverview,
  AttentionSettings,
  AttentionStatus,
} from "../lib/types";
import {
  BarList,
  Button,
  Card,
  CardHeader,
  EmptyState,
  Field,
  FormGroup,
  FormRow,
  List,
  ListRow,
  Notice,
  Page,
  PageHeader,
  ProgressBar,
  Section,
  Segmented,
  Select,
  Skeleton,
  SkeletonRows,
  Stat,
  StatGrid,
  Switch,
  Tabs,
  TextField,
} from "../ui";
import "./AttentionPage.css";

type View = "overview" | "timeline" | "focus" | "settings";
type Preset = "25" | "45" | "60" | "90" | "custom";

const ICON = { strokeWidth: 1.75 };
const VIEWS = [
  { id: "overview", label: "Overview" },
  { id: "timeline", label: "Timeline" },
  { id: "focus", label: "Focus" },
  { id: "settings", label: "Settings" },
] as const;
const SUBTITLES: Record<View, string> = {
  overview: "Where your time went, kept on this device",
  timeline: "Every application switch, newest first",
  focus: "Protected focus sessions and their history",
  settings: "What Attention tracks and how it groups it",
};
const PRESETS: { value: Preset; label: string }[] = [
  { value: "25", label: "25 min" },
  { value: "45", label: "45 min" },
  { value: "60", label: "60 min" },
  { value: "90", label: "90 min" },
  { value: "custom", label: "Custom" },
];
const RANGE_LABEL: Record<number, string> = { 1: "Today", 7: "Last 7 days", 30: "Last 30 days" };

const duration = (raw: number) => {
  const s = Math.max(0, Math.floor(raw));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  return h ? `${h}h ${m}m` : m ? `${m}m` : `${s}s`;
};
const clock = (raw: number) => {
  const s = Math.max(0, Math.floor(raw));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  const pad = (n: number) => String(n).padStart(2, "0");
  return h ? `${h}:${pad(m)}:${pad(sec)}` : `${pad(m)}:${pad(sec)}`;
};
const message = (error: unknown) =>
  (error instanceof Error ? error.message : String(error)).replace(/^Error:\s*/, "");

export function AttentionPage() {
  const [view, setView] = useState<View>("overview");
  const [days, setDays] = useState(1);
  const [status, setStatus] = useState<AttentionStatus | null>(null);
  const [overview, setOverview] = useState<AttentionOverview | null>(null);
  const [settings, setSettings] = useState<AttentionSettings | null>(null);
  const [history, setHistory] = useState<AttentionFocusSession[]>([]);
  const [name, setName] = useState("Deep work");
  const [minutes, setMinutes] = useState(45);
  const [preset, setPreset] = useState<Preset>("45");
  const [now, setNow] = useState(Date.now());
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);
  const [loaded, setLoaded] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [toggling, setToggling] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const [s, o, c, h] = await Promise.all([
        ipc.attentionStatus(),
        ipc.attentionOverview(days),
        ipc.attentionSettings(),
        ipc.attentionHistory(),
      ]);
      setStatus(s);
      setOverview(o);
      setSettings(c);
      setHistory(h);
      setError("");
    } catch (e) {
      setError(message(e));
    } finally {
      setLoaded(true);
    }
  }, [days]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, []);

  const save = async (next: AttentionSettings) => {
    setSettings(next);
    setSaving(true);
    try {
      await ipc.attentionSettingsSave(next);
      setError("");
      await refresh();
    } catch (e) {
      setError(message(e));
    } finally {
      setSaving(false);
    }
  };

  const active = status?.active;
  const elapsed = active ? Math.floor((now - Date.parse(active.startedAt)) / 1000) : 0;
  const remaining = active ? active.plannedDurationSeconds - elapsed : 0;

  const start = async () => {
    setToggling(true);
    try {
      await ipc.attentionStart(name, minutes * 60);
      await refresh();
    } catch (e) {
      setError(message(e));
    } finally {
      setToggling(false);
    }
  };
  const stop = async () => {
    setToggling(true);
    try {
      await ipc.attentionStop();
      await refresh();
    } catch (e) {
      setError(message(e));
    } finally {
      setToggling(false);
    }
  };

  const manualRefresh = async () => {
    setRefreshing(true);
    await refresh();
    setRefreshing(false);
  };

  const choosePreset = (next: Preset) => {
    setPreset(next);
    if (next !== "custom") setMinutes(Number(next));
  };

  const firstLoad = !loaded;
  const failedEmpty = loaded && error && !overview;

  return (
    <Page
      header={
        <PageHeader
          title="Attention"
          subtitle={SUBTITLES[view]}
          actions={
            <>
              {(view === "overview" || view === "timeline") && (
                <Select
                  variant="chip"
                  value={days}
                  onChange={(e) => setDays(Number(e.target.value))}
                  aria-label="Range"
                >
                  <option value={1}>Today</option>
                  <option value={7}>7 days</option>
                  <option value={30}>30 days</option>
                </Select>
              )}
              <Button
                icon={<RefreshCw {...ICON} />}
                busy={refreshing}
                onClick={() => void manualRefresh()}
              >
                Refresh
              </Button>
            </>
          }
          accessory={<Tabs items={VIEWS} value={view} onChange={setView} label="Attention views" />}
        />
      }
    >
      {error && (
        <Notice
          tone="critical"
          title="Attention did not answer"
          actions={
            <Button size="small" onClick={() => void manualRefresh()}>
              Retry
            </Button>
          }
        >
          {error}
        </Notice>
      )}

      {settings && !settings.enabled && view !== "settings" && (
        <Notice
          tone="neutral"
          title="Tracking is off"
          actions={
            <Button size="small" onClick={() => setView("settings")}>
              Open settings
            </Button>
          }
        >
          Existing history stays on this device.
        </Notice>
      )}

      {!failedEmpty && view === "overview" && (
        <>
          <StatGrid min={150}>
            {firstLoad || !overview ? (
              [0, 1, 2, 3].map((index) => (
                <div className="ui-stat" key={index} aria-hidden="true">
                  <Skeleton width={72} height={11} />
                  <Skeleton width={96} height={22} />
                </div>
              ))
            ) : (
              <>
                <Stat
                  label="Active"
                  value={<span className="ui-num">{duration(overview.activeSeconds)}</span>}
                  note={RANGE_LABEL[days]}
                />
                <Stat
                  label="Focused"
                  value={<span className="ui-num">{duration(overview.focusedSeconds)}</span>}
                  note={status ? `${status.completedSessions} sessions completed` : undefined}
                />
                <Stat
                  label="Idle"
                  value={<span className="ui-num">{duration(overview.idleSeconds)}</span>}
                />
                <Stat
                  label="Context switches"
                  value={<span className="ui-num">{overview.contextSwitches}</span>}
                />
              </>
            )}
          </StatGrid>

          <Section title="Applications" description="Local only, never uploaded">
            <Card>
              {firstLoad ? (
                <SkeletonRows rows={4} label="Reading activity" />
              ) : overview?.applications.length ? (
                <BarList
                  label="Time per application"
                  items={overview.applications.map(([app, seconds]) => ({
                    id: app,
                    label: app,
                    value: seconds,
                    display: duration(seconds),
                  }))}
                />
              ) : (
                <EmptyState
                  size="compact"
                  icon={<Activity {...ICON} />}
                  title="No activity yet"
                  action={
                    settings && !settings.enabled ? (
                      <Button size="small" onClick={() => setView("settings")}>
                        Turn on tracking
                      </Button>
                    ) : undefined
                  }
                >
                  Activity appears here once tracking is on.
                </EmptyState>
              )}
            </Card>
          </Section>

          {overview && overview.categories.length > 0 && (
            <Section title="Categories" description="Grouped by the rules in Settings">
              <Card>
                <BarList
                  label="Time per category"
                  items={overview.categories.map(([id, seconds]) => {
                    const category = settings?.categories.find((item) => item.id === id);
                    return {
                      id,
                      label: category?.name ?? (id === "unclassified" ? "Unclassified" : id),
                      value: seconds,
                      display: duration(seconds),
                      color: category?.color,
                    };
                  })}
                />
              </Card>
            </Section>
          )}
        </>
      )}

      {!failedEmpty && view === "timeline" && (
        <Section title="Events">
          {firstLoad ? (
            <List label="Loading events">
              <SkeletonRows rows={6} label="Reading the timeline" />
            </List>
          ) : overview?.events.length ? (
            <List label="Timeline">
              {overview.events
                .slice()
                .reverse()
                .map((event) => (
                  <ListRow
                    key={event.id}
                    title={event.idle ? "Idle" : event.application}
                    subtitle={event.title || new Date(event.startedAt).toLocaleString()}
                    meta={<span className="ui-num">{duration(event.durationSeconds)}</span>}
                  />
                ))}
            </List>
          ) : (
            <Card>
              <EmptyState
                size="compact"
                icon={<History {...ICON} />}
                title="Nothing recorded in this range"
              >
                Switch applications with tracking on and they show up here.
              </EmptyState>
            </Card>
          )}
        </Section>
      )}

      {!failedEmpty && view === "focus" && (
        <>
          <Card className="attn-focus">
            {active ? (
              <>
                <CardHeader title={active.name} subtitle="Focus session in progress" />
                <div className="attn-clock ui-num" aria-live="off">
                  {remaining >= 0 ? clock(remaining) : `+${clock(-remaining)}`}
                </div>
                <p className="attn-clock-label">
                  {remaining >= 0
                    ? `left of ${duration(active.plannedDurationSeconds)}`
                    : "Over the planned time"}
                </p>
                <ProgressBar
                  value={Math.min(1, elapsed / Math.max(1, active.plannedDurationSeconds))}
                  tone={remaining >= 0 ? "accent" : "warning"}
                  label="Session progress"
                />
                <div className="attn-focus-actions">
                  <Button
                    variant="primary"
                    icon={<CircleStop {...ICON} />}
                    busy={toggling}
                    onClick={() => void stop()}
                  >
                    Finish session
                  </Button>
                </div>
              </>
            ) : (
              <>
                <CardHeader
                  title="Start a focus session"
                  subtitle="Name a block of work and pick how long it should last."
                />
                <form
                  className="attn-focus-form"
                  onSubmit={(event) => {
                    event.preventDefault();
                    void start();
                  }}
                >
                  <Field label="Session name">
                    {(id) => (
                      <TextField id={id} value={name} onChange={(e) => setName(e.target.value)} />
                    )}
                  </Field>
                  <div className="attn-duration">
                    <span className="attn-duration-label">Duration</span>
                    <div className="attn-duration-controls">
                      <Segmented
                        options={PRESETS}
                        value={preset}
                        onChange={choosePreset}
                        label="Duration"
                      />
                      {preset === "custom" && (
                        <TextField
                          className="attn-minutes"
                          type="number"
                          min="1"
                          max="1440"
                          value={minutes}
                          onChange={(e) => setMinutes(Number(e.target.value))}
                          aria-label="Minutes"
                          trailing={<span className="attn-unit">min</span>}
                        />
                      )}
                    </div>
                  </div>
                  <div className="attn-focus-actions">
                    <Button
                      type="submit"
                      variant="primary"
                      icon={<Play {...ICON} />}
                      busy={toggling}
                    >
                      Start
                    </Button>
                  </div>
                </form>
              </>
            )}
          </Card>

          <Section title="Focus history">
            {history.length ? (
              <List label="Focus history">
                {history.map((item) => (
                  <ListRow
                    key={item.id}
                    title={item.name}
                    subtitle={new Date(item.startedAt).toLocaleString()}
                    meta={
                      <span className="ui-num">
                        {duration(
                          (Date.parse(item.endedAt ?? item.startedAt) -
                            Date.parse(item.startedAt)) /
                            1000,
                        )}
                      </span>
                    }
                  />
                ))}
              </List>
            ) : (
              <Card>
                <EmptyState size="compact" title="No sessions yet">
                  Finished focus sessions are listed here.
                </EmptyState>
              </Card>
            )}
          </Section>
        </>
      )}

      {view === "settings" && settings && (
        <>
          <FormGroup
            title="Tracking"
            footer={saving ? "Saving…" : "Saved on this device. Nothing is uploaded."}
          >
            <FormRow
              label="Track application activity"
              description="Follows the focused application through GNOME Shell."
              control={
                <Switch
                  label="Track application activity"
                  checked={settings.enabled}
                  onChange={(enabled) => void save({ ...settings, enabled })}
                />
              }
            />
            <FormRow
              label="Privacy"
              description="Applications only never stores window titles."
              control={
                <Select
                  value={settings.privacy}
                  onChange={(e) =>
                    void save({
                      ...settings,
                      privacy: e.target.value as AttentionSettings["privacy"],
                    })
                  }
                  aria-label="Privacy"
                >
                  <option value="applications">Applications only</option>
                  <option value="detailed">Detailed titles</option>
                </Select>
              }
            />
            <FormRow
              label="Idle threshold"
              description="Time without input before activity counts as idle."
              control={
                <Select
                  value={settings.idleThresholdSeconds}
                  onChange={(e) =>
                    void save({ ...settings, idleThresholdSeconds: Number(e.target.value) })
                  }
                  aria-label="Idle threshold"
                >
                  {[60, 300, 600, 900].map((v) => (
                    <option value={v} key={v}>
                      {duration(v)}
                    </option>
                  ))}
                </Select>
              }
            />
          </FormGroup>

          <FormGroup
            title="Categories"
            description="Applications are matched by name, separated by commas. Changes save when you leave a field."
          >
            {settings.categories.map((category, index) => (
              <div className="attn-category-row" key={category.id}>
                <span
                  className="attn-swatch"
                  style={{ background: category.color }}
                  aria-hidden="true"
                />
                <div className="attn-category">
                  <TextField
                    value={category.name}
                    aria-label="Category name"
                    onChange={(e) => {
                      const categories = [...settings.categories];
                      categories[index] = { ...category, name: e.target.value };
                      setSettings({ ...settings, categories });
                    }}
                    onBlur={() => settings && void save(settings)}
                  />
                  <TextField
                    aria-label={`${category.name} applications`}
                    value={category.applications.join(", ")}
                    onChange={(e) => {
                      const categories = [...settings.categories];
                      categories[index] = {
                        ...category,
                        applications: e.target.value
                          .split(",")
                          .map((v) => v.trim())
                          .filter(Boolean),
                      };
                      setSettings({ ...settings, categories });
                    }}
                    onBlur={() => settings && void save(settings)}
                  />
                </div>
              </div>
            ))}
          </FormGroup>
        </>
      )}

      {view === "settings" && !settings && firstLoad && (
        <List label="Loading settings">
          <SkeletonRows rows={3} label="Reading settings" />
        </List>
      )}
    </Page>
  );
}
