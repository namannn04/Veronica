import {
  Activity,
  ArrowUpRight,
  CalendarDays,
  Coffee,
  Cpu,
  EyeOff,
  Gauge as GaugeIcon,
  Globe2,
  Keyboard,
  LaptopMinimal,
  Music2,
  Plus,
  Sparkles,
  X,
  Zap,
} from "lucide-react";
import { type ReactNode, useCallback, useEffect, useMemo, useState } from "react";
import { ringStatus, SpendCalendar } from "../components/charts";
import { NotificationList } from "../components/NotificationList";
import { ProviderSelector } from "../components/ProviderSelector";
import { money, tokens } from "../lib/format";
import { ipc, listenEvent } from "../lib/ipc";
import { startVisiblePolling } from "../lib/polling";
import { type LimitProvider, limitProviderOf, storedLimitProvider } from "../lib/preferences";
import type {
  AgendaView,
  GaugeReport,
  NowPlaying,
  PresenterView,
  SystemSnapshot,
  UsageView,
} from "../lib/types";
import {
  Badge,
  Button,
  Card,
  CardHeader,
  Dialog,
  IconButton,
  List,
  ListRow,
  Notice,
  Page,
  PageHeader,
  ProgressBar,
  SearchField,
  Skeleton,
  SkeletonText,
} from "../ui";
import "./HomePage.css";

type HomeRoute = "usage" | "media" | "calendar" | "system";
type HomeData = {
  usage: UsageView | null;
  limits: GaugeReport | null;
  system: SystemSnapshot | null;
  agenda: AgendaView | null;
  playing: NowPlaying | null;
};

const EMPTY: HomeData = { usage: null, limits: null, system: null, agenda: null, playing: null };
const LIVE_POLL_MS = 3_000;
const LIMITS_POLL_MS = 5 * 60_000;
const DEFAULT_ZONES = ["America/New_York", "America/Los_Angeles"];
const ZONE_SUGGESTIONS = [
  "Europe/London",
  "Europe/Berlin",
  "Asia/Kolkata",
  "Asia/Tokyo",
  "Asia/Singapore",
  "Australia/Sydney",
  "America/Chicago",
  "America/Sao_Paulo",
  "Asia/Dubai",
];
const MAX_ZONES = 2;

function greeting(hour: number) {
  if (hour >= 5 && hour < 12) {
    return "Good morning";
  }
  if (hour < 17) {
    return "Good afternoon";
  }
  if (hour < 22) {
    return "Good evening";
  }
  return "Up late";
}

function zoneName(id: string) {
  return id.split("/").at(-1)?.replaceAll("_", " ") ?? id;
}

function supportedZones(): string[] {
  const intl = Intl as typeof Intl & { supportedValuesOf?: (key: "timeZone") => string[] };
  return intl.supportedValuesOf?.("timeZone") ?? ZONE_SUGGESTIONS;
}

function zonesFrom(value: unknown): string[] {
  const source = Array.isArray(value)
    ? value.filter((entry): entry is string => typeof entry === "string")
    : typeof value === "string"
      ? value.split(",")
      : DEFAULT_ZONES;
  return [...new Set(source)]
    .filter((zone) => {
      try {
        new Intl.DateTimeFormat(undefined, { timeZone: zone }).format();
        return true;
      } catch {
        return false;
      }
    })
    .slice(0, MAX_ZONES);
}

