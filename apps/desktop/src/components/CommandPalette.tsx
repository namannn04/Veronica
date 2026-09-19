import { useEffect, useRef, useState } from "react";

import { searchNav, type NavItem, type Route } from "../lib/navigation";
import { SearchIcon } from "./icons";

/**
 * Jump to any page by typing.
 *
 * Nineteen pages is more than a rail can make findable by eye, and the two
 * that are hardest to find are the ones you need least often - which is
 * exactly when you have forgotten where they live. Ctrl+K searches labels,
 * one-line descriptions and a keyword list, so "ssh" reaches Machines and
 * "theme" reaches Settings.
 */
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
  // A filter that removed the highlighted row must not leave the highlight
  // pointing past the end, or Enter would do nothing.
  const index = Math.min(active, Math.max(results.length - 1, 0));

  useEffect(() => {
    if (!open) return;
    setQuery("");
    setActive(0);
    // The dialog mounts hidden-then-shown, so focus has to wait for the frame
    // in which it is actually in the layout.
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
            placeholder="Go to a page…"
            aria-label="Search pages"
            aria-controls="palette-results"
            autoComplete="off"
            spellCheck={false}
            onChange={(event) => {
              setQuery(event.target.value);
              setActive(0);
            }}
          />
          <kbd>Esc</kbd>
        </div>

        <div className="palette-results" id="palette-results" role="listbox" ref={listRef}>
          {results.length === 0 && (
            <p className="palette-none">No page matches “{query}”.</p>
          )}
          {results.map((item, position) => (
            <button
              key={item.id}
              role="option"
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
