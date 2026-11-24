import { useCallback, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";

import "./charts.css";

type TipState = {
  x: number;
  y: number;
  content: ReactNode;
};

const FLIP_X = 280;
const FLIP_Y = 130;
const OFFSET = 14;

export function useTooltip() {
  const [tip, setTip] = useState<TipState | null>(null);

  const show = useCallback((event: { clientX: number; clientY: number }, content: ReactNode) => {
    setTip({ x: event.clientX, y: event.clientY, content });
  }, []);

  const hide = useCallback(() => setTip(null), []);

  let node: ReactNode = null;
  if (tip) {
    const flipX = tip.x > window.innerWidth - FLIP_X;
    const flipY = tip.y > window.innerHeight - FLIP_Y;
    node = createPortal(
      <div
        className="chart-tip"
        role="tooltip"
        style={{
          left: flipX ? tip.x - OFFSET : tip.x + OFFSET,
          top: flipY ? tip.y - OFFSET : tip.y + OFFSET,
          transform: `translate(${flipX ? "-100%" : "0"}, ${flipY ? "-100%" : "0"})`,
        }}
      >
        {tip.content}
      </div>,
      document.body,
    );
  }

  return { show, hide, node };
}

export function TipTitle({ children }: { children: ReactNode }) {
  return <div className="chart-tip-title">{children}</div>;
}

export function TipRow({ label, value, color }: { label: string; value: string; color?: string }) {
  return (
    <div className="chart-tip-row">
      <span className="chart-tip-label">
        {color && <span className="chart-swatch" style={{ background: color }} aria-hidden="true" />}
        {label}
      </span>
      <span className="chart-tip-value">{value}</span>
    </div>
  );
}
