import { useCallback, useEffect, useState, type CSSProperties } from "react";
import { Check, Copy, Pipette, Trash } from "lucide-react";

import { ipc, listenEvent } from "../lib/ipc";
import { timeAgo } from "../lib/format";
import type { CopyFormatOption, PickResult, SwatchRow } from "../lib/types";
import {
  Button,
  Card,
  ConfirmDialog,
  EmptyState,
  FormGroup,
  FormRow,
  IconButton,
  List,
  ListRow,
  Notice,
  Page,
  PageHeader,
  Section,
  Segmented,
  Select,
  Skeleton,
  Tooltip,
} from "../ui";
import "./ColorPickerPage.css";

type Message = { tone: "neutral" | "good"; text: string };

const PROFILES = [
  { value: "srgb", label: "sRGB" },
  { value: "displayP3", label: "Display P3" },
] as const;

export function ColorPickerPage() {
  const [swatches, setSwatches] = useState<SwatchRow[]>([]);
  const [formats, setFormats] = useState<CopyFormatOption[]>([]);
  const [settings, setSettings] = useState<Record<string, unknown>>({});
  const [picking, setPicking] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<Message | null>(null);
  const [copied, setCopied] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [loadFailed, setLoadFailed] = useState(false);
  const [confirmClear, setConfirmClear] = useState(false);
  const [clearing, setClearing] = useState(false);

  const load = useCallback(async () => {
    try {
      setSwatches(await ipc.colorSwatches());
      setError(null);
      setLoadFailed(false);
    } catch (reason) {
      setError(String(reason));
      setLoadFailed(true);
    } finally {
      setLoaded(true);
    }
  }, []);

  useEffect(() => {
    void load();
    void ipc.colorFormats().then(setFormats).catch(() => {});
    void ipc.settingsAll().then(setSettings).catch(() => {});
  }, [load]);

  useEffect(() => {
    const updated = listenEvent("swatches-updated", () => void load());
    const changed = listenEvent("settings-updated", () => {
      void ipc.settingsAll().then(setSettings).catch(() => {});
    });
    return () => {
      void updated.then((un) => un());
      void changed.then((un) => un());
    };
  }, [load]);

  const format = typeof settings.colorPickerCopyFormat === "string" ? settings.colorPickerCopyFormat : "hex";
  const profile = settings.colorPickerProfile === "displayP3" ? "displayP3" : "srgb";
  const selected = swatches.find((swatch) => swatch.id === selectedId) ?? swatches[0];

  const set = async (key: string, value: unknown) => {
    setSettings((current) => ({ ...current, [key]: value }));
    try {
      await ipc.settingsSet(key, value);
    } catch (reason) {
      setError(String(reason));
    }
  };

  const pick = async () => {
    setPicking(true);
    setError(null);
    setMessage(null);
    try {
      const result: PickResult = await ipc.colorPick();
      await load();
      setSelectedId(result.swatch.id);
      setMessage(
        result.copiedVia
          ? { tone: "good", text: `${result.value} copied via ${result.copiedVia}` }
          : { tone: "neutral", text: `${result.value} recorded but not copied` },
      );
      if (result.copyError) setError(result.copyError);
    } catch (reason) {
      const text = String(reason);
      if (/cancel/i.test(text)) setMessage({ tone: "neutral", text: "No color picked." });
      else setError(text);
    } finally {
      setPicking(false);
    }
  };

  const copy = async (swatch: SwatchRow, which?: string) => {
    try {
      const result = await ipc.colorCopy(swatch.id, which ?? null);
      const key = `${swatch.id}:${which ?? ""}`;
      setCopied(key);
      setMessage({ tone: "good", text: `${result.value} copied via ${result.copiedVia}` });
      setTimeout(() => setCopied((current) => (current === key ? null : current)), 1200);
      setError(null);
    } catch (reason) {
      setError(String(reason));
    }
  };

  const forget = async (id: number) => {
    try {
      await ipc.colorForget(id);
      if (selectedId === id) setSelectedId(null);
      await load();
    } catch (reason) {
      setError(String(reason));
    }
  };

  const clear = async () => {
    setClearing(true);
    try {
      await ipc.colorClear();
      setSelectedId(null);
      await load();
    } catch (reason) {
      setError(String(reason));
    } finally {
      setClearing(false);
      setConfirmClear(false);
    }
  };

  const status = picking ? (
    <Notice tone="neutral" title="The eyedropper is open">
      Click any pixel on screen, or press Escape to cancel.
    </Notice>
  ) : error ? (
    <Notice
      tone="critical"
      title={loadFailed ? "Could not read your swatches" : "Something went wrong"}
      actions={loadFailed ? <Button size="small" onClick={() => void load()}>Retry</Button> : undefined}
    >
      {error}
    </Notice>
  ) : message ? (
    <Notice tone={message.tone} compact>
      {message.text}
    </Notice>
  ) : null;

  return (
    <Page
      header={
        <PageHeader
          title="Color Picker"
          subtitle="Sample any pixel on screen and keep it as a swatch"
          actions={
            <Button variant="primary" icon={<Pipette />} busy={picking} onClick={() => void pick()}>
              {picking ? "Picking…" : "Pick color"}
            </Button>
          }
        />
      }
    >
      {status}

      {!loaded && <CurrentSkeleton />}

      {loaded && swatches.length === 0 && !loadFailed && (
        <EmptyState icon={<Pipette />} title="No colors picked yet">
          Pick a color and it is copied and kept here. The same history is behind vr color pick and the Pick Color action in the top bar.
        </EmptyState>
      )}

      {selected && (
        <Card className="cp-current">
          <div className="cp-current-preview" style={{ background: paint(selected) }} aria-hidden="true" />
          <div className="cp-current-body">
            <div className="cp-current-head">
              <div className="cp-current-titles">
                <p className="cp-current-hex ui-mono ui-selectable">{selected.hex.toUpperCase()}</p>
                <p className="cp-current-meta">
                  {selected.profileLabel} · picked {timeAgo(selected.pickedAt)}
                </p>
              </div>
              <Button size="small" variant="plain" aria-label={`Forget ${selected.hex}`} onClick={() => void forget(selected.id)}>
                Forget
              </Button>
            </div>
            <List inset={false} label="Formats" className="cp-formats">
              {formats.map((option) => {
                const key = `${selected.id}:${option.id}`;
                const done = copied === key;
                return (
                  <ListRow
                    key={option.id}
                    leading={<span className="cp-format-label">{option.label}</span>}
                    title={selected.formats[option.id] ?? ""}
                    mono
                    onClick={() => void copy(selected, option.id)}
                    trailing={
                      <IconButton
                        size="small"
                        label={done ? `Copied ${option.label}` : `Copy ${option.label}`}
                        tooltip={done ? "Copied" : `Copy as ${option.label}`}
                        icon={done ? <Check className="cp-copied" /> : <Copy />}
                        onClick={() => void copy(selected, option.id)}
                      />
                    }
                  />
                );
              })}
            </List>
          </div>
        </Card>
      )}

      {swatches.length > 0 && (
        <Section
          title="Swatches"
          description={`${swatches.length} ${swatches.length === 1 ? "color" : "colors"}, newest first. Click one to copy it as ${labelFor(formats, format)}.`}
          actions={
            <Button size="small" variant="destructive-plain" icon={<Trash />} onClick={() => setConfirmClear(true)}>
              Clear all
            </Button>
          }
        >
          <div className="cp-grid" role="list" aria-label="Swatches">
            {swatches.map((swatch) => {
              const done = copied === `${swatch.id}:`;
              return (
                <div role="listitem" key={swatch.id}>
                  <Tooltip content={`Copy ${swatch.hex} as ${labelFor(formats, format)}`}>
                    <button
                      type="button"
                      className="cp-swatch"
                      aria-pressed={swatch.id === selected?.id}
                      onClick={() => {
                        setSelectedId(swatch.id);
                        void copy(swatch);
                      }}
                    >
                      <span className="cp-swatch-color" style={{ "--swatch": paint(swatch) } as CSSProperties}>
                        {done && (
                          <Check className={swatch.prefersDarkText ? "cp-swatch-check cp-on-light" : "cp-swatch-check cp-on-dark"} aria-hidden="true" />
                        )}
                      </span>
                      <span className="cp-swatch-hex ui-mono">{done ? "Copied" : swatch.hex}</span>
                    </button>
                  </Tooltip>
                </div>
              );
            })}
          </div>
        </Section>
      )}

      {loaded && formats.length > 0 && (
        <FormGroup title="Picking">
          <FormRow
            label="Copy format"
            description="What a pick puts on the clipboard. Every format stays available per swatch."
            control={
              <Select value={format} onChange={(event) => void set("colorPickerCopyFormat", event.target.value)} aria-label="Copy format">
                {formats.map((option) => (
                  <option key={option.id} value={option.id}>
                    {option.label}
                  </option>
                ))}
              </Select>
            }
          />
          <FormRow
            label="Color profile"
            description="The compositor samples sRGB. Display P3 converts it for a wide-gamut panel."
            control={<Segmented options={PROFILES} value={profile} onChange={(value) => void set("colorPickerProfile", value)} label="Color profile" />}
          />
        </FormGroup>
      )}

      <ConfirmDialog
        open={confirmClear}
        onClose={() => setConfirmClear(false)}
        onConfirm={() => void clear()}
        busy={clearing}
        destructive
        title="Forget all swatches?"
        message={`This removes all ${swatches.length} saved colors. It cannot be undone.`}
        confirmLabel="Forget all"
      />
    </Page>
  );
}

function CurrentSkeleton() {
  return (
    <Card className="cp-current" aria-hidden="true">
      <Skeleton className="cp-current-preview" radius={8} />
      <div className="cp-current-body">
        <Skeleton width={120} height={18} />
        <Skeleton width={160} height={11} />
        <Skeleton width="100%" height={11} />
        <Skeleton width="80%" height={11} />
        <Skeleton width="90%" height={11} />
      </div>
    </Card>
  );
}

function labelFor(formats: CopyFormatOption[], id: string): string {
  return formats.find((option) => option.id === id)?.label ?? id;
}

function paint(swatch: SwatchRow): string {
  return swatch.profile === "displayP3" ? `color(display-p3 ${swatch.red} ${swatch.green} ${swatch.blue})` : swatch.hex;
}
