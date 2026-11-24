import { SearchIcon } from "../components/icons";
import type { NavGroup, Route } from "../lib/navigation";
import { Kbd } from "../ui";
import veronicaLogo from "../assets/veronica-logo.png";

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

      <div className="shell-nav">
        {groups.map((group) => (
          <div key={group.label} className="shell-nav-group" role="group" aria-label={group.label}>
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
          </div>
        ))}
      </div>
    </nav>
  );
}
