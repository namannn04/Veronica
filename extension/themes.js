/* The appearances the notch understands.
 *
 * One list, imported by everything that touches a `veronica-theme-*` style
 * class. The panel resolves the setting through it and the clock removes the
 * old class through it, so adding a theme can no longer leave a stale class
 * stuck on the popup because one of the two lists was missed.
 *
 * It must stay in step with `APPEARANCES` in
 * `apps/desktop/src/lib/preferences.ts` and with the blocks in
 * `stylesheet.css`. `system` is not here: it is not a class, it is the absence
 * of a choice, resolved against GNOME's own colour-scheme.
 */

/** Dark appearances, which inherit the notch's default white typography. */
export const DARK_THEMES = [
    'dark',
    'midnight',
    'aubergine',
    'forest',
    'nord',
    'ocean',
    'ember',
    'carbon',
];

/** Light appearances, which need the foreground inversion in the stylesheet. */
export const LIGHT_THEMES = ['light', 'sandstone', 'mist', 'paper'];

/** Every appearance that names a style class. */
export const THEMES = [...DARK_THEMES, ...LIGHT_THEMES];

const KNOWN = new Set(THEMES);

/** Whether `value` is an appearance the stylesheet has rules for. */
export function isTheme(value) {
    return KNOWN.has(value);
}