export function HomePage({ onNavigate }: { onNavigate: (route: HomeRoute) => void }) {
  const [now, setNow] = useState(() => new Date());
  const [data, setData] = useState<HomeData>(EMPTY);
  const [loaded, setLoaded] = useState(false);
  const [settings, setSettings] = useState<Record<string, unknown>>({});
  const [desktopReady, setDesktopReady] = useState(true);
  const [actionError, setActionError] = useState("");
  const [presenter, setPresenter] = useState<PresenterView | null>(null);

  useEffect(() => {
    const timer = window.setInterval(() => {
      if (!document.hidden) {
        setNow(new Date());
      }
    }, 1_000);
    return () => window.clearInterval(timer);
  }, []);

  const refresh = useCallback(async () => {
    const requests = await Promise.allSettled([
      ipc.usageView(null, []),
      ipc.usageLimits(),
      ipc.systemSnapshot(),
      ipc.calendarAgenda(2, false),
      ipc.mediaNowPlaying(),
      ipc.settingsAll(),
      ipc.presenterState(),
    ] as const);
    if (requests.every((result) => result.status === "rejected")) {
      setDesktopReady(false);
    }
    setData({
      usage: requests[0].status === "fulfilled" ? requests[0].value : null,
      limits: requests[1].status === "fulfilled" ? requests[1].value : null,
      system: requests[2].status === "fulfilled" ? requests[2].value : null,
      agenda: requests[3].status === "fulfilled" ? requests[3].value : null,
      playing: requests[4].status === "fulfilled" ? requests[4].value : null,
    });
    if (requests[5].status === "fulfilled") {
      setSettings(requests[5].value);
    }
    if (requests[6].status === "fulfilled") {
      setPresenter(requests[6].value);
    }
    setLoaded(true);
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    let live = true;
    const every = (ms: number, read: () => Promise<void>) =>
      startVisiblePolling(read, ms, document, false);
    const stopLive = every(LIVE_POLL_MS, async () => {
      const [system, playing] = await Promise.allSettled([
        ipc.systemSnapshot(),
        ipc.mediaNowPlaying(),
      ]);
      if (!live) {
        return;
      }
      setData((current) => ({
        ...current,
        system: system.status === "fulfilled" ? system.value : current.system,
        playing: playing.status === "fulfilled" ? playing.value : null,
      }));
    });
    const stopLimits = every(LIMITS_POLL_MS, async () => {
      const limits = await ipc.usageLimits().catch(() => null);
      if (live && limits) {
        setData((current) => ({ ...current, limits }));
      }
    });
    return () => {
      live = false;
      stopLive();
      stopLimits();
    };
  }, []);

  useEffect(() => {
    const changed = listenEvent("settings-updated", () => {
      void ipc
        .settingsAll()
        .then(setSettings)
        .catch(() => {});
      void ipc
        .presenterState()
        .then(setPresenter)
        .catch(() => {});
    });
    const usage = listenEvent("usage-updated", () => void refresh());
    return () => {
      void changed.then((unlisten) => unlisten());
      void usage.then((unlisten) => unlisten());
    };
  }, [refresh]);

  const set = async (key: string, value: unknown) => {
    const previous = settings[key];
    setSettings((current) => ({ ...current, [key]: value }));
    try {
      await ipc.settingsSet(key, value);
      setActionError("");
    } catch (error) {
      setSettings((current) => ({ ...current, [key]: previous }));
      setActionError(String(error));
    }
  };

  const togglePresenter = async () => {
    if (!presenter) {
      return;
    }
    const sharing = presenter.autoActive && presenter.autoEnabled && !presenter.autoPaused;
    const actions = !presenter.enabled
      ? ["enable", "start"]
      : !presenter.active
        ? ["start"]
        : [...(presenter.manual ? ["stop"] : []), ...(sharing ? ["dismiss"] : [])];
    try {
      for (const action of actions) {
        await ipc.presenterSet(action);
      }
      setPresenter(await ipc.presenterState());
      setActionError("");
    } catch (error) {
      setActionError(String(error));
    }
  };

  const cleanKeys = async () => {
    try {
      await ipc.shellAction("cleanKeys");
      setActionError("");
    } catch (error) {
      setActionError(String(error));
    }
  };

  const nextEvent = data.agenda?.happeningNow ?? data.agenda?.nextUp ?? null;
  const happeningNow = Boolean(data.agenda?.happeningNow);
  const totals = data.usage?.dashboard?.totals;
  const heatmap = data.usage?.dashboard?.heatmap ?? [];
  const selectedProvider = limitProviderOf(settings.limitsProvider);
  const gauges =
    data.limits?.gauges.filter((gauge) => gauge.provider === selectedProvider).slice(0, 3) ?? [];
  const providerNeedsAttention =
    data.limits?.notes.some((note) => note.startsWith(selectedProvider)) ?? false;
  const subtitle = useMemo(
    () =>
      `${now.toLocaleDateString([], { weekday: "long", month: "long", day: "numeric" })}, ${now.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })}`,
    [now],
  );
  const memoryPercent =
    data.system && data.system.memory.totalBytes > 0
      ? Math.round((data.system.memory.usedBytes / data.system.memory.totalBytes) * 100)
      : null;

  return (
    <Page header={<PageHeader title={greeting(now.getHours())} subtitle={subtitle} />}>
      {!desktopReady && (
        <Notice title="Live information is unavailable">
          Live information appears when Veronica runs as the installed desktop app.
        </Notice>
      )}
      {actionError && (
        <Notice
          tone="critical"
          title="That action did not finish"
          actions={
            <Button size="small" variant="plain" onClick={() => setActionError("")}>
              Dismiss
            </Button>
          }
        >
          {actionError}
        </Notice>
      )}

      <div className="home-grid">
        <WorldClocks
          now={now}
          zones={zonesFrom(settings.homeClockZones)}
          onChange={(zones) => void set("homeClockZones", zones)}
        />

        <Widget className="home-span-3" icon={<Zap />} title="Quick actions">
          <div className="home-actions">
            <ActionTile
              icon={<Keyboard />}
              title="Clean keys"
              detail="Lock the keyboard to wipe it"
              onClick={() => void cleanKeys()}
            />
            <ActionTile
              icon={<Coffee />}
              title="Keep awake"
              detail="Stop this computer from sleeping"
              active={Boolean(settings.preventSleep)}
              onClick={() => void set("preventSleep", !settings.preventSleep)}
            />
            <ActionTile
              icon={<EyeOff />}
              title="Presenter mode"
              detail="Blur sensitive values on screen"
              active={Boolean(presenter?.active)}
              disabled={!presenter}
              onClick={() => void togglePresenter()}
            />
            <ActionTile
              icon={<LaptopMinimal />}
              title="Lid awake"
              detail="Keep running with the lid closed"
              active={Boolean(settings.lidAwakeEnabled)}
              onClick={() => void set("lidAwakeEnabled", !settings.lidAwakeEnabled)}
            />
          </div>
        </Widget>

        <Widget
          className="home-span-4 home-activity"
          icon={<Activity />}
          title="Activity"
          meta="Daily estimated spend"
          bodyClassName="blur-money"
        >
          {!loaded ? (
            <div className="home-heat-skeleton">
              <Skeleton height={164} radius={8} />
            </div>
          ) : heatmap.length > 0 ? (
            <SpendCalendar cells={heatmap} />
          ) : (
            <Quiet>
              No activity collected yet. Agent usage fills this in after its first collection.
            </Quiet>
          )}
        </Widget>

        <Widget
          className="home-span-2"
          icon={<Sparkles />}
          title="Agent usage"
          meta="All time"
          bodyClassName="blur-money blur-usage"
          actions={
            <IconButton
              size="small"
              label="Open agent usage"
              icon={<ArrowUpRight />}
              onClick={() => onNavigate("usage")}
            />
          }
        >
          <div className="home-metrics home-metrics-pair">
            <Metric label="Spend" value={totals ? money(totals.cost) : null} loading={!loaded} />
            <Metric
              label="Tokens"
              value={totals ? tokens(totals.tokens) : null}
              loading={!loaded}
            />
          </div>
        </Widget>

        <Widget
          className="home-span-2"
          icon={<CalendarDays />}
          title="Meetings"
          meta={
            nextEvent ? (
              happeningNow ? (
                <Badge tone="accent" dot>
                  Now
                </Badge>
              ) : (
                "Next up"
              )
            ) : undefined
          }
          bodyClassName="blur-calendar"
          actions={
            <IconButton
              size="small"
              label="Open calendar"
              icon={<ArrowUpRight />}
              onClick={() => onNavigate("calendar")}
            />
          }
        >
          {!loaded ? (
            <PrimaryLines loading />
          ) : nextEvent ? (
            <PrimaryLines
              title={nextEvent.summary}
              detail={new Date(nextEvent.start).toLocaleString([], {
                weekday: "short",
                hour: "2-digit",
                minute: "2-digit",
              })}
            />
          ) : (
            <Quiet>No meetings today.</Quiet>
          )}
        </Widget>

        <Widget
          className="home-span-2"
          icon={<GaugeIcon />}
          title="Rate limits"
          bodyClassName="blur-agents"
          actions={
            <>
              <ProviderSelector
                compact
                value={selectedProvider}
                onChange={(provider: LimitProvider) =>
                  void set("limitsProvider", storedLimitProvider(provider))
                }
              />
              <IconButton
                size="small"
                label="Open rate limits"
                icon={<ArrowUpRight />}
                onClick={() => onNavigate("usage")}
              />
            </>
          }
        >
          {!loaded ? (
            <div className="home-limits" role="status" aria-label="Reading rate limits">
              {[56, 44, 72].map((width) => (
                <div className="home-limit" key={width}>
                  <span className="home-limit-name">
                    <SkeletonText width={width} />
                  </span>
                  <ProgressBar value={0} tone="neutral" size="thin" />
                  <span className="home-limit-value">
                    <SkeletonText width={28} />
                  </span>
                </div>
              ))}
            </div>
          ) : gauges.length > 0 ? (
            <div className="home-limits">
              {gauges.map((gauge) => {
                const status = ringStatus(gauge.percent);
                const tone =
                  status === "good" ? "good" : status === "critical" ? "critical" : "warning";
                return (
                  <div className="home-limit" key={`${gauge.provider}-${gauge.window}`}>
                    <span className="home-limit-name">{gauge.window}</span>
                    <ProgressBar
                      value={gauge.percent / 100}
                      tone={tone}
                      size="thin"
                      label={`${gauge.window} usage`}
                    />
                    <span className="home-limit-value ui-num">{Math.round(gauge.percent)}%</span>
                  </div>
                );
              })}
            </div>
          ) : (
            <Quiet>
              {providerNeedsAttention
                ? `${selectedProvider} needs attention. Open rate limits for details.`
                : "No limit data yet."}
            </Quiet>
          )}
        </Widget>

        <Widget
          className="home-span-2"
          icon={<Music2 />}
          title="Music"
          bodyClassName="blur-music"
          actions={
            <IconButton
              size="small"
              label="Open music"
              icon={<ArrowUpRight />}
              onClick={() => onNavigate("media")}
            />
          }
        >
          {!loaded ? (
            <div className="home-track" role="status" aria-label="Reading what is playing">
              <Skeleton className="home-track-art" width={48} height={48} radius={8} />
              <PrimaryLines loading />
            </div>
          ) : data.playing ? (
            <div className="home-track">
              <span className="home-track-art" aria-hidden="true">
                {data.playing.artUrl ? <img src={data.playing.artUrl} alt="" /> : <Music2 />}
              </span>
              <PrimaryLines
                title={data.playing.title || "Untitled"}
                detail={data.playing.artist || data.playing.identity}
              />
            </div>
          ) : (
            <Quiet>Nothing is playing.</Quiet>
          )}
        </Widget>

        <NotificationList limit={4} className="home-card home-span-4" />

        <Widget
          className="home-span-2"
          icon={<Cpu />}
          title="This computer"
          meta="Live"
          actions={
            <IconButton
              size="small"
              label="Open system"
              icon={<ArrowUpRight />}
              onClick={() => onNavigate("system")}
            />
          }
        >
          <div className="home-metrics home-metrics-stacked">
            <Metric
              label="CPU"
              value={data.system ? `${Math.round(data.system.cpu.usagePercent)}%` : null}
              loading={!loaded}
              bar={data.system ? data.system.cpu.usagePercent / 100 : loaded ? undefined : 0}
            />
            <Metric
              label="Memory"
              value={memoryPercent === null ? null : `${memoryPercent}%`}
              loading={!loaded}
              bar={memoryPercent === null ? (loaded ? undefined : 0) : memoryPercent / 100}
            />
          </div>
        </Widget>
      </div>
    </Page>
  );
}

