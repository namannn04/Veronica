import { FormGroup, FormRow, Page, PageHeader, Select } from "@veronica/ui";
import { useCallback, useEffect, useState } from "react";
import { AlertsPane } from "./AlertsPane";
import { errorText, ipc } from "./api";
import { ProviderSelector } from "./ProviderSelector";
import { type LimitProvider, limitProviderOf, storedLimitProvider } from "./providers";

const USAGE_REFRESH_CHOICES: [number, string][] = [
  [5, "Every 5 minutes"],
  [15, "Every 15 minutes"],
  [30, "Every 30 minutes"],
  [60, "Every hour"],
  [0, "Only when I press Refresh"],
];

function usageRefreshMinutes(value: unknown): number {
  const minutes = typeof value === "number" ? value : 15;
  return USAGE_REFRESH_CHOICES.some(([choice]) => choice === minutes) ? minutes : 15;
}

function Collection() {
  const [values, setValues] = useState<Record<string, unknown>>({});
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setValues(await ipc.settingsAll());
      setError(null);
    } catch (reason) {
      setError(errorText(reason));
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const set = async (key: string, value: unknown) => {
    try {
      await ipc.settingsSet(key, value);
      await load();
    } catch (reason) {
      setError(errorText(reason));
    }
  };

  return (
    <FormGroup
      title="Rate limits"
      description={error ?? "Which provider Home and the notch show by default."}
    >
      <FormRow
        label="Visible provider"
        description="Switch it here, on the Usage page, or on the Home card."
        control={
          <ProviderSelector
            value={limitProviderOf(values.limitsProvider)}
            onChange={(provider: LimitProvider) =>
              void set("limitsProvider", storedLimitProvider(provider))
            }
          />
        }
      />
      <FormRow
        label="Collect usage"
        description="How often Veronica reads your agents' history for Home, Agent Usage and the notch."
        control={
          <Select
            aria-label="Collect usage"
            value={String(usageRefreshMinutes(values.usageRefreshMinutes))}
            onChange={(event) => void set("usageRefreshMinutes", Number(event.target.value))}
          >
            {USAGE_REFRESH_CHOICES.map(([minutes, label]) => (
              <option key={minutes} value={minutes}>
                {label}
              </option>
            ))}
          </Select>
        }
      />
    </FormGroup>
  );
}

export function AlertsPage() {
  return (
    <Page
      header={
        <PageHeader title="Usage alerts" subtitle="Rate-limit alerts, pacing and reset reminders" />
      }
    >
      <Collection />
      <AlertsPane />
    </Page>
  );
}
