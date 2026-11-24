export const MIN_INTENSITY = 0;
export const MAX_INTENSITY = 0.9;
export const DEFAULT_INTENSITY = 0.45;

export const MIN_ANIMATION_SECONDS = 0.05;
export const MAX_ANIMATION_SECONDS = 1;
export const DEFAULT_ANIMATION_SECONDS = 0.25;

export const DISPLAY_MODES = ['perScreenFront', 'dimUnfocused'];
export const DEFAULT_DISPLAY_MODE = 'perScreenFront';

function asNumber(value) {
    return typeof value === 'number' && !Number.isNaN(value) ? value : null;
}

export function clampIntensity(value) {
    const number = asNumber(value);
    if (number === null) {
        return DEFAULT_INTENSITY;
    }
    return Math.min(Math.max(number, MIN_INTENSITY), MAX_INTENSITY);
}

export function clampAnimationSeconds(value) {
    const number = asNumber(value);
    if (number === null) {
        return DEFAULT_ANIMATION_SECONDS;
    }
    return Math.min(Math.max(number, MIN_ANIMATION_SECONDS), MAX_ANIMATION_SECONDS);
}

export function parseDisplayMode(raw) {
    if (typeof raw !== 'string') {
        return DEFAULT_DISPLAY_MODE;
    }
    const match = DISPLAY_MODES.find(mode => mode.toLowerCase() === raw.toLowerCase());
    return match ?? DEFAULT_DISPLAY_MODE;
}

export function overlayPlacement(mode, hasBrightWindow, hasFocus) {
    if (hasBrightWindow) {
        return 'below';
    }
    if (mode === 'dimUnfocused' && hasFocus) {
        return 'top';
    }
    return 'off';
}
