import { ChevronDown } from "lucide-react";
import { type KeyboardEvent, type ReactNode, useLayoutEffect, useRef, useState } from "react";

import "./Tabs.css";
import { useIndicator } from "./useIndicator";

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
  const list = useRef<HTMLDivElement>(null);
  const indicator = useIndicator(
    list,
    '[role="tab"][aria-selected="true"]',
    `${value}:${collapsed}`,
  );

  useLayoutEffect(() => {
    const node = host.current;
    const ruler = measure.current;
    if (!node || !ruler) {
      return;
    }
    const update = () => setCollapsed(ruler.scrollWidth > node.clientWidth + 1);
    update();
    const observer = new ResizeObserver(update);
    observer.observe(node);
    observer.observe(ruler);
    return () => observer.disconnect();
  }, []);

  const select = (next: T, animate: boolean) => {
    const page = host.current?.closest<HTMLElement>(".ui-page");
    if (page && next !== value) {
      if (animate) {
        page.dataset.tabSwap = page.dataset.tabSwap === "a" ? "b" : "a";
      } else {
        delete page.dataset.tabSwap;
      }
    }
    onChange(next);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const index = items.findIndex((item) => item.id === value);
    let next = index;
    if (event.key === "ArrowRight") {
      next = (index + 1) % items.length;
    } else if (event.key === "ArrowLeft") {
      next = (index - 1 + items.length) % items.length;
    } else if (event.key === "Home") {
      next = 0;
    } else if (event.key === "End") {
      next = items.length - 1;
    } else {
      return;
    }
    event.preventDefault();
    select(items[next].id, false);
    const buttons = event.currentTarget.querySelectorAll<HTMLButtonElement>("[role=tab]");
    buttons[next]?.focus();
  };

  const tabContent = (item: TabItem<T>) => (
    <>
      <span className="ui-tab-label" data-text={item.label}>
        {item.label}
      </span>
      {item.badge !== undefined && <span className="ui-tab-badge">{item.badge}</span>}
    </>
  );

  return (
    <div className="ui-tabs-host" ref={host}>
      <div className="ui-tabs ui-tabs-ruler" ref={measure} aria-hidden="true">
        {items.map((item) => (
          <button
            key={item.id}
            type="button"
            tabIndex={-1}
            className="ui-tab"
            data-selected={item.id === value || undefined}
          >
            {tabContent(item)}
          </button>
        ))}
      </div>
      {collapsed ? (
        <label className="ui-tabs-menu">
          <span className="ui-visually-hidden">{label}</span>
          <select value={value} onChange={(event) => select(event.target.value as T, true)}>
            {items.map((item) => (
              <option key={item.id} value={item.id}>
                {item.label}
              </option>
            ))}
          </select>
          <ChevronDown aria-hidden="true" />
        </label>
      ) : (
        <div className="ui-tabs" role="tablist" aria-label={label} onKeyDown={onKeyDown} ref={list}>
          <span className="ui-tabs-indicator" style={indicator} aria-hidden="true" />
          {items.map((item) => (
            <button
              key={item.id}
              type="button"
              role="tab"
              aria-selected={item.id === value}
              tabIndex={item.id === value ? 0 : -1}
              className="ui-tab"
              data-selected={item.id === value || undefined}
              onClick={() => select(item.id, true)}
            >
              {tabContent(item)}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