function Widget({
  icon,
  title,
  meta,
  actions,
  className,
  bodyClassName,
  children,
}: {
  icon: ReactNode;
  title: string;
  meta?: ReactNode;
  actions?: ReactNode;
  className?: string;
  bodyClassName?: string;
  children: ReactNode;
}) {
  const trailing =
    meta || actions ? (
      <>
        {meta && <span className="home-card-meta">{meta}</span>}
        {actions}
      </>
    ) : undefined;
  return (
    <Card as="section" className={["home-card", className].filter(Boolean).join(" ")}>
      <CardHeader icon={icon} title={title} actions={trailing} />
      <div className={["home-card-body", bodyClassName].filter(Boolean).join(" ")}>{children}</div>
    </Card>
  );
}

function Quiet({ children }: { children: ReactNode }) {
  return <p className="home-quiet">{children}</p>;
}

function PrimaryLines({
  title,
  detail,
  loading = false,
}: {
  title?: string;
  detail?: string;
  loading?: boolean;
}) {
  if (loading) {
    return (
      <div className="home-lines">
        <span className="home-lines-title">
          <SkeletonText width="70%" />
        </span>
        <span className="home-lines-detail">
          <SkeletonText width="40%" />
        </span>
      </div>
    );
  }
  return (
    <div className="home-lines">
      <strong className="home-lines-title">{title}</strong>
      {detail && <span className="home-lines-detail">{detail}</span>}
    </div>
  );
}

