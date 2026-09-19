import { useCallback, useEffect, useState } from "react";

import { ipc, listenEvent } from "../lib/ipc";
import { timeAgo } from "../lib/format";
import type { CopyFormatOption, PickResult, SwatchRow } from "../lib/types";

/**
 * The colour picker and its swatch history.
 *
 * Picking opens the compositor's own eyedropper — GNOME Shell's, or the desktop
 * portal's — so Veronica never reads the screen without a click. What comes back
 * is recorded here and copied in the configured format.
 */
export function ColorPickerPage() {
  const [swatches, setSwatches] = useState<SwatchRow[]>([]);
  const [formats, setFormats] = useState<CopyFormatOption[]>([]);
  const [settings, setSettings] = useState<Record<string, unknown>>({});
  const [picking, setPicking] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [copied, setCopied] = useState<number | null>(null);
  const [expanded, setExpanded] = useState<number | null>(null);
  const [loaded, setLoaded] = useState(false);

  const load = useCallback(async () => {
    try {
      setSwatches(await ipc.colorSwatches());
      setError(null);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setLoaded(true);
    }
  }, []);

  useEffect(() => {
    void load();
    void ipc.colorFormats().then(setFormats).catch(() => {});
    void ipc.settingsAll().then(setSettings).catch(() => {});
  }, [load]);

  // A pick started from the notch or the CLI lands in the same history.
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

  const format = typeof settings.colorPickerCopyFormat === "string"
    ? settings.colorPickerCopyFormat
    : "hex";
  const profile = settings.colorPickerProfile === "displayP3" ? "displayP3" : "srgb";

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
    setNotice(null);
    try {
      const result: PickResult = await ipc.colorPick();
      await load();
      setNotice(
        result.copiedVia
          ? `${result.value} copied via ${result.copiedVia}`
          : `${result.value} recorded — ${result.copyError ?? "not copied"}`,
      );
      if (result.copyError) setError(result.copyError);
    } catch (reason) {
      // Cancelling the eyedropper is the ordinary way to back out, so it reads
      // as a note rather than a failure.
      const message = String(reason);
      if (/cancel/i.test(message)) setNotice("No colour picked.");
      else setError(message);
    } finally {
      setPicking(false);
    }
  };

  const copy = async (swatch: SwatchRow, which?: string) => {
    try {
      const result = await ipc.colorCopy(swatch.id, which ?? null);
      setCopied(swatch.id);
      setNotice(`${result.value} copied via ${result.copiedVia}`);
      setTimeout(() => setCopied((c) => (c === swatch.id ? null : c)), 1200);
      setError(null);
    } catch (reason) {
      // The clipboard needs the shell extension or wl-clipboard; say so.
      setError(String(reason));
    }
  };

  const forget = async (id: number) => {
    try {
      await ipc.colorForget(id);
      if (expanded === id) setExpanded(null);
      await load();
    } catch (reason) {
      setError(String(reason));
    }
  };

  const clear = async () => {
    if (!window.confirm(`Forget all ${swatches.length} swatches?`)) return;
    try {
      await ipc.colorClear();
      setExpanded(null);
      await load();
    } catch (reason) {
      setError(String(reason));
    }
  };

  return (
    <div className="color-page">
      <div className="page-head edith-head">
        <div>
          <h1>Color Picker</h1>
          <div className="page-sub">Sample any pixel on screen, kept as a swatch history</div>
        </div>
        <div className="head-actions">
          <button className="button primary" onClick={() => void pick()} disabled={picking}>
            {picking ? "Pick a pixel…" : "Pick a color"}
          </button>
          {swatches.length > 0 && (
            <button className="button" onClick={() => void clear()}>
              Clear all
            </button>
          )}
        </div>
      </div>

      {error && <div className="banner error">{error}</div>}
      {notice && !error && <div className="banner">{notice}</div>}
      {picking && (
        <div className="banner">
          The eyedropper is open — click any pixel, or press Escape to cancel.
        </div>
      )}

      <section className="card color-settings">
        <div className="setting-row">
          <div>
            <strong>Copy format</strong>
            <small>What a pick puts on the clipboard. Every format stays available per swatch.</small>
          </div>
          <div className="segmented">
            {formats.map((option) => (
              <button
                key={option.id}
                aria-pressed={format === option.id}
                onClick={() => void set("colorPickerCopyFormat", option.id)}
              >
                {option.label}
              </button>
            ))}
          </div>
        </div>
        <div className="setting-row">
          <div>
            <strong>Color profile</strong>
            <small>
              The compositor samples sRGB. Display P3 converts it, for a wide-gamut panel.
            </small>
          </div>
          <div className="segmented">
            {[
              { id: "srgb", label: "sRGB" },
              { id: "displayP3", label: "Display P3" },
            ].map((option) => (
              <button
                key={option.id}
                aria-pressed={profile === option.id}
                onClick={() => void set("colorPickerProfile", option.id)}
              >
                {option.label}
              </button>
            ))}
          </div>
        </div>
      </section>

      {loaded && swatches.length === 0 && (
        <div className="empty">
          <div className="empty-glyph">◐</div>
          <h3>No colors picked yet</h3>
          <p>
            Pick a color and it is copied and kept here. The same history is behind
            <code> vr color pick </code>
            and the Pick Color action in the top bar.
          </p>
        </div>
      )}

      {swatches.length > 0 && (
        <section className="card">
          <div className="card-head">
            <h2>Swatches</h2>
            <span className="card-note">
              {swatches.length} {swatches.length === 1 ? "color" : "colors"} · newest first
            </span>
          </div>
          <div className="swatch-grid">
            {swatches.map((swatch) => (
              <div className="swatch" key={swatch.id}>
                <button
                  className={`swatch-chip ${swatch.prefersDarkText ? "on-light" : "on-dark"}`}
                  style={{ background: swatch.hex }}
                  onClick={() => void copy(swatch)}
                  title={`Copy as ${format}`}
                >
                  <span className="swatch-hex">
                    {copied === swatch.id ? "Copied" : swatch.hex}
                  </span>
                </button>
                <div className="swatch-foot">
                  <button
                    className="swatch-more"
                    aria-expanded={expanded === swatch.id}
                    onClick={() => setExpanded(expanded === swatch.id ? null : swatch.id)}
                  >
                    {swatch.profileLabel} · {timeAgo(swatch.pickedAt)}
                  </button>
                  <button
                    className="swatch-forget"
                    onClick={() => void forget(swatch.id)}
                    aria-label={`Forget ${swatch.hex}`}
                    title="Forget"
                  >
                    ✕
                  </button>
                </div>
                {expanded === swatch.id && (
                  <div className="swatch-formats">
                    {formats.map((option) => (
                      <button
                        key={option.id}
                        onClick={() => void copy(swatch, option.id)}
                        title={`Copy ${option.label}`}
                      >
                        <span>{option.label}</span>
                        <code>{swatch.formats[option.id]}</code>
                      </button>
                    ))}
                  </div>
                )}
              </div>
            ))}
          </div>
        </section>
      )}
    </div>
  );
}
