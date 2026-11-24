import type { CSSProperties, ReactNode } from "react";

import type { Tone } from "./Feedback";
import "./Stat.css";

export function Stat({
  label,
  value,
  unit,
  note,
  tone = "neutral",
  icon,
  className,
}: {
  label: ReactNode;
  value: ReactNode;
  unit?: ReactNode;
  note?: ReactNode;
  tone?: Tone;
  icon?: ReactNode;
  className?: string;
}) {
  return (
    <div className={["ui-stat", `ui-stat-${tone}`, className].filter(Boolean).join(" ")}>
      <div className="ui-stat-label">
        {icon && <span className="ui-stat-icon">{icon}</span>}
        <span>{label}</span>
      </div>
      <div className="ui-stat-value ui-selectable">
        {value}
        {unit && <span className="ui-stat-unit">{unit}</span>}
      </div>
      {note && <div className="ui-stat-note">{note}</div>}
    </div>
  );
}

export function StatGrid({ children, min = 160 }: { children: ReactNode; min?: number }) {
  return (
    <div className="ui-stat-grid" style={{ "--stat-min": `${min}px` } as CSSProperties}>
      {children}
    </div>
  );
}
