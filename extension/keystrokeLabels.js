export const MIN_DURATION_SECONDS = 0.5;
export const MAX_DURATION_SECONDS = 3;
export const DEFAULT_DURATION_SECONDS = 1.5;
export const MAXIMUM_VISIBLE = 6;

export const POSITIONS = ['top', 'bottom'];
export const DEFAULT_POSITION = 'bottom';

export const SHIFT_MASK = 1 << 0;
export const CONTROL_MASK = 1 << 2;
export const ALT_MASK = 1 << 3;
export const SUPER_MASK = 1 << 26;

function asNumber(value) {
    return typeof value === 'number' && !Number.isNaN(value) ? value : null;
}

export function clampDuration(value) {
    const number = asNumber(value);
    if (number === null) {
        return DEFAULT_DURATION_SECONDS;
    }
    return Math.min(Math.max(number, MIN_DURATION_SECONDS), MAX_DURATION_SECONDS);
}

export function parsePosition(raw) {
    if (typeof raw !== 'string') {
        return DEFAULT_POSITION;
    }
    const match = POSITIONS.find(position => position === raw.toLowerCase());
    return match ?? DEFAULT_POSITION;
}

const NAMED_KEYS = new Map([
    [0xff08, '⌫'],
    [0xff09, 'Tab'],
    [0xff0d, '↩'],
    [0xff1b, 'Esc'],
    [0xff50, 'Home'],
    [0xff51, '←'],
    [0xff52, '↑'],
    [0xff53, '→'],
    [0xff54, '↓'],
    [0xff55, 'Page ↑'],
    [0xff56, 'Page ↓'],
    [0xff57, 'End'],
    [0xff63, 'Insert'],
    [0xff8d, '⌤'],
    [0xff9f, '⌦'],
    [0xffff, '⌦'],
    [0xff7f, 'Num Lock'],
    [0xffe5, 'Caps Lock'],
    [0xff14, 'Scroll Lock'],
    [0xff13, 'Pause'],
    [0xff61, 'Print'],
    [0x0020, 'Space'],
    [0xff80, 'Space'],
]);

const MODIFIER_KEYVALS = new Set([
    0xffe1, 0xffe2, 0xffe3, 0xffe4, 0xffe9, 0xffea, 0xffeb, 0xffec, 0xffe7, 0xffe8, 0xfe03, 0xffe5,
]);

function functionKeyLabel(keyval) {
    if (keyval >= 0xffbe && keyval <= 0xffe0) {
        return `F${keyval - 0xffbe + 1}`;
    }
    return null;
}

function keypadLabel(keyval) {
    if (keyval >= 0xffb0 && keyval <= 0xffb9) {
        return String(keyval - 0xffb0);
    }
    const operators = new Map([
        [0xffaa, '*'],
        [0xffab, '+'],
        [0xffad, '-'],
        [0xffae, '.'],
        [0xffaf, '/'],
    ]);
    return operators.get(keyval) ?? null;
}

export function keyLabel(keyval, unicode) {
    if (typeof keyval !== 'number') {
        return null;
    }
    if (MODIFIER_KEYVALS.has(keyval)) {
        return null;
    }

    const named = NAMED_KEYS.get(keyval);
    if (named) {
        return named;
    }

    const fn = functionKeyLabel(keyval);
    if (fn) {
        return fn;
    }

    const keypad = keypadLabel(keyval);
    if (keypad) {
        return keypad;
    }

    if (isPrintable(unicode)) {
        return String.fromCodePoint(unicode).toUpperCase();
    }

    if (keyval > 0x20 && keyval < 0x100 && keyval !== 0x7f) {
        return String.fromCodePoint(keyval).toUpperCase();
    }

    return null;
}

function isPrintable(unicode) {
    return typeof unicode === 'number' && unicode > 0x20 && unicode !== 0x7f;
}

export function labelsFor(keyval, unicode, modifierState) {
    const key = keyLabel(keyval, unicode);
    if (key === null) {
        return [];
    }

    const state = typeof modifierState === 'number' ? modifierState : 0;
    const labels = [];
    if (state & CONTROL_MASK) {
        labels.push('Ctrl');
    }
    if (state & ALT_MASK) {
        labels.push('Alt');
    }
    if (state & SHIFT_MASK) {
        labels.push('Shift');
    }
    if (state & SUPER_MASK) {
        labels.push('Super');
    }

    labels.push(key);
    return labels;
}

export class KeystrokeQueue {
    constructor(maximumVisible = MAXIMUM_VISIBLE) {
        this._entries = [];
        this._maximumVisible = Math.max(1, maximumVisible);
        this._nextId = 1;
    }

    get entries() {
        return this._entries;
    }

    get maximumVisible() {
        return this._maximumVisible;
    }

    append(keys, nowMs, durationSeconds) {
        if (!Array.isArray(keys) || keys.length === 0) {
            return null;
        }
        const entry = {
            id: this._nextId++,
            keys,
            expiresAtMs: nowMs + Math.round(clampDuration(durationSeconds) * 1000),
        };
        this._entries.push(entry);
        if (this._entries.length > this._maximumVisible) {
            this._entries.splice(0, this._entries.length - this._maximumVisible);
        }
        return entry;
    }

    remove(id) {
        this._entries = this._entries.filter(entry => entry.id !== id);
    }

    removeExpired(nowMs) {
        const expired = this._entries.filter(entry => entry.expiresAtMs <= nowMs);
        this._entries = this._entries.filter(entry => entry.expiresAtMs > nowMs);
        return expired;
    }

    clear() {
        const dropped = this._entries;
        this._entries = [];
        return dropped;
    }
}
