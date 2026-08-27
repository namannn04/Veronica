/* Clamps and mode parsing for Focus Dim.
 *
 * Mirrors `focus_dim.rs` exactly. Two copies rather than one because they guard
 * different moments: the Rust side stops a bad value being *stored*, and this
 * stops a hand-edited settings file from being *applied* — an overlay at full
 * opacity would leave the user unable to see the desktop well enough to fix it.
 *
 * Free of any `gi://` import so it can be tested with `node --test`.
 */

/** A fully opaque overlay is a lock screen, not a focus aid, so 0.9 is the cap. */
export const MIN_INTENSITY = 0;
export const MAX_INTENSITY = 0.9;
export const DEFAULT_INTENSITY = 0.45;

/** Not zero: an instant change reads as a flicker as focus moves. */
export const MIN_ANIMATION_SECONDS = 0.05;
export const MAX_ANIMATION_SECONDS = 1;
export const DEFAULT_ANIMATION_SECONDS = 0.25;

export const DISPLAY_MODES = ['perScreenFront', 'dimUnfocused'];
export const DEFAULT_DISPLAY_MODE = 'perScreenFront';

/**
 * Whether a stored value is a number at all.
 *
 * This mirrors `Settings::f64_or`, which falls back unless the JSON value really
 * is a number: a string, null or a missing key are all *absent* rather than out
 * of range, and clamping them would silently pick a bound. A NaN is treated the
 * same way for the same reason, matching `clamp_intensity` in the core.
 * Infinities are numbers and do clamp — JSON cannot carry one, but the check
 * stays faithful rather than convenient.
 */
function asNumber(value) {
    return typeof value === 'number' && !Number.isNaN(value) ? value : null;
}

/** Clamp a stored intensity, or fall back when it is not a number. */
export function clampIntensity(value) {
    const number = asNumber(value);
    if (number === null)
        return DEFAULT_INTENSITY;
    return Math.min(Math.max(number, MIN_INTENSITY), MAX_INTENSITY);
}

/** Clamp a stored fade length, or fall back when it is not a number. */
export function clampAnimationSeconds(value) {
    const number = asNumber(value);
    if (number === null)
        return DEFAULT_ANIMATION_SECONDS;
    return Math.min(Math.max(number, MIN_ANIMATION_SECONDS), MAX_ANIMATION_SECONDS);
}

export function parseDisplayMode(raw) {
    if (typeof raw !== 'string')
        return DEFAULT_DISPLAY_MODE;
    const match = DISPLAY_MODES.find(mode => mode.toLowerCase() === raw.toLowerCase());
    return match ?? DEFAULT_DISPLAY_MODE;
}
