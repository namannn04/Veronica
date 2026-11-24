import { useRef } from "react";
import veronicaLogo from "../assets/veronica-logo.png";
import { SearchIcon } from "../components/icons";
import type { NavGroup, Route } from "../lib/navigation";
import { Kbd, useIndicator } from "../ui";

export function Sidebar({
  groups,
  route,
  version,
  onNavigate,
  onSearch,
}: {
  groups: NavGroup[];
  route: Route;
  version: string | undefined;
  onNavigate: (route: Route) => void;
  onSearch: () => void;
}) {
  const nav = useRef<HTMLDivElement>(null);
  const selection = useIndicator(nav, '[aria-current="page"]', `${route}:${groups.length}`);
  return (
    <nav className="shell-sidebar" aria-label="Sections">
      <div className="shell-brand">
        <img className="shell-brand-mark" src={veronicaLogo} alt="" aria-hidden="true" />
        <span className="shell-brand-name">Veronica</span>
        {version && <span className="shell-brand-version">{version}</span>}
      </div>

      <button type="button" className="shell-search" onClick={onSearch}>
        <SearchIcon />
        <span>Go to</span>
        <Kbd>Ctrl K</Kbd>
      </button>

      <div className="shell-nav" ref={nav}>
        <span className="shell-nav-selection" style={selection} aria-hidden="true" />
        {groups.map((group) => (
          <fieldset key={group.label} className="shell-nav-group" aria-label={group.label}>
            <div className="shell-nav-heading">{group.label}</div>
            {group.items.map((item) => (
              <button
                key={item.id}
                type="button"
                className="shell-nav-item"
                aria-current={route === item.id ? "page" : undefined}
                onClick={() => onNavigate(item.id)}
              >
                <span className="shell-nav-icon">{item.icon}</span>
                <span className="shell-nav-label">{item.label}</span>
              </button>
            ))}
          </fieldset>
        ))}
      </div>
    </nav>
  );
}
