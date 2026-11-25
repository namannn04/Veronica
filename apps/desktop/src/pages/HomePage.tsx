import {
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
  SearchField,
} from "@veronica/ui";
import { Coffee, EyeOff, Globe2, Keyboard, LaptopMinimal, Plus, X, Zap } from "lucide-react";
import { type ReactNode, useCallback, useEffect, useMemo, useState } from "react";
import { ExtensionCards } from "../components/ExtensionCards";
import { NotificationList } from "../components/NotificationList";
import { WelcomeCard } from "../components/WelcomeCard";
import { ipc, listenEvent } from "../lib/ipc";
import type { ExtensionRoute } from "../lib/navigation";
import { STARTERS_KEY, storedStarters } from "../lib/onboarding";
import type { PresenterView } from "../lib/types";
import "./HomePage.css";

type HomeRoute = "library" | ExtensionRoute;
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
    const requests = await Promise.allSettled([ipc.settingsAll(), ipc.presenterState()] as const);
    if (requests.every((result) => result.status === "rejected")) {
      setDesktopReady(false);
    }
    if (requests[0].status === "fulfilled") {
      setSettings(requests[0].value);
    }
    if (requests[1].status === "fulfilled") {
      setPresenter(requests[1].value);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

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
    return () => {
      void changed.then((unlisten) => unlisten());
    };
  }, []);

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

  const subtitle = useMemo(
    () =>
      `${now.toLocaleDateString([], { weekday: "long", month: "long", day: "numeric" })}, ${now.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })}`,
    [now],
  );

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
        {storedStarters(settings).length > 0 && (
          <WelcomeCard
            starters={storedStarters(settings)}
            onDismiss={() => void set(STARTERS_KEY, [])}
            onOpenLibrary={() => onNavigate("library")}
          />
        )}
        <ExtensionCards onOpen={onNavigate} />

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

        <NotificationList limit={4} className="home-card home-span-4" />
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
