import type { CSSProperties, InputHTMLAttributes } from "react";

import "./Slider.css";

export function Slider({
  value,
  onChange,
  min = 0,
  max = 100,
  step = 1,
  label,
  disabled,
  className,
  ...rest
}: Omit<InputHTMLAttributes<HTMLInputElement>, "value" | "onChange" | "type" | "min" | "max" | "step"> & {
  value: number;
  onChange: (value: number) => void;
  min?: number;
  max?: number;
  step?: number;
  label: string;
}) {
  const fraction = max > min ? (Math.min(max, Math.max(min, value)) - min) / (max - min) : 0;
  return (
    <input
      type="range"
      className={["ui-slider", className].filter(Boolean).join(" ")}
      style={{ "--slider-fill": `${fraction * 100}%` } as CSSProperties}
      value={value}
      min={min}
      max={max}
      step={step}
      aria-label={label}
      disabled={disabled}
      onChange={(event) => onChange(Number(event.target.value))}
      {...rest}
    />
  );
}
