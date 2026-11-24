import { Check } from "lucide-react";
import { type CSSProperties, useId } from "react";

import { APPEARANCE_FAMILIES, APPEARANCES, type Appearance } from "../lib/preferences";
import "./ThemePicker.css";

const DETAIL_OVERRIDES: Partial<Record<Appearance, string>> = {
  dark: "Warm graphite",
};

type Swatch = readonly string[];

export function ThemePicker({
  value,
  onChange,
}: {
  value: Appearance;
  onChange: (appearance: Appearance) => void;
}) {
  const name = useId();

  return (
    <div className="theme-picker" role="radiogroup" aria-label="Theme">
      {APPEARANCE_FAMILIES.map((family) => {
        const themes = APPEARANCES.filter((theme) => theme.family === family.id);
        if (themes.length === 0) {
          return null;
        }
        return (
          <fieldset key={family.id} className="theme-family" aria-label={family.label}>
            <h3 className="theme-family-title">{family.label}</h3>
            <div className="theme-grid">
              {themes.map((theme) => {
                const selected = value === theme.id;
                return (
                  <label
                    key={theme.id}
                    className="theme-option"
                    data-selected={selected || undefined}
                  >
                    <input
                      type="radio"
                      className="ui-visually-hidden"
                      name={name}
                      value={theme.id}
                      checked={selected}
                      onChange={() => onChange(theme.id)}
                    />
                    <ThemeTile
                      swatch={theme.swatch}
                      swatchDark={"swatchDark" in theme ? theme.swatchDark : undefined}
                    />
                    <span className="theme-option-text">
                      <span className="theme-option-label">
                        {theme.label}
                        {selected && <Check aria-hidden="true" />}
                      </span>
                      <span className="theme-option-detail">
                        {DETAIL_OVERRIDES[theme.id] ?? theme.detail}
                      </span>
                    </span>
                  </label>
                );
              })}
            </div>
          </fieldset>
        );
      })}
    </div>
  );
}

function ThemeTile({ swatch, swatchDark }: { swatch: Swatch; swatchDark?: Swatch }) {
  const split = swatchDark !== undefined;
  return (
    <span className="theme-tile" aria-hidden="true">
      <span
        className={split ? "theme-tile-half theme-tile-split-a" : "theme-tile-half"}
        style={tileVars(swatch)}
      />
      {swatchDark && (
        <span className="theme-tile-half theme-tile-split-b" style={tileVars(swatchDark)} />
      )}
    </span>
  );
}

function tileVars([plane, raised, accent]: Swatch): CSSProperties {
  return {
    "--tile-plane": plane,
    "--tile-raised": raised,
    "--tile-accent": accent,
  } as CSSProperties;
}
