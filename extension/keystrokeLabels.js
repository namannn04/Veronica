/* Turning a key press into the labels a keycap shows.
 *
 * Mirrors `keystroke.rs` for the clamps, and replaces Edith's
 * `KeystrokeLabelResolver` for the labels themselves. Edith maps macOS virtual
 * key codes and renders modifiers as ⌃⌥⇧⌘ in that order; a Linux keyboard has
 * different keys and a different convention, so the names here are the ones
 * printed on an Ubuntu keyboard — Ctrl, Alt, Shift, Super — in the order those
 * appear in a shortcut people write down.
 *
 * Free of any `gi://` import so it can be tested with `node --test`. The caller
 * passes the Clutter keyval and modifier bits as plain numbers.
 */

/** Mirrors `MIN_DURATION_SECS` and friends in `keystroke.rs`. */
export const MIN_DURATION_SECONDS = 0.5;
export const MAX_DURATION_SECONDS = 3;
export const DEFAULT_DURATION_SECONDS = 1.5;
export const MAXIMUM_VISIBLE = 6;

export const POSITIONS = ['top', 'bottom'];
export const DEFAULT_POSITION = 'bottom';

/** Clutter modifier bits, which match the X11 ones GDK also uses. */
export const SHIFT_MASK = 1 << 0;
export const CONTROL_MASK = 1 << 2;
export const ALT_MASK = 1 << 3;
export const SUPER_MASK = 1 << 26;

function asNumber(value) {
    return typeof value === 'number' && !Number.isNaN(value) ? value : null;
}

/** Clamp a stored duration, or fall back when it is not a number. */
export function clampDuration(value) {
    const number = asNumber(value);
    if (number === null)
        return DEFAULT_DURATION_SECONDS;
    return Math.min(Math.max(number, MIN_DURATION_SECONDS), MAX_DURATION_SECONDS);
}

export function parsePosition(raw) {
    if (typeof raw !== 'string')
        return DEFAULT_POSITION;
    const match = POSITIONS.find(position => position === raw.toLowerCase());
    return match ?? DEFAULT_POSITION;
}

/* Keyvals whose name is not what belongs on a keycap. X11 spells Return
 * "Return" and Escape "Escape"; a demo overlay wants the short forms and the
 * arrows people recognise. */
const NAMED_KEYS = new Map([
    [0xff08, '⌫'],       // BackSpace
    [0xff09, 'Tab'],
    [0xff0d, '↩'],       // Return
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
    [0xff8d, '⌤'],       // KP_Enter
    [0xff9f, '⌦'],       // KP_Delete
    [0xffff, '⌦'],       // Delete
    [0xff7f, 'Num Lock'],
    [0xffe5, 'Caps Lock'],
    [0xff14, 'Scroll Lock'],
    [0xff13, 'Pause'],
    [0xff61, 'Print'],
    [0x0020, 'Space'],
    [0xff80, 'Space'],   // KP_Space
]);

/* Modifiers pressed on their own. A demo overlay showing a bare "Ctrl" every
 * time somebody reaches for a shortcut would be noise, so these resolve to no
 * labels at all and the queue drops the press. */
const MODIFIER_KEYVALS = new Set([
    0xffe1, 0xffe2, // Shift_L, Shift_R
    0xffe3, 0xffe4, // Control_L, Control_R
    0xffe9, 0xffea, // Alt_L, Alt_R
    0xffeb, 0xffec, // Super_L, Super_R
    0xffe7, 0xffe8, // Meta_L, Meta_R
    0xfe03,         // ISO_Level3_Shift (AltGr)
    0xffe5,         // Caps_Lock, when held as a modifier
]);

/** F1-F35 occupy one contiguous run of keyvals. */
function functionKeyLabel(keyval) {
    if (keyval >= 0xffbe && keyval <= 0xffe0)
        return `F${keyval - 0xffbe + 1}`;
    return null;
}

