import type { CSSProperties, KeyboardEvent } from "react";
import { Check } from "lucide-react";

import { APPEARANCES, APPEARANCE_FAMILIES, type Appearance } from "../lib/preferences";
import "./ThemePicker.css";

const DETAIL_OVERRIDES: Partial<Record<Appearance, string>> = {
  dark: "Warm graphite",
};

type Swatch = readonly string[];

export function ThemePicker({ value, onChange }: { value: Appearance; onChange: (appearance: Appearance) => void }) {
  const order = APPEARANCE_FAMILIES.flatMap((family) => APPEARANCES.filter((theme) => theme.family === family.id).map((theme) => theme.id));

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const index = order.indexOf(value);
    let next = index;
    if (event.key === "ArrowRight" || event.key === "ArrowDown") next = (index + 1) % order.length;
    else if (event.key === "ArrowLeft" || event.key === "ArrowUp") next = (index - 1 + order.length) % order.length;
    else return;
    event.preventDefault();
    onChange(order[next]);
    event.currentTarget.querySelector<HTMLButtonElement>(`[data-theme-id="${order[next]}"]`)?.focus();
  };

  return (
    <div className="theme-picker" role="radiogroup" aria-label="Theme" onKeyDown={onKeyDown}>
      {APPEARANCE_FAMILIES.map((family) => {
        const themes = APPEARANCES.filter((theme) => theme.family === family.id);
        if (themes.length === 0) return null;
        return (
          <div key={family.id} className="theme-family" role="group" aria-label={family.label}>
            <h3 className="theme-family-title">{family.label}</h3>
            <div className="theme-grid">
              {themes.map((theme) => {
                const selected = value === theme.id;
                return (
                  <button
                    key={theme.id}
                    type="button"
                    role="radio"
                    aria-checked={selected}
                    tabIndex={selected ? 0 : -1}
                    data-theme-id={theme.id}
                    className="theme-option"
                    onClick={() => onChange(theme.id)}
                  >
                    <ThemeTile swatch={theme.swatch} swatchDark={"swatchDark" in theme ? theme.swatchDark : undefined} />
                    <span className="theme-option-text">
                      <span className="theme-option-label">
                        {theme.label}
                        {selected && <Check aria-hidden="true" />}
                      </span>
                      <span className="theme-option-detail">{DETAIL_OVERRIDES[theme.id] ?? theme.detail}</span>
                    </span>
                  </button>
                );
              })}
            </div>
          </div>
        );
      })}
    </div>
  );
}

function ThemeTile({ swatch, swatchDark }: { swatch: Swatch; swatchDark?: Swatch }) {
  const split = swatchDark !== undefined;
  return (
    <span className="theme-tile" aria-hidden="true">
      <span className={split ? "theme-tile-half theme-tile-split-a" : "theme-tile-half"} style={tileVars(swatch)} />
      {swatchDark && <span className="theme-tile-half theme-tile-split-b" style={tileVars(swatchDark)} />}
    </span>
  );
}

function tileVars([plane, raised, accent]: Swatch): CSSProperties {
  return { "--tile-plane": plane, "--tile-raised": raised, "--tile-accent": accent } as CSSProperties;
}
