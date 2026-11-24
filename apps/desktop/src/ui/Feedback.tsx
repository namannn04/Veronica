import type { CSSProperties, ReactNode } from "react";
import { AlertTriangle, CheckCircle2, Info, OctagonAlert } from "lucide-react";

import "./Feedback.css";

export type Tone = "neutral" | "accent" | "good" | "warning" | "critical";

export function Badge({ tone = "neutral", children, dot = false }: { tone?: Tone; children: ReactNode; dot?: boolean }) {
  return (
    <span className={`ui-badge ui-badge-${tone}`}>
      {dot && <span className="ui-badge-dot" aria-hidden="true" />}
      {children}
    </span>
  );
}

export function StatusDot({ tone = "neutral", label }: { tone?: Tone; label?: string }) {
  return <span className={`ui-status-dot ui-status-dot-${tone}`} role={label ? "img" : undefined} aria-label={label} aria-hidden={label ? undefined : true} />;
}

const NOTICE_ICONS: Record<Exclude<Tone, "accent">, ReactNode> = {
  neutral: <Info />,
  good: <CheckCircle2 />,
  warning: <AlertTriangle />,
  critical: <OctagonAlert />,
};

export function Notice({
  tone = "neutral",
  title,
  children,
  actions,
  compact = false,
}: {
  tone?: Exclude<Tone, "accent">;
  title?: ReactNode;
  children?: ReactNode;
  actions?: ReactNode;
  compact?: boolean;
}) {
  return (
    <div className={["ui-notice", `ui-notice-${tone}`, compact && "ui-notice-compact"].filter(Boolean).join(" ")} role={tone === "critical" ? "alert" : "status"}>
      <span className="ui-notice-icon" aria-hidden="true">{NOTICE_ICONS[tone]}</span>
      <div className="ui-notice-body">
        {title && <p className="ui-notice-title">{title}</p>}
        {children && <div className="ui-notice-message">{children}</div>}
      </div>
      {actions && <div className="ui-notice-actions">{actions}</div>}
    </div>
  );
}

export function EmptyState({
  icon,
  title,
  children,
  action,
  size = "regular",
}: {
  icon?: ReactNode;
  title: ReactNode;
  children?: ReactNode;
  action?: ReactNode;
  size?: "regular" | "compact";
}) {
  return (
    <div className={`ui-empty ui-empty-${size}`}>
      {icon && <span className="ui-empty-icon" aria-hidden="true">{icon}</span>}
      <p className="ui-empty-title">{title}</p>
      {children && <p className="ui-empty-message">{children}</p>}
      {action && <div className="ui-empty-action">{action}</div>}
    </div>
  );
}

export function Skeleton({ width, height = 12, radius, className }: { width?: number | string; height?: number | string; radius?: number; className?: string }) {
  const style: CSSProperties = { width, height, borderRadius: radius };
  return <span className={["ui-skeleton", className].filter(Boolean).join(" ")} style={style} aria-hidden="true" />;
}

export function SkeletonRows({ rows = 4, label = "Loading" }: { rows?: number; label?: string }) {
  return (
    <div className="ui-skeleton-rows" role="status" aria-label={label}>
      {Array.from({ length: rows }, (_, index) => (
        <div className="ui-skeleton-row" key={index}>
          <Skeleton width={28} height={28} radius={8} />
          <div className="ui-skeleton-lines">
            <Skeleton width={`${62 - ((index * 13) % 24)}%`} height={11} />
            <Skeleton width={`${38 - ((index * 7) % 14)}%`} height={9} />
          </div>
        </div>
      ))}
    </div>
  );
}

export function Spinner({ size = 16, label }: { size?: number; label?: string }) {
  return (
    <span
      className="ui-spinner"
      style={{ width: size, height: size }}
      role={label ? "status" : undefined}
      aria-label={label}
      aria-hidden={label ? undefined : true}
    />
  );
}

export function ProgressBar({
  value,
  tone = "accent",
  label,
  size = "regular",
}: {
  value: number | null;
  tone?: Tone;
  label?: string;
  size?: "regular" | "thin";
}) {
  const clamped = value === null ? null : Math.max(0, Math.min(1, value));
  return (
    <div
      className={`ui-progress ui-progress-${tone} ui-progress-${size}`}
      role="progressbar"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={clamped === null ? undefined : Math.round(clamped * 100)}
      data-indeterminate={clamped === null || undefined}
    >
      <span className="ui-progress-fill" style={clamped === null ? undefined : { width: `${clamped * 100}%` }} />
    </div>
  );
}

export function Kbd({ children }: { children: ReactNode }) {
  return <kbd className="ui-kbd">{children}</kbd>;
}
