import { useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";

import { SpendCalendar } from "../components/charts";
import { NotificationList } from "../components/NotificationList";
import { ProviderSelector } from "../components/ProviderSelector";
import { ipc } from "../lib/ipc";
import { limitProviderOf, storedLimitProvider, type LimitProvider } from "../lib/preferences";
import type { AgendaView, GaugeReport, NowPlaying, SystemSnapshot, UsageView } from "../lib/types";

type HomeRoute = "usage" | "media" | "calendar" | "system";
type HomeData = {
  usage: UsageView | null;
  limits: GaugeReport | null;
  system: SystemSnapshot | null;
  agenda: AgendaView | null;
  playing: NowPlaying | null;
};

const EMPTY: HomeData = { usage: null, limits: null, system: null, agenda: null, playing: null };
const DEFAULT_ZONES = ["America/New_York", "America/Los_Angeles"];
const ZONE_SUGGESTIONS = [
  "Europe/London", "Europe/Berlin", "Asia/Kolkata", "Asia/Tokyo", "Asia/Singapore",
  "Australia/Sydney", "America/Chicago", "America/Sao_Paulo", "Asia/Dubai",
];

function greeting(hour: number) {
  if (hour >= 5 && hour < 12) return "Good morning";
  if (hour < 17) return "Good afternoon";
  if (hour < 22) return "Good evening";
  return "Up late";
}

function compact(value: number) {
  return new Intl.NumberFormat(undefined, { notation: "compact", maximumFractionDigits: 1 }).format(value);
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
    : typeof value === "string" ? value.split(",") : DEFAULT_ZONES;
  return [...new Set(source)].filter((zone) => {
    try { new Intl.DateTimeFormat(undefined, { timeZone: zone }).format(); return true; }
    catch { return false; }
  }).slice(0, 2);
}

export function HomePage({ onNavigate }: { onNavigate: (route: HomeRoute) => void }) {
  const [now, setNow] = useState(() => new Date());
  const [data, setData] = useState<HomeData>(EMPTY);
  const [settings, setSettings] = useState<Record<string, unknown>>({});
  const [desktopReady, setDesktopReady] = useState(true);
  const [actionError, setActionError] = useState("");

  useEffect(() => {
    const timer = window.setInterval(() => setNow(new Date()), 1_000);
    return () => window.clearInterval(timer);
  }, []);

  const refresh = async () => {
    const requests = await Promise.allSettled([
      ipc.usageView(null, []), ipc.usageLimits(), ipc.systemSnapshot(),
      ipc.calendarAgenda(2, false), ipc.mediaNowPlaying(), ipc.settingsAll(),
    ] as const);
    if (requests.every((result) => result.status === "rejected")) setDesktopReady(false);
    setData({
      usage: requests[0].status === "fulfilled" ? requests[0].value : null,
      limits: requests[1].status === "fulfilled" ? requests[1].value : null,
      system: requests[2].status === "fulfilled" ? requests[2].value : null,
      agenda: requests[3].status === "fulfilled" ? requests[3].value : null,
      playing: requests[4].status === "fulfilled" ? requests[4].value : null,
    });
    if (requests[5].status === "fulfilled") setSettings(requests[5].value);
  };

  useEffect(() => { void refresh(); }, []);
  useEffect(() => {
    const changed = listen("settings-updated", () => void ipc.settingsAll().then(setSettings).catch(() => {}));
    const usage = listen("usage-updated", () => void refresh());
    return () => { void changed.then((unlisten) => unlisten()); void usage.then((unlisten) => unlisten()); };
  }, []);

  const set = async (key: string, value: unknown) => {
    const previous = settings[key];
    setSettings((current) => ({ ...current, [key]: value }));
    try { await ipc.settingsSet(key, value); setActionError(""); }
    catch (error) {
      setSettings((current) => ({ ...current, [key]: previous }));
      setActionError(String(error));
    }
  };

  const cleanKeys = async () => {
    try { await ipc.shellAction("cleanKeys"); setActionError(""); }
    catch (error) { setActionError(String(error)); }
  };

  const nextEvent = data.agenda?.happeningNow ?? data.agenda?.nextUp;
  const totals = data.usage?.dashboard?.totals;
  const selectedProvider = limitProviderOf(settings.limitsProvider);
  const gauges = data.limits?.gauges.filter((gauge) => gauge.provider === selectedProvider).slice(0, 3) ?? [];
  const localClock = useMemo(() => now.toLocaleTimeString([], { hour: "numeric", minute: "2-digit", second: "2-digit" }), [now]);

  return <div className="home-page">
    <header className="home-hero">
      <div><h1>{greeting(now.getHours())}<span className="home-name">.</span></h1><div className="home-kicker">{now.toLocaleDateString([], { weekday: "long", month: "long", day: "numeric" })}</div></div>
      <div className="home-clock"><strong>{localClock}</strong><span>{now.getHours() < 12 ? "AM" : "PM"} · {Intl.DateTimeFormat().resolvedOptions().timeZone}</span></div>
    </header>

    {!desktopReady && <div className="banner">Live information appears when Veronica runs as the installed desktop app.</div>}
    {actionError && <div className="banner error">{actionError}</div>}

    <div className="home-top-grid">
      <WorldClocks now={now} zones={zonesFrom(settings.homeClockZones)} onChange={(zones) => void set("homeClockZones", zones)} />
      <QuickActions settings={settings} onToggle={(key) => void set(key, !Boolean(settings[key]))} onCleanKeys={() => void cleanKeys()} />
    </div>

    {data.usage?.dashboard && <section className="card home-card activity-card blur-money"><div className="card-head"><h2>Activity</h2><span className="card-note">daily cost</span></div><SpendCalendar cells={data.usage.dashboard.heatmap} /></section>}

    <div className="home-grid">
      <section className="card home-card blur-calendar"><div className="card-head"><h2>Today's meetings</h2><span className="card-note">{nextEvent ? "Next up" : ""}</span></div>{nextEvent ? <div className="meeting"><strong>{nextEvent.summary}</strong><span>{new Date(nextEvent.start).toLocaleString([], { weekday: "short", hour: "2-digit", minute: "2-digit" })}</span></div> : <p className="quiet">No meetings today. Clear runway.</p>}<JumpLink label="Open Calendar" onClick={() => onNavigate("calendar")} /></section>

      <section className="card home-card blur-money blur-usage"><div className="card-head"><h2>Agent usage</h2><span className="card-note">all time</span></div><div className="metric-row compact-metrics"><Metric label="Spend" value={totals ? `$${totals.cost.toFixed(2)}` : "—"} /><Metric label="Tokens" value={totals ? compact(totals.tokens) : "—"} /></div><JumpLink label="Open Agent Usage" onClick={() => onNavigate("usage")} /></section>

      <section className="card home-card blur-agents"><div className="card-head"><div className="limits-title"><h2>Rate limits</h2><ProviderSelector compact value={selectedProvider} onChange={(provider: LimitProvider) => void set("limitsProvider", storedLimitProvider(provider))} /></div></div><div className="limit-list">{gauges.length ? gauges.map((gauge) => <div className="limit-row" key={`${gauge.provider}-${gauge.window}`}><span>{gauge.window}</span><div><i style={{ width: `${Math.min(100, gauge.percent)}%` }} /></div><b>{Math.round(gauge.percent)}%</b></div>) : <span className="card-note">No limit data yet</span>}</div><JumpLink label="View limits" onClick={() => onNavigate("usage")} /></section>

      <section className="card home-card blur-music"><div className="card-head"><h2>Music</h2><span className="card-note">MPRIS</span></div>{data.playing ? <div className="meeting"><strong>{data.playing.title || "Untitled"}</strong><span>{data.playing.artist || data.playing.identity}</span></div> : <p className="quiet">Nothing is playing.</p>}<JumpLink label="Open Music" onClick={() => onNavigate("media")} /></section>

      <section className="card home-card notice-card"><NotificationList limit={4} /></section>

      <section className="card home-card"><div className="card-head"><h2>This computer</h2><span className="card-note">Live</span></div><div className="metric-row compact-metrics"><Metric label="CPU" value={data.system ? `${Math.round(data.system.cpu.usagePercent)}%` : "—"} /><Metric label="Memory" value={data.system ? `${Math.round(data.system.memory.usedBytes / data.system.memory.totalBytes * 100)}%` : "—"} /></div><JumpLink label="Open System" onClick={() => onNavigate("system")} /></section>
    </div>
  </div>;
}

function WorldClocks({ now, zones, onChange }: { now: Date; zones: string[]; onChange: (zones: string[]) => void }) {
  const [adding, setAdding] = useState(false);
  const [query, setQuery] = useState("");
  const panel = useRef<HTMLDivElement>(null);
  const local = Intl.DateTimeFormat().resolvedOptions().timeZone;
  const matches = useMemo(() => {
    const needle = query.trim().replaceAll(" ", "_").toLowerCase();
    return supportedZones().filter((zone) => !zones.includes(zone) && zone !== local && (!needle || zone.toLowerCase().includes(needle))).slice(0, 14);
  }, [local, query, zones]);

  useEffect(() => {
    if (!adding) return;
    const close = (event: PointerEvent) => { if (!panel.current?.contains(event.target as Node)) setAdding(false); };
    window.addEventListener("pointerdown", close);
    return () => window.removeEventListener("pointerdown", close);
  }, [adding]);

  return <section className="card home-card world-card"><div className="card-head"><h2>World clocks</h2><span className="card-note">hover a clock to remove</span></div><div className="clock-row">
    <ClockTile now={now} zone={local} label="Local" />
    {zones.map((zone) => <ClockTile key={zone} now={now} zone={zone} label={zoneName(zone)} onRemove={() => onChange(zones.filter((item) => item !== zone))} />)}
    {zones.length < 2 && <div className="clock-add-wrap" ref={panel}><button className="clock-add" aria-label="Add city" onClick={() => setAdding((value) => !value)}><span>+</span><b>Add city</b></button>{adding && <div className="zone-popover"><input autoFocus value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Search city or region" aria-label="Search city or region" /><div>{matches.map((zone) => <button key={zone} onClick={() => { onChange([...zones, zone]); setAdding(false); setQuery(""); }}><strong>{zoneName(zone)}</strong><span>{zone.split("/")[0]}</span></button>)}{matches.length === 0 && <p>No matching timezones</p>}</div></div>}</div>}
  </div></section>;
}

function ClockTile({ now, zone, label, onRemove }: { now: Date; zone: string; label: string; onRemove?: () => void }) {
  const parts = new Intl.DateTimeFormat("en-US", { timeZone: zone, hour: "numeric", minute: "numeric", second: "numeric", hour12: false }).formatToParts(now);
  const number = (type: Intl.DateTimeFormatPartTypes) => Number(parts.find((part) => part.type === type)?.value ?? 0);
  const hour = number("hour") % 12;
  const minute = number("minute");
  const second = number("second");
  const zoneOffset = offsetMinutes(now, zone) - offsetMinutes(now, Intl.DateTimeFormat().resolvedOptions().timeZone);
  const offset = zoneOffset === 0 ? "same time" : `${zoneOffset > 0 ? "+" : "−"}${Math.abs(zoneOffset / 60).toFixed(zoneOffset % 60 ? 1 : 0)}h`;
  return <div className="clock-tile"><div className="clock-face" aria-hidden="true">{Array.from({ length: 12 }, (_, index) => <i key={index} className={index % 3 === 0 ? "major" : ""} style={{ transform: `rotate(${index * 30}deg)` }} />)}<span className="clock-hand hour" style={{ transform: `translateX(-50%) rotate(${(hour + minute / 60) * 30}deg)` }} /><span className="clock-hand minute" style={{ transform: `translateX(-50%) rotate(${(minute + second / 60) * 6}deg)` }} /><span className="clock-hand second" style={{ transform: `translateX(-50%) rotate(${second * 6}deg)` }} />{onRemove && <button aria-label={`Remove ${label}`} onClick={onRemove}>×</button>}</div><strong>{label}</strong><span>{now.toLocaleTimeString([], { timeZone: zone, hour: "numeric", minute: "2-digit" })}</span><small>{offset}</small></div>;
}

function offsetMinutes(date: Date, zone: string) {
  const parts = new Intl.DateTimeFormat("en-US", { timeZone: zone, year: "numeric", month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", hourCycle: "h23" }).formatToParts(date);
  const take = (type: Intl.DateTimeFormatPartTypes) => Number(parts.find((part) => part.type === type)?.value);
  return Math.round((Date.UTC(take("year"), take("month") - 1, take("day"), take("hour"), take("minute")) - date.getTime()) / 60_000);
}

function QuickActions({ settings, onToggle, onCleanKeys }: { settings: Record<string, unknown>; onToggle: (key: string) => void; onCleanKeys: () => void }) {
  return <section className="card home-card actions-card"><div className="card-head"><h2>Quick actions</h2></div><div className="action-tiles"><Action icon="⌨" title="Clean keys" detail="Lock the keyboard to wipe it" onClick={onCleanKeys} /><Action icon="☾" title="Keep awake" detail="Stop this computer from sleeping" active={Boolean(settings.preventSleep)} onClick={() => onToggle("preventSleep")} /><Action icon="◯" title="Presenter mode" detail="Blur sensitive values on screen" active={Boolean(settings.presenterMode)} onClick={() => onToggle("presenterMode")} /><Action icon="▰" title="Lid awake" detail="Keep running with the lid closed" active={Boolean(settings.lidAwakeEnabled)} onClick={() => onToggle("lidAwakeEnabled")} /></div></section>;
}

function Action({ icon, title, detail, active, onClick }: { icon: string; title: string; detail: string; active?: boolean; onClick: () => void }) {
  return <button className="action-tile" aria-pressed={active} onClick={onClick}><span>{icon}</span><strong>{title}</strong><small>{detail}</small></button>;
}

function JumpLink({ label, onClick }: { label: string; onClick: () => void }) { return <button className="jump-link" onClick={onClick}>{label}<span>→</span></button>; }
function Metric({ label, value }: { label: string; value: string }) { return <div className="home-metric"><span>{label}</span><strong>{value}</strong></div>; }
