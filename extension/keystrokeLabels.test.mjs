/* Tests for Keystroke Highlight's labels and queue.
 *
 * The clamp and queue tests mirror the Rust ones in `keystroke.rs`,
 * deliberately: the two copies guard different moments and must not drift. The
 * label tests have no Rust counterpart, because the labels are the part that is
 * Linux's rather than Edith's.
 */

import assert from 'node:assert/strict';
import { test } from 'node:test';

import {
    ALT_MASK,
    CONTROL_MASK,
    DEFAULT_DURATION_SECONDS,
    DEFAULT_POSITION,
    KeystrokeQueue,
    MAXIMUM_VISIBLE,
    MAX_DURATION_SECONDS,
    MIN_DURATION_SECONDS,
    SHIFT_MASK,
    SUPER_MASK,
    clampDuration,
    keyLabel,
    labelsFor,
    parsePosition,
} from './keystrokeLabels.js';

test('the defaults match the core', () => {
    assert.equal(DEFAULT_DURATION_SECONDS, 1.5);
    assert.equal(MIN_DURATION_SECONDS, 0.5);
    assert.equal(MAX_DURATION_SECONDS, 3);
    assert.equal(MAXIMUM_VISIBLE, 6);
    assert.equal(DEFAULT_POSITION, 'bottom');
});

test('a duration outside the range is clamped into it', () => {
    assert.equal(clampDuration(0), MIN_DURATION_SECONDS);
    assert.equal(clampDuration(600), MAX_DURATION_SECONDS);
    assert.equal(clampDuration(2), 2);
});

test('a value that is not a number falls back rather than clamping', () => {
    // Mirrors `Settings::f64_or`: absent, not out of range.
    assert.equal(clampDuration(undefined), DEFAULT_DURATION_SECONDS);
    assert.equal(clampDuration('slow'), DEFAULT_DURATION_SECONDS);
    assert.equal(clampDuration(null), DEFAULT_DURATION_SECONDS);
    assert.equal(clampDuration(NaN), DEFAULT_DURATION_SECONDS);
});

test('an unknown position falls back to the default', () => {
    assert.equal(parsePosition('sideways'), DEFAULT_POSITION);
    assert.equal(parsePosition(undefined), DEFAULT_POSITION);
    assert.equal(parsePosition('TOP'), 'top');
});

test('a letter is shown upper case, the way a shortcut is written', () => {
    assert.equal(keyLabel(0x0061, 0x61), 'A');
    assert.equal(labelsFor(0x0061, 0x61, 0).join(' '), 'A');
});

test('a control character is not what belongs on the keycap', () => {
    // Ctrl+C produces U+0003; the cap has to read "Ctrl C".
    assert.deepEqual(labelsFor(0x0063, 0x03, CONTROL_MASK), ['Ctrl', 'C']);
});

test('modifiers are named in the order a shortcut is written', () => {
    assert.deepEqual(
        labelsFor(0x0073, 0x73, CONTROL_MASK | ALT_MASK | SHIFT_MASK | SUPER_MASK),
        ['Ctrl', 'Alt', 'Shift', 'Super', 'S']
    );
});

test('a modifier held on its own has nothing to show', () => {
    // Otherwise every reach for a shortcut would flash a bare "Ctrl".
    assert.deepEqual(labelsFor(0xffe3, 0, 0), [], 'Control_L');
    assert.deepEqual(labelsFor(0xffe9, 0, ALT_MASK), [], 'Alt_L');
    assert.deepEqual(labelsFor(0xffeb, 0, SUPER_MASK), [], 'Super_L');
});

test('named keys use the short forms a demo overlay wants', () => {
    assert.equal(keyLabel(0xff0d, 0), '↩', 'Return');
    assert.equal(keyLabel(0xff1b, 0), 'Esc');
    assert.equal(keyLabel(0xff08, 0), '⌫', 'BackSpace');
    assert.equal(keyLabel(0xffff, 0), '⌦', 'Delete');
    assert.equal(keyLabel(0xff09, 0), 'Tab');
    assert.equal(keyLabel(0xff51, 0), '←');
    assert.equal(keyLabel(0xff56, 0), 'Page ↓');
});

test('space is a word, not a blank keycap', () => {
    assert.equal(keyLabel(0x0020, 0x20), 'Space');
});

test('function keys and the keypad resolve to their own labels', () => {
    assert.equal(keyLabel(0xffbe, 0), 'F1');
    assert.equal(keyLabel(0xffc9, 0), 'F12');
    assert.equal(keyLabel(0xffb7, 0), '7', 'KP_7');
    assert.equal(keyLabel(0xffab, 0), '+', 'KP_Add');
});

test('a keyval with no printable character and no name is not shown', () => {
    assert.equal(keyLabel(0xff20, 0), null, 'Multi_key');
    assert.equal(keyLabel(undefined, undefined), null);
});

test('a press with no labels is not queued', () => {
    const queue = new KeystrokeQueue();
    assert.equal(queue.append([], 0, 1.5), null);
    assert.equal(queue.entries.length, 0);
});

test('the queue keeps the newest six and drops the oldest', () => {
    const queue = new KeystrokeQueue();
    for (let index = 0; index < 9; index += 1)
        queue.append([String(index)], 0, 1.5);
    assert.equal(queue.entries.length, MAXIMUM_VISIBLE);
    assert.deepEqual(queue.entries.map(entry => entry.keys[0]), ['3', '4', '5', '6', '7', '8']);
});

test('a queue asked to hold nothing still holds one', () => {
    const queue = new KeystrokeQueue(0);
    assert.equal(queue.maximumVisible, 1);
    queue.append(['A'], 0, 1.5);
    assert.equal(queue.entries.length, 1);
});

test('an entry expires at its duration and not before', () => {
    const queue = new KeystrokeQueue();
    queue.append(['Ctrl', 'C'], 1000, 1.5);
    assert.equal(queue.removeExpired(2499).length, 0, 'still inside its 1.5s');
    assert.equal(queue.removeExpired(2500).length, 1, 'gone exactly at its deadline');
    assert.equal(queue.entries.length, 0);
});

test('a stored duration out of range cannot extend an entry’s life', () => {
    const queue = new KeystrokeQueue();
    queue.append(['A'], 0, 600);
    assert.equal(queue.entries[0].expiresAtMs, 3000);
});

test('entries carry distinct ids so one can be removed alone', () => {
    const queue = new KeystrokeQueue();
    queue.append(['A'], 0, 1.5);
    queue.append(['B'], 0, 1.5);
    const first = queue.entries[0].id;
    assert.notEqual(first, queue.entries[1].id);
    queue.remove(first);
    assert.deepEqual(queue.entries.map(entry => entry.keys[0]), ['B']);
});

test('pausing clears what is on screen and reports what it removed', () => {
    const queue = new KeystrokeQueue();
    queue.append(['A'], 0, 1.5);
    assert.equal(queue.clear().length, 1);
    assert.equal(queue.entries.length, 0);
});
