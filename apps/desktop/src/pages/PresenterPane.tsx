import { useCallback, useEffect, useState } from "react";

import { ipc, listenEvent } from "../lib/ipc";
import { BLUR_CATEGORIES, blurKey } from "../lib/preferences";
import type { PresenterView } from "../lib/types";

/**
 * Presenter mode.
 *
 * Three switches rather than one, because they answer different questions:
 * whether the feature exists at all, whether the user asked for it, and whether
 * Veronica noticed a screen share. Collapsing them would mean a detected share
 * could not be dismissed without turning the feature off.
 */
export function PresenterPane() {
  const [view, setView] = useState<PresenterView | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setView(await ipc.presenterState());
      setError(null);
    } catch (reason) {
      setError(String(reason));
    }
  }, []);

  useEffect(() => {
    void load();
    // The detector writes its findings to the settings, so this is how a share
    // starting mid-session reaches the screen.
    const changed = listenEvent("settings-updated", () => void load());
    return () => {
      void changed.then((un) => un());
    };
  }, [load]);

  const act = async (action: string) => {
    try {
      await ipc.presenterSet(action);
      await load();
    } catch (reason) {
      setError(String(reason));
    }
  };

  const set = async (key: string, value: unknown) => {
    try {
      await ipc.settingsSet(key, value);
      await load();
    } catch (reason) {
      setError(String(reason));
    }
  };

  if (!view) {
    return (
      <section className="settings-info">
        <div className="info-glyph">◯</div>
        <h2>Presenter</h2>
        <p>{error ?? "Reading the presenter state…"}</p>
      </section>
    );
  }

  const { enabled, manual, autoEnabled, autoActive, autoPaused, autoReason, active } = view;

  return (
    <div className="settings-stack presenter-pane">
      {error && <div className="banner error">{error}</div>}

      {active && (
        <div className="banner presenter-live">
          <strong>Presenter mode is on.</strong>{" "}
          {autoReason ?? "Sensitive figures are blurred."}
        </div>
      )}

      <Group
        title="Presenter"
        subtitle="Blurs spend, token counts, rate limits, calendar entries and track names, while leaving navigation and controls usable."
      >
        <Row
          label="Enable presenter mode"
          detail={
            enabled
              ? "Available from here, the Home page's quick actions and the top bar."
              : "Off means nothing is blurred and no screen-share detection runs."
          }
        >
          <Switch
            label="Enable presenter mode"
            checked={enabled}
            onChange={(value) => void act(value ? "enable" : "disable")}
          />
        </Row>
      </Group>

      {enabled && (
        <>
          <Group title="Turn it on" subtitle="By hand, or when a screen share starts.">
            <Row label="Blur now" detail="Your own switch. Independent of detection.">
              <Switch
                label="Blur now"
                checked={manual}
                onChange={(value) => void act(value ? "start" : "stop")}
              />
            </Row>
            <Row
              label="Detect screen sharing"
              detail={
                view.share.unavailable
                  ? `Detection is unavailable here: ${view.share.unavailable}`
                  : "Activates automatically while an app is capturing the screen or controlling it remotely."
              }
            >
              <Switch
                label="Detect screen sharing"
                checked={autoEnabled}
                onChange={(value) => void set("presenterAutoEnabled", value)}
              />
            </Row>
            <Row
              label="What detection sees"
              detail={
                view.share.unavailable ??
                (view.share.sharing
                  ? `${view.share.screencastSessions} screencast, ${view.share.remoteSessions} remote`
                  : "Nothing is capturing the screen.")
              }
            >
              <span className={`pill ${view.share.sharing ? "warn" : "good"}`}>
                {view.share.unavailable
                  ? "unknown"
                  : view.share.sharing
                    ? "sharing"
                    : "private"}
              </span>
            </Row>
            {autoActive && (
              <Row
                label="This share"
                detail={
                  autoPaused
                    ? "Dismissed. The next share will blur again."
                    : "Dismiss to keep working unblurred until this share ends."
                }
              >
                <button
                  className="button"
                  onClick={() => void act(autoPaused ? "resume" : "dismiss")}
                >
                  {autoPaused ? "Blur again" : "Dismiss"}
                </button>
              </Row>
            )}
          </Group>

          <Group
            title="What to blur"
            subtitle="Reveal a category deliberately — a demo may want the calendar visible while spend stays hidden."
          >
            {BLUR_CATEGORIES.map((category) => (
              <Row
                key={category.id}
                label={category.label}
                detail={
                  view.blurredClasses.includes(category.css)
                    ? "Blurred right now."
                    : active
                      ? "Visible right now."
                      : "Blurred once presenter mode is on."
                }
              >
                <Switch
                  label={category.label}
                  checked={view.categories.includes(category.id)}
                  onChange={(value) => void set(blurKey(category.id), value)}
                />
              </Row>
            ))}
          </Group>
        </>
      )}
    </div>
  );
}

function Group({
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

function Row({
  label,
  detail,
  children,
}: {
  label: string;
  detail: string;
  children: React.ReactNode;
}) {
  return (
    <div className="setting-row">
      <div>
        <strong>{label}</strong>
        <small>{detail}</small>
      </div>
      {children}
    </div>
  );
}

function Switch({
  label,
  checked,
  onChange,
}: {
  label: string;
  checked: boolean;
  onChange: (value: boolean) => void;
}) {
  return (
    <button
      className="switch"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      onClick={() => onChange(!checked)}
    >
      <span className="knob" />
    </button>
  );
}
