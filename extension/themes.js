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

export const LIGHT_THEMES = ['light', 'sandstone', 'mist', 'paper'];

export const THEMES = [...DARK_THEMES, ...LIGHT_THEMES];

const KNOWN = new Set(THEMES);

export function isTheme(value) {
    return KNOWN.has(value);
}
