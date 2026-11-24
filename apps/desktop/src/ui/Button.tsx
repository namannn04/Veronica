import { forwardRef, type ButtonHTMLAttributes, type ReactNode } from "react";

import { Tooltip } from "./Tooltip";
import "./Button.css";

export type ButtonVariant = "primary" | "secondary" | "plain" | "destructive" | "destructive-plain";
export type ButtonSize = "regular" | "small";

type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: ButtonVariant;
  size?: ButtonSize;
  icon?: ReactNode;
  trailingIcon?: ReactNode;
  busy?: boolean;
};

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  { variant = "secondary", size = "regular", icon, trailingIcon, busy = false, className, children, type = "button", disabled, ...rest },
  ref,
) {
  return (
    <button
      ref={ref}
      type={type}
      className={["ui-button", `ui-button-${variant}`, `ui-button-${size}`, className].filter(Boolean).join(" ")}
      disabled={disabled || busy}
      aria-busy={busy || undefined}
      {...rest}
    >
      {busy ? <span className="ui-button-spinner" aria-hidden="true" /> : icon && <span className="ui-button-icon">{icon}</span>}
      {children !== undefined && children !== null && <span className="ui-button-label">{children}</span>}
      {trailingIcon && <span className="ui-button-icon">{trailingIcon}</span>}
    </button>
  );
});

type IconButtonProps = Omit<ButtonHTMLAttributes<HTMLButtonElement>, "children"> & {
  label: string;
  icon: ReactNode;
  size?: ButtonSize;
  variant?: "plain" | "secondary";
  pressed?: boolean;
  tooltip?: string;
};

export const IconButton = forwardRef<HTMLButtonElement, IconButtonProps>(function IconButton(
  { label, icon, size = "regular", variant = "plain", pressed, tooltip, className, type = "button", ...rest },
  ref,
) {
  return (
    <Tooltip content={tooltip ?? label}>
      <button
        ref={ref}
        type={type}
        aria-label={label}
        aria-pressed={pressed}
        className={["ui-icon-button", `ui-icon-button-${variant}`, `ui-icon-button-${size}`, className].filter(Boolean).join(" ")}
        {...rest}
      >
        {icon}
      </button>
    </Tooltip>
  );
});

export function ButtonGroup({ children, className }: { children: ReactNode; className?: string }) {
  return <div className={["ui-button-group", className].filter(Boolean).join(" ")}>{children}</div>;
}
