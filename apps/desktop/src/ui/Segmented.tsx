import type { ReactNode } from "react";

import { Tooltip } from "./Tooltip";
import "./Segmented.css";

export type SegmentedOption<T extends string> = {
  value: T;
  label: string;
  icon?: ReactNode;
  disabled?: boolean;
};

export function Segmented<T extends string>({
  options,
  value,
  onChange,
  label,
  size = "regular",
  iconOnly = false,
  stretch = false,
}: {
  options: readonly SegmentedOption<T>[];
  value: T;
  onChange: (value: T) => void;
  label: string;
  size?: "regular" | "small";
  iconOnly?: boolean;
  stretch?: boolean;
}) {
  return (
    <div
      className={["ui-segmented", `ui-segmented-${size}`, stretch && "ui-segmented-stretch"].filter(Boolean).join(" ")}
      role="radiogroup"
      aria-label={label}
      style={{ gridTemplateColumns: `repeat(${options.length}, minmax(0, 1fr))` }}
    >
      {options.map((option) => {
        const segment = (
          <button
            key={option.value}
            type="button"
            role="radio"
            aria-checked={option.value === value}
            aria-label={iconOnly ? option.label : undefined}
            disabled={option.disabled}
            className="ui-segment"
            onClick={() => onChange(option.value)}
          >
            {option.icon}
            {!iconOnly && <span>{option.label}</span>}
          </button>
        );
        return iconOnly ? (
          <Tooltip key={option.value} content={option.label}>
            {segment}
          </Tooltip>
        ) : (
          segment
        );
      })}
    </div>
  );
}
