import type { ReactNode } from "react";

import "./FilterChips.css";

export type FilterChip<T extends string> = { value: T; label: string; leading?: ReactNode };

export function FilterChips<T extends string>({
  chips,
  selected,
  onToggle,
  onClear,
  allLabel = "All",
  label,
}: {
  chips: readonly FilterChip<T>[];
  selected: readonly T[];
  onToggle: (value: T) => void;
  onClear: () => void;
  allLabel?: string;
  label: string;
}) {
  return (
    <div className="ui-filter-chips" role="group" aria-label={label}>
      <button type="button" className="ui-filter-chip" aria-pressed={selected.length === 0} onClick={onClear}>
        {allLabel}
      </button>
      {chips.map((chip) => (
        <button
          key={chip.value}
          type="button"
          className="ui-filter-chip"
          aria-pressed={selected.includes(chip.value)}
          onClick={() => onToggle(chip.value)}
        >
          {chip.leading && <span className="ui-filter-chip-leading">{chip.leading}</span>}
          {chip.label}
        </button>
      ))}
    </div>
  );
}
