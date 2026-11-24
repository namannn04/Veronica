import {
  forwardRef,
  useId,
  type InputHTMLAttributes,
  type ReactNode,
  type SelectHTMLAttributes,
  type TextareaHTMLAttributes,
} from "react";
import { ChevronDown, Search, X } from "lucide-react";

import "./Form.css";

export function Switch({
  checked,
  onChange,
  label,
  disabled,
  size = "regular",
}: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: string;
  disabled?: boolean;
  size?: "regular" | "small";
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      className={`ui-switch ui-switch-${size}`}
      onClick={() => onChange(!checked)}
    >
      <span className="ui-switch-thumb" />
    </button>
  );
}

export function Checkbox({
  checked,
  onChange,
  label,
  description,
  disabled,
}: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: ReactNode;
  description?: ReactNode;
  disabled?: boolean;
}) {
  return (
    <label className="ui-checkbox" aria-disabled={disabled || undefined}>
      <input type="checkbox" checked={checked} disabled={disabled} onChange={(event) => onChange(event.target.checked)} />
      <span className="ui-checkbox-text">
        <span className="ui-checkbox-label">{label}</span>
        {description && <span className="ui-checkbox-description">{description}</span>}
      </span>
    </label>
  );
}

type TextFieldProps = InputHTMLAttributes<HTMLInputElement> & {
  leading?: ReactNode;
  trailing?: ReactNode;
  mono?: boolean;
  size?: "regular" | "small";
};

export const TextField = forwardRef<HTMLInputElement, Omit<TextFieldProps, "size"> & { size?: "regular" | "small" }>(function TextField(
  { leading, trailing, mono = false, size = "regular", className, ...rest },
  ref,
) {
  return (
    <span className={["ui-field", `ui-field-${size}`, className].filter(Boolean).join(" ")}>
      {leading && <span className="ui-field-leading">{leading}</span>}
      <input ref={ref} className={mono ? "ui-field-input ui-mono" : "ui-field-input"} spellCheck={false} autoComplete="off" {...rest} />
      {trailing && <span className="ui-field-trailing">{trailing}</span>}
    </span>
  );
});

export const SearchField = forwardRef<
  HTMLInputElement,
  Omit<InputHTMLAttributes<HTMLInputElement>, "onChange" | "value" | "size"> & {
    value: string;
    onChange: (value: string) => void;
    size?: "regular" | "small";
  }
>(function SearchField({ value, onChange, placeholder = "Search", size = "regular", className, onKeyDown, ...rest }, ref) {
  return (
    <TextField
      ref={ref}
      type="search"
      size={size}
      className={["ui-search", className].filter(Boolean).join(" ")}
      value={value}
      placeholder={placeholder}
      aria-label={rest["aria-label"] ?? placeholder}
      leading={<Search aria-hidden="true" />}
      trailing={
        value ? (
          <button type="button" className="ui-field-clear" aria-label="Clear search" onClick={() => onChange("")}>
            <X aria-hidden="true" />
          </button>
        ) : undefined
      }
      onChange={(event) => onChange(event.target.value)}
      onKeyDown={(event) => {
        if (event.key === "Escape" && value) {
          event.preventDefault();
          event.stopPropagation();
          onChange("");
        } else if (event.key === "Escape") {
          event.currentTarget.blur();
        }
        onKeyDown?.(event);
      }}
      {...rest}
    />
  );
});

export const Select = forwardRef<
  HTMLSelectElement,
  Omit<SelectHTMLAttributes<HTMLSelectElement>, "size"> & { size?: "regular" | "small"; variant?: "field" | "chip" }
>(function Select({ size = "regular", variant = "field", className, children, ...rest }, ref) {
  return (
    <span className={["ui-select", `ui-select-${size}`, `ui-select-${variant}`, className].filter(Boolean).join(" ")}>
      <select ref={ref} {...rest}>
        {children}
      </select>
      <ChevronDown aria-hidden="true" />
    </span>
  );
});

export const TextArea = forwardRef<HTMLTextAreaElement, TextareaHTMLAttributes<HTMLTextAreaElement> & { mono?: boolean }>(function TextArea(
  { mono = false, className, ...rest },
  ref,
) {
  return <textarea ref={ref} className={["ui-textarea", mono && "ui-mono", className].filter(Boolean).join(" ")} {...rest} />;
});

export function Field({
  label,
  hint,
  error,
  children,
}: {
  label: ReactNode;
  hint?: ReactNode;
  error?: ReactNode;
  children: (id: string) => ReactNode;
}) {
  const id = useId();
  return (
    <div className="ui-labeled-field">
      <label className="ui-labeled-field-label" htmlFor={id}>
        {label}
      </label>
      {children(id)}
      {error ? <p className="ui-labeled-field-error">{error}</p> : hint && <p className="ui-labeled-field-hint">{hint}</p>}
    </div>
  );
}

export function FormGroup({
  title,
  description,
  children,
  footer,
}: {
  title?: ReactNode;
  description?: ReactNode;
  children: ReactNode;
  footer?: ReactNode;
}) {
  return (
    <section className="ui-form-group">
      {(title || description) && (
        <div className="ui-form-group-head">
          {title && <h2 className="ui-form-group-title">{title}</h2>}
          {description && <p className="ui-form-group-description">{description}</p>}
        </div>
      )}
      <div className="ui-form-group-body">{children}</div>
      {footer && <p className="ui-form-group-footer">{footer}</p>}
    </section>
  );
}

export function FormRow({
  label,
  description,
  control,
  children,
  stacked = false,
}: {
  label: ReactNode;
  description?: ReactNode;
  control?: ReactNode;
  children?: ReactNode;
  stacked?: boolean;
}) {
  return (
    <div className={["ui-form-row", stacked && "ui-form-row-stacked"].filter(Boolean).join(" ")}>
      <div className="ui-form-row-text">
        <div className="ui-form-row-label">{label}</div>
        {description && <div className="ui-form-row-description">{description}</div>}
      </div>
      {control && <div className="ui-form-row-control">{control}</div>}
      {children && <div className="ui-form-row-content">{children}</div>}
    </div>
  );
}
