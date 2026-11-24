import { type CSSProperties, type RefObject, useLayoutEffect, useState } from "react";

type Box = { left: number; top: number; width: number; height: number };

export function useIndicator(
  container: RefObject<HTMLElement | null>,
  selector: string,
  key: unknown,
): CSSProperties | undefined {
  const [box, setBox] = useState<Box | null>(null);
  const [animated, setAnimated] = useState(false);

  // biome-ignore lint/correctness/useExhaustiveDependencies: key names the selected item so the indicator remeasures when the selection moves
  useLayoutEffect(() => {
    const host = container.current;
    if (!host) {
      return;
    }
    const measure = () => {
      const target = host.querySelector<HTMLElement>(selector);
      if (!target) {
        setBox(null);
        return;
      }
      setBox({
        left: target.offsetLeft,
        top: target.offsetTop,
        width: target.offsetWidth,
        height: target.offsetHeight,
      });
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(host);
    for (const child of Array.from(host.children)) {
      observer.observe(child);
    }
    return () => observer.disconnect();
  }, [container, selector, key]);

  useLayoutEffect(() => {
    if (box && !animated) {
      const frame = requestAnimationFrame(() => setAnimated(true));
      return () => cancelAnimationFrame(frame);
    }
  }, [box, animated]);

  if (!box) {
    return undefined;
  }
  return {
    width: box.width,
    height: box.height,
    transform: `translate(${box.left}px, ${box.top}px)`,
    transition: animated ? undefined : "none",
  };
}
