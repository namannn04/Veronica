import { useCallback, useEffect, useState } from "react";

import { ipc, listenEvent } from "../lib/ipc";
import { BLUR_CATEGORIES, blurKey } from "../lib/preferences";
import type { PresenterView } from "../lib/types";
import { Badge, Button, FormGroup, FormRow, Notice, Skeleton,
  SkeletonText, Stack } from "../ui";
import { SwitchRow } from "./settingsRows";

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

  const errorNotice = error && (
    <Notice
      tone="critical"
      title={view ? "That change was not saved" : "Presenter state did not load"}
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
      <FormGroup title="Presenter">
        {[0, 1].map((slot) => (
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

  const { enabled, manual, autoEnabled, autoActive, autoPaused, autoReason, active, share } = view;
  const shareBadge = share.unavailable ? (
    <Badge>Unknown</Badge>
  ) : share.sharing ? (
    <Badge tone="warning" dot>
      Sharing
    </Badge>
  ) : (
    <Badge tone="good" dot>
      Private
    </Badge>
  );

  return (
    <Stack>
      {errorNotice}

      {active && (
        <Notice tone="neutral" title="Presenter mode is on">
          {autoReason ?? "Sensitive figures are blurred."}
        </Notice>
      )}

      <FormGroup
        title="Presenter"
        description="Blurs spend, token counts, rate limits, calendar entries and track names, while navigation and controls stay usable."
      >
        <SwitchRow
          label="Enable presenter mode"
          description={
            enabled
              ? "Available from here, the Home page's quick actions and the top bar."
              : "Off means nothing is blurred and no screen-share detection runs."
          }
          checked={enabled}
          onChange={(value) => void act(value ? "enable" : "disable")}
        />
      </FormGroup>

      {enabled && (
        <>
          <FormGroup title="Turn it on" description="By hand, or when a screen share starts.">
            <SwitchRow
              label="Blur now"
              description="Your own switch, independent of detection."
              checked={manual}
              onChange={(value) => void act(value ? "start" : "stop")}
            />
            <SwitchRow
              label="Detect screen sharing"
              description={
                share.unavailable
                  ? `Detection is unavailable here: ${share.unavailable}`
                  : "Turns on while an app is capturing the screen or controlling it remotely."
              }
              checked={autoEnabled}
              onChange={(value) => void set("presenterAutoEnabled", value)}
            />
            <FormRow
              label="What detection sees"
              description={
                share.unavailable ??
                (share.sharing
                  ? `${share.screencastSessions} screencast, ${share.remoteSessions} remote`
                  : "Nothing is capturing the screen.")
              }
              control={shareBadge}
            />
            {autoActive && (
              <FormRow
                label="This share"
                description={
                  autoPaused
                    ? "Dismissed. The next share will blur again."
                    : "Dismiss to keep working unblurred until this share ends."
                }
                control={
                  <Button size="small" onClick={() => void act(autoPaused ? "resume" : "dismiss")}>
                    {autoPaused ? "Blur again" : "Dismiss"}
                  </Button>
                }
              />
            )}
          </FormGroup>

          <FormGroup
            title="What to blur"
            description="Reveal a category on purpose: a demo may want the calendar visible while spend stays hidden."
          >
            {BLUR_CATEGORIES.map((category) => (
              <SwitchRow
                key={category.id}
                label={category.label}
                description={
                  view.blurredClasses.includes(category.css)
                    ? "Blurred right now."
                    : active
                      ? "Visible right now."
                      : "Blurred once presenter mode is on."
                }
                checked={view.categories.includes(category.id)}
                onChange={(value) => void set(blurKey(category.id), value)}
              />
            ))}
          </FormGroup>
        </>
      )}
    </Stack>
  );
}