function Metric({
  label,
  value,
  loading,
  bar,
}: {
  label: string;
  value: string | null;
  loading: boolean;
  bar?: number;
}) {
  return (
    <div className="home-metric">
      <span className="home-metric-label">{label}</span>
      {loading ? (
        <span className="home-metric-value">
          <SkeletonText width={96} />
        </span>
      ) : value === null ? (
        <span className="home-metric-missing">Not available</span>
      ) : (
        <strong className="home-metric-value ui-num ui-selectable">{value}</strong>
      )}
      {bar !== undefined && (
        <ProgressBar value={bar} tone="neutral" size="thin" label={`${label} usage`} />
      )}
    </div>
  );
}

function ActionTile({
  icon,
  title,
  detail,
  active,
  disabled,
  onClick,
}: {
  icon: ReactNode;
  title: string;
  detail: string;
  active?: boolean;
  disabled?: boolean;
  onClick: () => void;
}) {
  const toggle = active !== undefined;
  return (
    <button
      type="button"
      className="home-action"
      aria-pressed={toggle ? active : undefined}
      disabled={disabled}
      onClick={onClick}
    >
      <span className="home-action-top">
        <span className="home-action-icon" aria-hidden="true">
          {icon}
        </span>
        {toggle && <span className="home-action-state">{active ? "On" : "Off"}</span>}
      </span>
      <span className="home-action-title">{title}</span>
      <span className="home-action-detail">{detail}</span>
    </button>
  );
}

