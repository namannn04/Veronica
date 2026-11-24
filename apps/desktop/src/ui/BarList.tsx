import type { CSSProperties, ReactNode } from "react";

import "./BarList.css";

export type BarListItem = { id: string; label: ReactNode; value: number; display: ReactNode; color?: string };

export function BarList({ items, label, max }: { items: readonly BarListItem[]; label: string; max?: number }) {
  const top = Math.max(1, max ?? 0, ...items.map((item) => item.value));
  return (
    <ul className="ui-bar-list" aria-label={label}>
      {items.map((item) => (
        <li className="ui-bar-row" key={item.id}>
          <span className="ui-bar-label">{item.label}</span>
          <span className="ui-bar-track" aria-hidden="true">
            <span
              className="ui-bar-fill"
              style={{ width: `${(item.value / top) * 100}%`, ...(item.color ? ({ "--bar-color": item.color } as CSSProperties) : {}) }}
            />
          </span>
          <span className="ui-bar-value ui-num">{item.display}</span>
        </li>
      ))}
    </ul>
  );
}
