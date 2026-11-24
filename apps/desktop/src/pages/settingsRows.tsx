import type { ReactNode } from "react";

import { FormRow, Select, Switch } from "../ui";

export function SwitchRow({
  label,
  description,
  checked,
  onChange,
  disabled,
}: {
  label: string;
  description?: ReactNode;
  checked: boolean;
  onChange: (value: boolean) => void;
  disabled?: boolean;
}) {
  return (
    <FormRow
      label={label}
      description={description}
      control={<Switch label={label} checked={checked} disabled={disabled} onChange={onChange} />}
    />
  );
}

export type NumberChoice = { value: number; label: string };

export function ChoiceRow({
  label,
  description,
  value,
  options,
  onChange,
  disabled,
  format = String,
}: {
  label: string;
  description?: ReactNode;
  value: number;
  options: NumberChoice[];
  format?: (value: number) => string;
  onChange: (value: number) => void;
  disabled?: boolean;
}) {
  const listed = options.some((option) => option.value === value)
    ? options
    : [...options, { value, label: format(value) }].sort((a, b) => a.value - b.value);
  return (
    <FormRow
      label={label}
      description={description}
      control={
        <Select
          aria-label={label}
          value={String(value)}
          disabled={disabled}
          onChange={(event) => onChange(Number(event.target.value))}
        >
          {listed.map((option) => (
            <option key={option.value} value={option.value}>
              {option.label}
            </option>
          ))}
        </Select>
      }
    />
  );
}
