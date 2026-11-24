import { useEffect, useRef, useState } from "react";

import { searchNav, type NavItem, type Route } from "../lib/navigation";
import { Kbd } from "../ui";
import { SearchIcon } from "./icons";

export function CommandPalette({
  open,
  onClose,
  onNavigate,
  items,
}: {
  open: boolean;
  onClose: () => void;
  onNavigate: (route: Route) => void;
  items: NavItem[];
}) {
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  const results = searchNav(query, items);
  const index = Math.min(active, Math.max(results.length - 1, 0));

  useEffect(() => {
    if (!open) return;
    setQuery("");
    setActive(0);
    const frame = requestAnimationFrame(() => input.current?.focus());
    return () => cancelAnimationFrame(frame);
  }, [open]);

  useEffect(() => {
    listRef.current
      ?.querySelector('[aria-selected="true"]')
      ?.scrollIntoView({ block: "nearest" });
  }, [index, query]);

  if (!open) return null;

  const choose = (route: Route) => {
    onNavigate(route);
    onClose();
  };

  const onKeyDown = (event: React.KeyboardEvent) => {
    if (event.key === "ArrowDown" || (event.key === "Tab" && !event.shiftKey)) {
      event.preventDefault();
      setActive((current) => (current + 1) % Math.max(results.length, 1));
    } else if (event.key === "ArrowUp" || (event.key === "Tab" && event.shiftKey)) {
      event.preventDefault();
      setActive((current) => (current - 1 + results.length) % Math.max(results.length, 1));
    } else if (event.key === "Enter") {
      event.preventDefault();
      const chosen = results[index];
      if (chosen) choose(chosen.id);
    } else if (event.key === "Escape") {
      event.preventDefault();
      onClose();
    }
  };

  return (
    <div className="palette-scrim" onMouseDown={onClose}>
      <div
        className="palette"
        role="dialog"
        aria-modal="true"
        aria-label="Go to page"
        onMouseDown={(event) => event.stopPropagation()}
        onKeyDown={onKeyDown}
      >
        <div className="palette-field">
          <SearchIcon />
          <input
            ref={input}
            value={query}
            placeholder="Go to a page"
            aria-label="Search pages"
            aria-controls="palette-results"
            autoComplete="off"
            spellCheck={false}
            onChange={(event) => {
              setQuery(event.target.value);
              setActive(0);
            }}
          />
          <Kbd>Esc</Kbd>
        </div>

        <div className="palette-results" id="palette-results" role="listbox" ref={listRef}>
          {results.length === 0 && (
            <p className="palette-none">No page matches “{query}”.</p>
          )}
          {results.map((item, position) => (
            <button
              key={item.id}
              type="button"
              role="option"
              className="palette-option"
              aria-selected={position === index}
              onMouseEnter={() => setActive(position)}
              onClick={() => choose(item.id)}
            >
              <span className="palette-glyph">{item.icon}</span>
              <span className="palette-copy">
                <strong>{item.label}</strong>
                <small>{item.hint}</small>
              </span>
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
