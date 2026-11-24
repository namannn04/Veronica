import {
  cloneElement,
  isValidElement,
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type ReactElement,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";

import "./Tooltip.css";

const OPEN_DELAY_MS = 450;
const WARM_WINDOW_MS = 600;
let lastClosedAt = 0;

type Placement = { left: number; top: number; side: "top" | "bottom" };

type TriggerProps = {
  onMouseEnter?: (event: React.MouseEvent) => void;
  onMouseLeave?: (event: React.MouseEvent) => void;
  onFocus?: (event: React.FocusEvent) => void;
  onBlur?: (event: React.FocusEvent) => void;
  onMouseDown?: (event: React.MouseEvent) => void;
  "aria-describedby"?: string;
};

export function Tooltip({ content, children }: { content: ReactNode; children: ReactElement<TriggerProps> }) {
  const id = useId();
  const [open, setOpen] = useState(false);
  const [placement, setPlacement] = useState<Placement | null>(null);
  const anchor = useRef<Element | null>(null);
  const bubble = useRef<HTMLDivElement>(null);
  const timer = useRef<number | undefined>(undefined);

  const clear = () => window.clearTimeout(timer.current);

  const show = useCallback((target: Element, immediate: boolean) => {
    anchor.current = target;
    clear();
    const warm = Date.now() - lastClosedAt < WARM_WINDOW_MS;
    if (immediate || warm) setOpen(true);
    else timer.current = window.setTimeout(() => setOpen(true), OPEN_DELAY_MS);
  }, []);

  const hide = useCallback(() => {
    clear();
    setOpen((current) => {
      if (current) lastClosedAt = Date.now();
      return false;
    });
    setPlacement(null);
  }, []);

  useEffect(() => clear, []);

  useLayoutEffect(() => {
    if (!open || !anchor.current || !bubble.current) return;
    const rect = anchor.current.getBoundingClientRect();
    const tip = bubble.current.getBoundingClientRect();
    const gap = 6;
    const below = rect.top - tip.height - gap < 4;
    const top = below ? rect.bottom + gap : rect.top - tip.height - gap;
    const centered = rect.left + rect.width / 2 - tip.width / 2;
    const left = Math.min(Math.max(centered, 6), window.innerWidth - tip.width - 6);
    setPlacement({ left, top, side: below ? "bottom" : "top" });
  }, [open]);

  if (!isValidElement(children) || content === null || content === undefined || content === "") return children;

  const props = children.props;
  const trigger = cloneElement(children, {
    onMouseEnter: (event: React.MouseEvent) => {
      props.onMouseEnter?.(event);
      show(event.currentTarget, false);
    },
    onMouseLeave: (event: React.MouseEvent) => {
      props.onMouseLeave?.(event);
      hide();
    },
    onFocus: (event: React.FocusEvent) => {
      props.onFocus?.(event);
      if (event.currentTarget.matches(":focus-visible")) show(event.currentTarget, true);
    },
    onBlur: (event: React.FocusEvent) => {
      props.onBlur?.(event);
      hide();
    },
    onMouseDown: (event: React.MouseEvent) => {
      props.onMouseDown?.(event);
      hide();
    },
    "aria-describedby": open ? id : props["aria-describedby"],
  });

  return (
    <>
      {trigger}
      {open &&
        createPortal(
          <div
            ref={bubble}
            id={id}
            role="tooltip"
            className="ui-tooltip"
            data-side={placement?.side}
            style={placement ? { left: placement.left, top: placement.top } : { left: -9999, top: -9999 }}
          >
            {content}
          </div>,
          document.body,
        )}
    </>
  );
}