function WorldClocks({
  now,
  zones,
  onChange,
}: {
  now: Date;
  zones: string[];
  onChange: (zones: string[]) => void;
}) {
  const [adding, setAdding] = useState(false);
  const [query, setQuery] = useState("");
  const local = Intl.DateTimeFormat().resolvedOptions().timeZone;
  const matches = useMemo(() => {
    const needle = query.trim().replaceAll(" ", "_").toLowerCase();
    return supportedZones()
      .filter(
        (zone) =>
          !zones.includes(zone) &&
          zone !== local &&
          (!needle || zone.toLowerCase().includes(needle)),
      )
      .slice(0, 14);
  }, [local, query, zones]);

  const close = () => {
    setAdding(false);
    setQuery("");
  };

  return (
    <Widget className="home-span-3" icon={<Globe2 />} title="World clocks">
      <div className="home-clocks">
        <ClockTile now={now} zone={local} label="Local" />
        {zones.map((zone) => (
          <ClockTile
            key={zone}
            now={now}
            zone={zone}
            label={zoneName(zone)}
            onRemove={() => onChange(zones.filter((item) => item !== zone))}
          />
        ))}
        {zones.length < MAX_ZONES && (
          <button type="button" className="home-clock-add" onClick={() => setAdding(true)}>
            <span className="home-clock-add-face" aria-hidden="true">
              <Plus />
            </span>
            <span className="home-clock-label">Add city</span>
          </button>
        )}
      </div>
      <Dialog
        open={adding}
        onClose={close}
        title="Add a city"
        description="Show another time zone beside your local time."
        width={420}
      >
        <div className="home-zone-picker">
          <SearchField
            value={query}
            onChange={setQuery}
            placeholder="Search city or region"
            data-autofocus
          />
          <div className="home-zone-results">
            {matches.length > 0 ? (
              <List label="Time zones">
                {matches.map((zone) => (
                  <ListRow
                    key={zone}
                    title={zoneName(zone)}
                    meta={zone.split("/")[0]}
                    onClick={() => {
                      onChange([...zones, zone]);
                      close();
                    }}
                  />
                ))}
              </List>
            ) : (
              <Quiet>No matching time zones.</Quiet>
            )}
          </div>
        </div>
      </Dialog>
    </Widget>
  );
}