/** The numeric keypad, which X11 gives its own keyvals. */
function keypadLabel(keyval) {
    if (keyval >= 0xffb0 && keyval <= 0xffb9)
        return String(keyval - 0xffb0);
    const operators = new Map([
        [0xffaa, '*'], [0xffab, '+'], [0xffad, '-'], [0xffae, '.'], [0xffaf, '/'],
    ]);
    return operators.get(keyval) ?? null;
}

/**
 * The label for one key, ignoring modifiers, or `null` when there is nothing
 * worth showing.
 *
 * `unicode` is the character the keyval maps to, which the caller reads with
 * `Clutter.keyval_to_unicode`. Letters are upper-cased so a shortcut reads the
 * way it is written down, and a control character is not a label — Ctrl+C
 * produces U+0003, which is not what belongs on the cap.
 */
export function keyLabel(keyval, unicode) {
    if (typeof keyval !== 'number')
        return null;
    if (MODIFIER_KEYVALS.has(keyval))
        return null;

    const named = NAMED_KEYS.get(keyval);
    if (named)
        return named;

    const fn = functionKeyLabel(keyval);
    if (fn)
        return fn;

    const keypad = keypadLabel(keyval);
    if (keypad)
        return keypad;

    if (isPrintable(unicode))
        return String.fromCodePoint(unicode).toUpperCase();

    // Ctrl+C reports U+0003 as the character, which is not what belongs on the
    // cap. X11 keyvals below 0x100 are their own Latin-1 code point, so the
    // keyval still says which key was struck when the character does not.
    if (keyval > 0x20 && keyval < 0x100 && keyval !== 0x7f)
        return String.fromCodePoint(keyval).toUpperCase();

    return null;
}

/** A printable character, i.e. not a control code and not a bare space, which
 *  `NAMED_KEYS` already spells out as a word. */
function isPrintable(unicode) {
    return typeof unicode === 'number' && unicode > 0x20 && unicode !== 0x7f;
}

/**
 * The full row for one press: the modifiers, then the key.
 *
 * Returns an empty array when the press has nothing to show, which is what the
 * queue treats as "not an entry".
 */
export function labelsFor(keyval, unicode, modifierState) {
    const key = keyLabel(keyval, unicode);
    if (key === null)
        return [];

    const state = typeof modifierState === 'number' ? modifierState : 0;
    const labels = [];
    if (state & CONTROL_MASK)
        labels.push('Ctrl');
    if (state & ALT_MASK)
        labels.push('Alt');
    if (state & SHIFT_MASK)
        labels.push('Shift');
    if (state & SUPER_MASK)
        labels.push('Super');

    // Shift is already spelled out, so showing "Shift A" rather than "Shift ⇧A"
    // avoids saying the same thing twice. A shifted symbol still shows the
    // symbol, since that is the key the viewer has to find.
    labels.push(key);
    return labels;
}

/**
 * The visible keycaps, oldest first.
 *
 * Mirrors `keystroke::Queue`: a press with no labels is not an entry, and only
 * the newest `maximumVisible` are kept so a fast typist pushes old caps out
 * rather than filling the screen.
 */
export class KeystrokeQueue {
    constructor(maximumVisible = MAXIMUM_VISIBLE) {
        this._entries = [];
        // A queue that can hold nothing would drop every press silently.
        this._maximumVisible = Math.max(1, maximumVisible);
        this._nextId = 1;
    }

    get entries() {
        return this._entries;
    }

    get maximumVisible() {
        return this._maximumVisible;
    }

    /** Record a press. Returns the entry, or `null` when there was nothing to show. */
    append(keys, nowMs, durationSeconds) {
        if (!Array.isArray(keys) || keys.length === 0)
            return null;
        const entry = {
            id: this._nextId++,
            keys,
            expiresAtMs: nowMs + Math.round(clampDuration(durationSeconds) * 1000),
        };
        this._entries.push(entry);
        if (this._entries.length > this._maximumVisible)
            this._entries.splice(0, this._entries.length - this._maximumVisible);
        return entry;
    }

    remove(id) {
        this._entries = this._entries.filter(entry => entry.id !== id);
    }

    /** `<=` rather than `<`, so an expired entry does not linger for one tick. */
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
