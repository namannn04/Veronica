import { ProgressBar, Stat, type Tone } from "../ui";
import "./MeterStat.css";

export function thresholdTone(value: number, warnAt: number, criticalAt: number): Tone {
  if (value >= criticalAt) {
    return "critical";
  }
  if (value >= warnAt) {
    return "warning";
  }
  return "neutral";
}

export function MeterStat({
  label,
  value,
  note,
  warnAt = 60,
  criticalAt = 85,
  mono = false,
}: {
  label: string;
  value: number;
  note: string;
  warnAt?: number;
  criticalAt?: number;
  mono?: boolean;
}) {
  const tone = thresholdTone(value, warnAt, criticalAt);
  return (
    <Stat
      className="meter-stat"
      label={
        mono ? (
          <span className="ui-mono" title={label}>
            {label}
          </span>
        ) : (
          label
        )
      }
      value={Math.round(value)}
      unit="%"
      tone={tone}
      note={
        <>
          <ProgressBar
            value={Math.max(0.01, value / 100)}
            tone={tone === "neutral" ? "accent" : tone}
            size="thin"
            label={`${label} usage`}
          />
          <span>
            {tone === "neutral" ? note : `${tone === "critical" ? "Very high" : "High"} · ${note}`}
          </span>
        </>
      }
    />
  );
}
