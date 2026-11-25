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
  ListSkeleton,
  Notice,
  Page,
  PageHeader,
  Section,
  Segmented,
  Select,
  Skeleton,
  SkeletonText,
  Tooltip,
} from "@veronica/ui";
import { useClient } from "@veronica/ui/frame";
import { Check, Copy, Pipette, Trash } from "lucide-react";
import { type CSSProperties, useCallback, useEffect, useState } from "react";
import { timeAgo } from "./format";
import type { CopyFormatOption, CopyResult, PickResult, SwatchRow } from "./types";
import "./ColorPickerPage.css";

type Message = { tone: "neutral" | "good"; text: string };

const PROFILES = [
  { value: "srgb", label: "sRGB" },
  { value: "displayP3", label: "Display P3" },
] as const;

const FORMAT_KEY = "copyFormat";
const PROFILE_KEY = "profile";

export function ColorPickerPage() {
  const client = useClient();
  const [swatches, setSwatches] = useState<SwatchRow[]>([]);
  const [formatOptions, setFormats] = useState<CopyFormatOption[] | null>(null);
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
      setSwatches(await client.backend.call<SwatchRow[]>("swatches"));
      setError(null);
      setLoadFailed(false);
    } catch (reason) {
      setError(String(reason));
      setLoadFailed(true);
    } finally {
      setLoaded(true);
    }
  }, [client]);

  useEffect(() => {
    void load();
    void client.backend
      .call<CopyFormatOption[]>("formats")
      .then(setFormats)
      .catch(() => setFormats([]));
    void Promise.all([client.settings.get(FORMAT_KEY), client.settings.get(PROFILE_KEY)])
      .then(([format, profile]) => setSettings({ [FORMAT_KEY]: format, [PROFILE_KEY]: profile }))
      .catch(() => {});
  }, [client, load]);

  useEffect(
    () =>
      client.on("visibility.changed", ({ visible }) => {
        if (visible) {
          void load();
        }
      }),
    [client, load],
  );

  const formats = formatOptions ?? [];
  const format = typeof settings[FORMAT_KEY] === "string" ? settings[FORMAT_KEY] : "hex";
  const profile = settings[PROFILE_KEY] === "displayP3" ? "displayP3" : "srgb";
  const selected = swatches.find((swatch) => swatch.id === selectedId) ?? swatches[0];

  const set = async (key: string, value: unknown) => {
    setSettings((current) => ({ ...current, [key]: value }));
    try {
      await client.settings.set(key, value);
    } catch (reason) {
      setError(String(reason));
    }
  };

  const pick = async () => {
    setPicking(true);
    setError(null);
    setMessage(null);
    try {
      const result = await client.backend.call<PickResult>("pick", {}, { timeoutMs: 120_000 });
      await load();
      setSelectedId(result.swatch.id);
      setMessage(
        result.copiedVia
          ? { tone: "good", text: `${result.value} copied via ${result.copiedVia}` }
          : { tone: "neutral", text: `${result.value} recorded but not copied` },
      );
      if (result.copyError) {
        setError(result.copyError);
      }
    } catch (reason) {
      const text = String(reason);
      if (/cancel/i.test(text)) {
        setMessage({ tone: "neutral", text: "No color picked." });
      } else {
        setError(text);
      }
    } finally {
      setPicking(false);
    }
  };

  const copy = async (swatch: SwatchRow, which?: string) => {
    try {
      const result = await client.backend.call<CopyResult>("copy", {
        id: swatch.id,
        format: which ?? null,
      });
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
      await client.backend.call("forget", { id });
      if (selectedId === id) {
        setSelectedId(null);
      }
      await load();
    } catch (reason) {
      setError(String(reason));
    }
  };

  const clear = async () => {
    setClearing(true);
    try {
      await client.backend.call("clear");
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
      actions={
        loadFailed ? (
          <Button size="small" onClick={() => void load()}>
            Retry
          </Button>
        ) : undefined
      }
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

      {!loaded && <SwatchesSkeleton />}

      {loaded && swatches.length === 0 && !loadFailed && (
        <EmptyState icon={<Pipette />} title="No colors picked yet">
          Pick a color and it is copied and kept here. The same history is behind vr color pick and
          the Pick Color action in the top bar.
        </EmptyState>
      )}

      {selected && (
        <Card className="cp-current">
          <div
            className="cp-current-preview"
            style={{ background: paint(selected) }}
            aria-hidden="true"
          />
          <div className="cp-current-body">
            <div className="cp-current-head">
              <div className="cp-current-titles">
                <p className="cp-current-hex ui-mono ui-selectable">{selected.hex.toUpperCase()}</p>
                <p className="cp-current-meta">
                  {selected.profileLabel} · picked {timeAgo(selected.pickedAt)}
                </p>
              </div>
              <Button
                size="small"
                variant="plain"
                aria-label={`Forget ${selected.hex}`}
                onClick={() => void forget(selected.id)}
              >
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
            <Button
              size="small"
              variant="destructive-plain"
              icon={<Trash />}
              onClick={() => setConfirmClear(true)}
            >
              Clear all
            </Button>
          }
        >
          <ul className="cp-grid" aria-label="Swatches">
            {swatches.map((swatch) => {
              const done = copied === `${swatch.id}:`;
              return (
                <li key={swatch.id}>
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
                      <span
                        className="cp-swatch-color"
                        style={{ "--swatch": paint(swatch) } as CSSProperties}
                      >
                        {done && (
                          <Check
                            className={
                              swatch.prefersDarkText
                                ? "cp-swatch-check cp-on-light"
                                : "cp-swatch-check cp-on-dark"
                            }
                            aria-hidden="true"
                          />
                        )}
                      </span>
                      <span className="cp-swatch-hex ui-mono">{done ? "Copied" : swatch.hex}</span>
                    </button>
                  </Tooltip>
                </li>
              );
            })}
          </ul>
        </Section>
      )}

      {(!loaded || formatOptions === null) && <PickingSkeleton />}

      {loaded && formats.length > 0 && (
        <FormGroup title="Picking">
          <FormRow
            label="Copy format"
            description="What a pick puts on the clipboard. Every format stays available per swatch."
            control={
              <Select
                value={format}
                onChange={(event) => void set(FORMAT_KEY, event.target.value)}
                aria-label="Copy format"
              >
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
            control={
              <Segmented
                options={PROFILES}
                value={profile}
                onChange={(value) => void set(PROFILE_KEY, value)}
                label="Color profile"
              />
            }
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
    <Card className="cp-current" role="status" aria-label="Reading swatches">
      <Skeleton className="cp-current-preview" height="auto" radius={8} />
      <div className="cp-current-body">
        <div className="cp-current-head">
          <div className="cp-current-titles">
            <p className="cp-current-hex">
              <SkeletonText width={132} />
            </p>
            <p className="cp-current-meta">
              <SkeletonText width={140} />
            </p>
          </div>
        </div>
        <ListSkeleton
          rows={6}
          inset={false}
          leading={0}
          subtitle={false}
          trailing={26}
          className="cp-formats"
        />
      </div>
    </Card>
  );
}

function SwatchesSkeleton() {
  return (
    <Section title="Swatches" description={<SkeletonText width={300} />}>
      <ul className="cp-grid" aria-hidden="true">
        {[0, 1, 2, 3, 4, 5, 6, 7, 8].map((slot) => (
          <li key={slot}>
            <span className="cp-swatch">
              <Skeleton width="100%" height={56} radius={8} />
              <span className="cp-swatch-hex">
                <SkeletonText width={52} />
              </span>
            </span>
          </li>
        ))}
      </ul>
    </Section>
  );
}

function PickingSkeleton() {
  return (
    <FormGroup title="Picking">
      <FormRow
        label="Copy format"
        description="What a pick puts on the clipboard. Every format stays available per swatch."
        control={<Skeleton width={132} height={32} radius={8} />}
      />
      <FormRow
        label="Color profile"
        description="The compositor samples sRGB. Display P3 converts it for a wide-gamut panel."
        control={<Skeleton width={164} height={32} radius={8} />}
      />
    </FormGroup>
  );
}

function labelFor(formats: CopyFormatOption[], id: string): string {
  return formats.find((option) => option.id === id)?.label ?? id;
}

function paint(swatch: SwatchRow): string {
  return swatch.profile === "displayP3"
    ? `color(display-p3 ${swatch.red} ${swatch.green} ${swatch.blue})`
    : swatch.hex;
}
