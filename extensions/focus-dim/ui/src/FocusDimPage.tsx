import {
  FormGroup,
  FormRow,
  Notice,
  Page,
  PageHeader,
  Segmented,
  Select,
  Stack,
  Switch,
} from "@veronica/ui";
import { useClient } from "@veronica/ui/frame";
import { useCallback, useEffect, useState } from "react";

interface FocusDimState {
  enabled: boolean;
  intensityPercent: number;
  fadeSeconds: number;
  mode: "perScreenFront" | "dimUnfocused";
  canApply: boolean;
}

type Update = Partial<Pick<FocusDimState, "enabled" | "intensityPercent" | "fadeSeconds" | "mode">>;

const INTENSITIES = [15, 30, 45, 60, 75, 90];
const FADES = [
  { value: 0.05, label: "Instant" },
  { value: 0.15, label: "Quick" },
  { value: 0.25, label: "Normal" },
  { value: 0.5, label: "Slow" },
];
const MODES = [
  { value: "perScreenFront", label: "Per display" },
  { value: "dimUnfocused", label: "Focused only" },
] as const;

export function FocusDimPage() {
  const client = useClient();
  const [state, setState] = useState<FocusDimState | null>(null);
  const [error, setError] = useState("");

  const run = useCallback(
    async (method: "status" | "update", update: Update = {}) => {
      try {
        setState(await client.backend.call<FocusDimState>(method, update));
        setError("");
      } catch (reason) {
        setError(String(reason));
      }
    },
    [client],
  );

  useEffect(() => {
    void run("status");
  }, [run]);

  const update = (change: Update) => void run("update", change);

  return (
    <Page
      width="form"
      header={
        <PageHeader
          title="Focus Dim"
          subtitle="Darkens everything behind the window you are working in"
        />
      }
    >
      <Stack>
        {error && <Notice tone="critical">{error}</Notice>}
        {state && !state.canApply && (
          <Notice tone="warning" title="Nothing draws the overlay here">
            This desktop is not GNOME. Only the compositor can place a dim behind another app's
            window.
          </Notice>
        )}
        {state && (
          <FormGroup description="Drawn by Veronica's GNOME Shell extension.">
            <FormRow
              label="Dim behind the focused window"
              description="Takes effect immediately."
              control={
                <Switch
                  label="Dim behind the focused window"
                  checked={state.enabled}
                  onChange={(enabled) => update({ enabled })}
                />
              }
            />
            <FormRow
              label="Intensity"
              description="Capped below opaque, so the desktop never disappears."
              control={
                <Select
                  aria-label="Intensity"
                  value={state.intensityPercent}
                  onChange={(event) => update({ intensityPercent: Number(event.target.value) })}
                >
                  {[...new Set([...INTENSITIES, state.intensityPercent])]
                    .sort((a, b) => a - b)
                    .map((percent) => (
                      <option key={percent} value={percent}>
                        {percent}%
                      </option>
                    ))}
                </Select>
              }
            />
            <FormRow
              label="Fade"
              description="How long the change takes as focus moves."
              control={
                <Select
                  aria-label="Fade"
                  value={state.fadeSeconds}
                  onChange={(event) => update({ fadeSeconds: Number(event.target.value) })}
                >
                  {FADES.map((fade) => (
                    <option key={fade.value} value={fade.value}>
                      {fade.label}
                    </option>
                  ))}
                </Select>
              }
            />
            <FormRow
              label="Other displays"
              control={
                <Segmented
                  label="Other displays"
                  options={MODES}
                  value={state.mode}
                  onChange={(mode) => update({ mode })}
                />
              }
            />
          </FormGroup>
        )}
      </Stack>
    </Page>
  );
}
