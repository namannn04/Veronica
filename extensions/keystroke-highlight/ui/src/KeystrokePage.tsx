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

interface KeystrokeState {
  enabled: boolean;
  active: boolean;
  durationSeconds: number;
  position: "top" | "bottom";
  maximumVisible: number;
  canApply: boolean;
}

type Update = Partial<Pick<KeystrokeState, "active" | "durationSeconds" | "position">>;

const DURATIONS = [0.5, 1, 1.5, 2, 3];
const POSITIONS = [
  { value: "top", label: "Top" },
  { value: "bottom", label: "Bottom" },
] as const;

export function KeystrokePage() {
  const client = useClient();
  const [state, setState] = useState<KeystrokeState | null>(null);
  const [error, setError] = useState("");

  const run = useCallback(
    async (method: "status" | "update", update: Update = {}) => {
      try {
        setState(await client.backend.call<KeystrokeState>(method, update));
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
          title="Keystroke Highlight"
          subtitle="A keycap for every key press, for demos and recordings"
        />
      }
    >
      <Stack>
        {error && <Notice tone="critical">{error}</Notice>}
        {state && !state.canApply && (
          <Notice tone="warning" title="Nothing reads the presses here">
            This desktop is not GNOME. A Wayland compositor hands key presses only to the focused
            window.
          </Notice>
        )}
        {state && (
          <FormGroup description="Drawn by Veronica's GNOME Shell extension. Nothing is recorded while the lock screen or a password prompt is up.">
            <FormRow
              label="Show key presses now"
              description="Pausing hides the keycaps without turning the extension off."
              control={
                <Switch
                  label="Show key presses now"
                  checked={state.active}
                  onChange={(active) => update({ active })}
                />
              }
            />
            <FormRow
              label="Duration"
              description={`How long one keycap stays. At most ${state.maximumVisible} show at once.`}
              control={
                <Select
                  aria-label="Duration"
                  value={state.durationSeconds}
                  onChange={(event) => update({ durationSeconds: Number(event.target.value) })}
                >
                  {[...new Set([...DURATIONS, state.durationSeconds])]
                    .sort((a, b) => a - b)
                    .map((seconds) => (
                      <option key={seconds} value={seconds}>
                        {seconds}s
                      </option>
                    ))}
                </Select>
              }
            />
            <FormRow
              label="Position"
              control={
                <Segmented
                  label="Position"
                  options={POSITIONS}
                  value={state.position}
                  onChange={(position) => update({ position })}
                />
              }
            />
          </FormGroup>
        )}
      </Stack>
    </Page>
  );
}
