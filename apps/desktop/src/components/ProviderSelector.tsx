import {
  LIMIT_PROVIDERS,
  type LimitProvider,
} from "../lib/preferences";

export function ProviderSelector({
  value,
  onChange,
  compact = false,
}: {
  value: LimitProvider;
  onChange: (provider: LimitProvider) => void;
  compact?: boolean;
}) {
  return (
    <div className={`provider-selector${compact ? " compact" : ""}`} aria-label="Rate limit provider">
      {LIMIT_PROVIDERS.map((provider) => (
        <button
          key={provider}
          aria-pressed={value === provider}
          onClick={() => onChange(provider)}
        >
          <span aria-hidden="true">{provider === "Claude" ? "C" : "⌘"}</span>
          {provider}
        </button>
      ))}
    </div>
  );
}
