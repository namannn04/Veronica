import { Segmented } from "../ui";
import { LIMIT_PROVIDERS, type LimitProvider } from "../lib/preferences";

const OPTIONS = LIMIT_PROVIDERS.map((provider) => ({ value: provider, label: provider }));

export function ProviderSelector({
  value,
  onChange,
  compact = false,
}: {
  value: LimitProvider;
  onChange: (provider: LimitProvider) => void;
  compact?: boolean;
}) {
  return <Segmented options={OPTIONS} value={value} onChange={onChange} label="Rate limit provider" size={compact ? "small" : "regular"} />;
}