function ClockTile({
  now,
  zone,
  label,
  onRemove,
}: {
  now: Date;
  zone: string;
  label: string;
  onRemove?: () => void;
}) {
  const parts = new Intl.DateTimeFormat("en-US", {
    timeZone: zone,
    hour: "numeric",
    minute: "numeric",
    second: "numeric",
    hour12: false,
  }).formatToParts(now);
  const number = (type: Intl.DateTimeFormatPartTypes) =>
    Number(parts.find((part) => part.type === type)?.value ?? 0);
  const hour = number("hour") % 12;
  const minute = number("minute");
  const second = number("second");
  const zoneOffset =
    offsetMinutes(now, zone) - offsetMinutes(now, Intl.DateTimeFormat().resolvedOptions().timeZone);
  const offset =
    zoneOffset === 0
      ? "Same time"
      : `${zoneOffset > 0 ? "+" : "−"}${Math.abs(zoneOffset / 60).toFixed(zoneOffset % 60 ? 1 : 0)}h`;
  return (
    <div className="home-clock">
      <div className="home-clock-face" aria-hidden="true">
        {Array.from({ length: 12 }, (_, index) => index).map((tick) => (
          <i
            key={tick}
            className={tick % 3 === 0 ? "home-clock-tick major" : "home-clock-tick"}
            style={{ transform: `rotate(${tick * 30}deg)` }}
          />
        ))}
        <span
          className="home-clock-hand hour"
          style={{ transform: `translateX(-50%) rotate(${(hour + minute / 60) * 30}deg)` }}
        />
        <span
          className="home-clock-hand minute"
          style={{ transform: `translateX(-50%) rotate(${(minute + second / 60) * 6}deg)` }}
        />
        <span
          className="home-clock-hand second"
          style={{ transform: `translateX(-50%) rotate(${second * 6}deg)` }}
        />
      </div>
      {onRemove && (
        <span className="home-clock-remove">
          <IconButton
            size="small"
            variant="secondary"
            label={`Remove ${label}`}
            icon={<X />}
            onClick={onRemove}
          />
        </span>
      )}
      <span className="home-clock-label">{label}</span>
      <span className="home-clock-time ui-num">
        {now.toLocaleTimeString([], { timeZone: zone, hour: "numeric", minute: "2-digit" })}
      </span>
      <span className="home-clock-offset ui-num">{offset}</span>
    </div>
  );
}

function offsetMinutes(date: Date, zone: string) {
  const parts = new Intl.DateTimeFormat("en-US", {
    timeZone: zone,
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    hourCycle: "h23",
  }).formatToParts(date);
  const take = (type: Intl.DateTimeFormatPartTypes) =>
    Number(parts.find((part) => part.type === type)?.value);
  return Math.round(
    (Date.UTC(take("year"), take("month") - 1, take("day"), take("hour"), take("minute")) -
      date.getTime()) /
      60_000,
  );
}
