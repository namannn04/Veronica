import type { ReactNode } from "react";

import { occurrenceKeys } from "./keys";

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
    <ul
      aria-label={label}
      className={["ui-list", inset && "ui-list-inset", className].filter(Boolean).join(" ")}
    >
      {children}
    </ul>
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
  const content = (
    <>
      {leading && <span className="ui-list-row-leading">{leading}</span>}
      <span className="ui-list-row-text">
        <span className={["ui-list-row-title", mono && "ui-mono"].filter(Boolean).join(" ")}>
          {title}
        </span>
        {subtitle && <span className="ui-list-row-subtitle">{subtitle}</span>}
      </span>
      {meta && <span className="ui-list-row-meta">{meta}</span>}
    </>
  );
  return (
    <li
      className={["ui-list-row", interactive && "ui-list-row-interactive", className]
        .filter(Boolean)
        .join(" ")}
      data-selected={selected || undefined}
      aria-disabled={disabled || undefined}
    >
      {interactive ? (
        <button
          type="button"
          className="ui-list-row-main"
          aria-pressed={selected}
          onClick={onClick}
        >
          {content}
        </button>
      ) : (
        <div className="ui-list-row-main">{content}</div>
      )}
      {trailing && <div className="ui-list-row-trailing">{trailing}</div>}
    </li>
  );
}

export function KeyValueList({
  items,
  className,
}: {
  items: { label: string; value: ReactNode; mono?: boolean }[];
  className?: string;
}) {
  const keys = occurrenceKeys(items.map((item) => item.label));
  const rows = items.map((item, position) => ({ ...item, key: keys[position] }));
  return (
    <dl className={["ui-kv", className].filter(Boolean).join(" ")}>
      {rows.map((item) => (
        <div className="ui-kv-row" key={item.key}>
          <dt>{item.label}</dt>
          <dd className={item.mono ? "ui-mono ui-selectable" : "ui-selectable"}>{item.value}</dd>
        </div>
      ))}
    </dl>
  );
}
