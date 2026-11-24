import type { ReactNode } from "react";

import "./Toolbar.css";

export function Toolbar({
  leading,
  center,
  trailing,
  className,
}: {
  leading?: ReactNode;
  center?: ReactNode;
  trailing?: ReactNode;
  className?: string;
}) {
  return (
    <div className={["ui-toolbar", className].filter(Boolean).join(" ")} role="toolbar">
      {leading && <div className="ui-toolbar-group ui-toolbar-leading">{leading}</div>}
      {center && <div className="ui-toolbar-group ui-toolbar-center">{center}</div>}
      {trailing && <div className="ui-toolbar-group ui-toolbar-trailing">{trailing}</div>}
    </div>
  );
}

export function Table({
  children,
  className,
  label,
}: {
  children: ReactNode;
  className?: string;
  label?: string;
}) {
  return (
    <div className={["ui-table-wrap", className].filter(Boolean).join(" ")}>
      <table className="ui-table" aria-label={label}>
        {children}
      </table>
    </div>
  );
}
