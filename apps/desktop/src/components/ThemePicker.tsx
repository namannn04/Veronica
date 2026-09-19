import type { CSSProperties } from "react";

import {
  APPEARANCES,
  APPEARANCE_FAMILIES,
  type Appearance,
} from "../lib/preferences";

/**
 * The appearance picker, grouped Automatic / Light / Dark.
 *
 * The tiles are painted from the catalogue's own swatch triples rather than a
 * rule per theme, so a palette edited in `styles.css` and mirrored in
 * `preferences.ts` cannot leave a stale preview behind.
 */
export function ThemePicker({
  value,
  onChange,
}: {
  value: Appearance;
  onChange: (appearance: Appearance) => void;
}) {
  return (
    <div className="theme-picker">
      {APPEARANCE_FAMILIES.map((family) => {
        const themes = APPEARANCES.filter((theme) => theme.family === family.id);
        if (themes.length === 0) return null;
        return (
          <section key={family.id} className="theme-family">
            <h3>{family.label}</h3>
            <div className="theme-grid">
              {themes.map((theme) => (
                <button
                  key={theme.id}
                  className="theme-option"
                  aria-pressed={value === theme.id}
                  onClick={() => onChange(theme.id)}
                >
                  <ThemeTile
                    swatch={theme.swatch}
                    // Only `system` carries a second palette, and it is drawn as
                    // the far half of a diagonal split: the tile has to say
                    // "either one, depending on Ubuntu".
                    swatchDark={"swatchDark" in theme ? theme.swatchDark : undefined}
                  />
                  <strong>{theme.label}</strong>
                  <small>{theme.detail}</small>
                </button>
              ))}
            </div>
          </section>
        );
      })}
    </div>
  );
}

type Swatch = readonly [string, string, string] | readonly string[];

/** A miniature of the app: page, card and accent, in that theme's colours. */
function ThemeTile({ swatch, swatchDark }: { swatch: Swatch; swatchDark?: Swatch }) {
  const split = swatchDark !== undefined;
  return (
    <span className="theme-tile" aria-hidden="true">
      <span className={`theme-tile-half${split ? " split-a" : ""}`} style={vars(swatch)} />
      {swatchDark && <span className="theme-tile-half split-b" style={vars(swatchDark)} />}
    </span>
  );
}

function vars([plane, raised, accent]: Swatch): CSSProperties {
  return { "--tile-plane": plane, "--tile-raised": raised, "--tile-accent": accent } as CSSProperties;
}
