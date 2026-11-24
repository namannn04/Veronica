import type { KeyboardEvent, ReactNode } from "react";

import "./List.css";

export function List({
  children,
  label,
  inset = true,
  className,
}: {
  children: ReactNode;
  label?: string;
  inset?: boolean;
  className?: string;
}) {
  return (
    <div role="list" aria-label={label} className={["ui-list", inset && "ui-list-inset", className].filter(Boolean).join(" ")}>
      {children}
    </div>
  );
}

export function ListRow({
  leading,
  title,
  subtitle,
  meta,
  trailing,
  onClick,
  selected,
  disabled,
  mono = false,
  className,
}: {
  leading?: ReactNode;
  title: ReactNode;
  subtitle?: ReactNode;
  meta?: ReactNode;
  trailing?: ReactNode;
  onClick?: () => void;
  selected?: boolean;
  disabled?: boolean;
  mono?: boolean;
  className?: string;
}) {
  const interactive = onClick !== undefined && !disabled;
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (!interactive) return;
    if (event.target !== event.currentTarget) return;
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      onClick?.();
    }
  };
  return (
    <div
      role="listitem"
      className={["ui-list-row", interactive && "ui-list-row-interactive", className].filter(Boolean).join(" ")}
      data-selected={selected || undefined}
      aria-disabled={disabled || undefined}
    >
      <div
        className="ui-list-row-main"
        role={interactive ? "button" : undefined}
        tabIndex={interactive ? 0 : undefined}
        aria-pressed={interactive && selected !== undefined ? selected : undefined}
        onClick={interactive ? onClick : undefined}
        onKeyDown={onKeyDown}
      >
        {leading && <span className="ui-list-row-leading">{leading}</span>}
        <span className="ui-list-row-text">
          <span className={["ui-list-row-title", mono && "ui-mono"].filter(Boolean).join(" ")}>{title}</span>
          {subtitle && <span className="ui-list-row-subtitle">{subtitle}</span>}
        </span>
        {meta && <span className="ui-list-row-meta">{meta}</span>}
      </div>
      {trailing && <div className="ui-list-row-trailing">{trailing}</div>}
    </div>
  );
}

export function KeyValueList({ items, className }: { items: { label: ReactNode; value: ReactNode; mono?: boolean }[]; className?: string }) {
  return (
    <dl className={["ui-kv", className].filter(Boolean).join(" ")}>
      {items.map((item, index) => (
        <div className="ui-kv-row" key={index}>
          <dt>{item.label}</dt>
          <dd className={item.mono ? "ui-mono ui-selectable" : "ui-selectable"}>{item.value}</dd>
        </div>
      ))}
    </dl>
  );
}
