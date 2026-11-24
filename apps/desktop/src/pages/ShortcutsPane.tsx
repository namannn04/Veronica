import { type FormEvent, type ReactNode, useCallback, useEffect, useState } from "react";

import { ipc, listenEvent } from "../lib/ipc";
import type { PlannedShortcut, ShortcutState } from "../lib/types";
import {
  Badge,
  Button,
  ButtonGroup,
  FormGroup,
  FormRow,
  Kbd,
  Notice,
  TextField,
  type Tone,
} from "../ui";

const OVERRIDES_KEY = "shortcutOverrides";

const DETAILS: Record<string, string> = {
  "veronica/show": "Open, unminimise and focus the app.",
  "veronica/cleanKeys": "Take a compositor grab until you click Done.",
  "clipboard/open": "Open the notch clipboard, or the app as a fallback.",
  "micMute/toggle": "Toggle the default PipeWire microphone system-wide.",
  "colorPicker/pick": "Open GNOME's eyedropper and save the swatch.",
  "emoji/open": "Open the picker. Clicking an emoji copies it.",
};

const STATES: Record<Exclude<ShortcutState, "active">, { label: string; tone: Tone }> = {
  unbound: { label: "Off", tone: "neutral" },
  invalid: { label: "Invalid", tone: "critical" },
  conflict: { label: "Conflict", tone: "warning" },
  unavailable: { label: "Taken", tone: "warning" },
};

type Overrides = Record<string, string | null>;

function keyOf(shortcut: PlannedShortcut): string {
  return `${shortcut.owner}/${shortcut.id}`;
}

function describeShortcut(shortcut: PlannedShortcut): string {
  const detail = DETAILS[keyOf(shortcut)] ?? `From the ${shortcut.owner} extension.`;
  switch (shortcut.state) {
    case "conflict":
      return `${shortcut.binding} is already used by ${shortcut.conflictsWith}. Choose another key.`;
    case "invalid":
      return "The saved key is not a valid shortcut. Choose another or reset it.";
    case "unavailable":
      return `Another app holds ${shortcut.binding}. Choose another key.`;
    case "unbound":
      return shortcut.overridden ? `Turned off. ${detail}` : `No key assigned. ${detail}`;
    default:
      return detail;
  }
}

function overridesOf(value: unknown): Overrides {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    return {};
  }
  return Object.fromEntries(
    Object.entries(value).filter(
      (entry): entry is [string, string | null] =>
        typeof entry[1] === "string" || entry[1] === null,
    ),
  );
}

export function ShortcutsPane({ footer }: { footer?: ReactNode }) {
  const [shortcuts, setShortcuts] = useState<PlannedShortcut[] | null>(null);
  const [overrides, setOverrides] = useState<Overrides>({});
  const [editing, setEditing] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(() => {
    void Promise.all([ipc.shortcutsList(), ipc.settingsAll()])
      .then(([planned, stored]) => {
        setShortcuts(planned);
        setOverrides(overridesOf(stored[OVERRIDES_KEY]));
      })
      .catch((reason) => setError(String(reason)));
  }, []);

  useEffect(() => {
    refresh();
    const updated = listenEvent("shortcuts-updated", refresh);
    return () => {
      void updated.then((unlisten) => unlisten());
    };
  }, [refresh]);

  const save = async (key: string, binding: string | null | undefined) => {
    const next = { ...overrides };
    if (binding === undefined) {
      delete next[key];
    } else {
      next[key] = binding;
    }
    try {
      await ipc.settingsSet(OVERRIDES_KEY, next);
      setOverrides(next);
      setEditing(null);
      setError(null);
      refresh();
    } catch (reason) {
      setError(String(reason));
    }
  };

  const submit = (event: FormEvent, key: string) => {
    event.preventDefault();
    const binding = draft.trim();
    if (binding) {
      void save(key, binding);
    }
  };

  return (
    <FormGroup
      title="Global shortcuts"
      description="Built-in features and every extension that is on can add one. Type a key such as Ctrl+Alt+J to change it."
    >
      {error && (
        <Notice tone="critical" title="Shortcuts did not save">
          {error}
        </Notice>
      )}
      {shortcuts?.map((shortcut) => {
        const key = keyOf(shortcut);
        if (editing === key) {
          return (
            <form key={key} onSubmit={(event) => submit(event, key)}>
              <FormRow
                label={shortcut.title}
                description="Use Ctrl, Alt or Super with one key."
                control={
                  <ButtonGroup>
                    <TextField
                      size="small"
                      mono
                      aria-label={`New key for ${shortcut.title}`}
                      placeholder="Ctrl+Alt+J"
                      value={draft}
                      onChange={(event) => setDraft(event.target.value)}
                    />
                    <Button size="small" type="submit" disabled={!draft.trim()}>
                      Save
                    </Button>
                    <Button size="small" variant="plain" onClick={() => setEditing(null)}>
                      Cancel
                    </Button>
                  </ButtonGroup>
                }
              />
            </form>
          );
        }
        const state = shortcut.state === "active" ? null : STATES[shortcut.state];
        return (
          <FormRow
            key={key}
            label={shortcut.title}
            description={describeShortcut(shortcut)}
            control={
              <ButtonGroup>
                {state ? (
                  <Badge tone={state.tone}>{state.label}</Badge>
                ) : (
                  <KeyCombo keys={(shortcut.binding ?? "").split("+")} />
                )}
                <Button
                  size="small"
                  variant="plain"
                  onClick={() => {
                    setDraft(shortcut.binding ?? "");
                    setEditing(key);
                  }}
                >
                  Change
                </Button>
                {shortcut.state !== "unbound" && (
                  <Button size="small" variant="plain" onClick={() => void save(key, null)}>
                    Turn off
                  </Button>
                )}
                {key in overrides && (
                  <Button size="small" variant="plain" onClick={() => void save(key, undefined)}>
                    Reset
                  </Button>
                )}
              </ButtonGroup>
            }
          />
        );
      })}
      {footer}
    </FormGroup>
  );
}

function KeyCombo({ keys }: { keys: string[] }) {
  return (
    <span className="settings-keys" role="img" aria-label={keys.join(" + ")}>
      {keys.map((key) => (
        <Kbd key={key}>{key}</Kbd>
      ))}
    </span>
  );
}
