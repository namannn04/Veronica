import { useCallback, useState, type ReactNode } from "react";

interface TipState {
  x: number;
  y: number;
  content: ReactNode;
}

/**
 * Pointer-following tooltip shared by every chart.
 *
 * The tooltip is rendered outside the SVG and never receives pointer events, so
 * it cannot flicker by stealing the hover from the mark that opened it. It is
 * flipped toward the centre near a viewport edge so it stays fully visible.
 */
export function useTooltip() {
  const [tip, setTip] = useState<TipState | null>(null);

  const show = useCallback(
    (event: { clientX: number; clientY: number }, content: ReactNode) => {
      setTip({ x: event.clientX, y: event.clientY, content });
    },
    [],
  );

  const hide = useCallback(() => setTip(null), []);

  const node = tip ? (
    <div
      className="tooltip"
      role="tooltip"
      style={{
        // Offset from the cursor, flipping before the edge rather than after,
        // because a clipped tooltip is worse than one on the other side.
        left: tip.x > window.innerWidth - 280 ? tip.x - 14 : tip.x + 14,
        top: tip.y > window.innerHeight - 130 ? tip.y - 14 : tip.y + 14,
        transform: `translate(${tip.x > window.innerWidth - 280 ? "-100%" : "0"}, ${
          tip.y > window.innerHeight - 130 ? "-100%" : "0"
        })`,
      }}
    >
      {tip.content}
    </div>
  ) : null;

  return { show, hide, node };
}

export function TipRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="t-row">
      <span>{label}</span>
      <span>{value}</span>
    </div>
  );
}
