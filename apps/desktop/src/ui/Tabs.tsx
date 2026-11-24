import { useLayoutEffect, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { ChevronDown } from "lucide-react";

import "./Tabs.css";

export type TabItem<T extends string> = { id: T; label: string; badge?: ReactNode };

export function Tabs<T extends string>({
  items,
  value,
  onChange,
  label,
}: {
  items: readonly TabItem<T>[];
  value: T;
  onChange: (value: T) => void;
  label: string;
}) {
  const measure = useRef<HTMLDivElement>(null);
  const host = useRef<HTMLDivElement>(null);
  const [collapsed, setCollapsed] = useState(false);

  useLayoutEffect(() => {
    const node = host.current;
    const ruler = measure.current;
    if (!node || !ruler) return;
    const update = () => setCollapsed(ruler.scrollWidth > node.clientWidth + 1);
    update();
    const observer = new ResizeObserver(update);
    observer.observe(node);
    return () => observer.disconnect();
  }, [items]);

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const index = items.findIndex((item) => item.id === value);
    let next = index;
    if (event.key === "ArrowRight") next = (index + 1) % items.length;
    else if (event.key === "ArrowLeft") next = (index - 1 + items.length) % items.length;
    else if (event.key === "Home") next = 0;
    else if (event.key === "End") next = items.length - 1;
    else return;
    event.preventDefault();
    onChange(items[next].id);
    const buttons = event.currentTarget.querySelectorAll<HTMLButtonElement>("[role=tab]");
    buttons[next]?.focus();
  };

  const tabs = (interactive: boolean) =>
    items.map((item) => (
      <button
        key={item.id}
        type="button"
        role={interactive ? "tab" : undefined}
        aria-selected={interactive ? item.id === value : undefined}
        tabIndex={interactive ? (item.id === value ? 0 : -1) : -1}
        className="ui-tab"
        data-selected={item.id === value || undefined}
        onClick={interactive ? () => onChange(item.id) : undefined}
      >
        <span className="ui-tab-label" data-text={item.label}>{item.label}</span>
        {item.badge !== undefined && <span className="ui-tab-badge">{item.badge}</span>}
      </button>
    ));

  return (
    <div className="ui-tabs-host" ref={host}>
      <div className="ui-tabs ui-tabs-ruler" ref={measure} aria-hidden="true">
        {tabs(false)}
      </div>
      {collapsed ? (
        <label className="ui-tabs-menu">
          <span className="ui-visually-hidden">{label}</span>
          <select value={value} onChange={(event) => onChange(event.target.value as T)}>
            {items.map((item) => (
              <option key={item.id} value={item.id}>
                {item.label}
              </option>
            ))}
          </select>
          <ChevronDown aria-hidden="true" />
        </label>
      ) : (
        <div className="ui-tabs" role="tablist" aria-label={label} onKeyDown={onKeyDown}>
          {tabs(true)}
        </div>
      )}
    </div>
  );
}
