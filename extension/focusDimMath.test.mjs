/* Tests for Focus Dim's clamps.
 *
 * These mirror the Rust tests in `focus_dim.rs`, deliberately: the two clamps
 * guard different moments and must not drift.
 */

import assert from 'node:assert/strict';
import { test } from 'node:test';

import {
    DEFAULT_ANIMATION_SECONDS,
    DEFAULT_DISPLAY_MODE,
    DEFAULT_INTENSITY,
    MAX_ANIMATION_SECONDS,
    MAX_INTENSITY,
    MIN_ANIMATION_SECONDS,
    MIN_INTENSITY,
    clampAnimationSeconds,
    clampIntensity,
    parseDisplayMode,
} from './focusDimMath.js';

test('the defaults match the core', () => {
    assert.equal(DEFAULT_INTENSITY, 0.45);
    assert.equal(DEFAULT_ANIMATION_SECONDS, 0.25);
    assert.equal(DEFAULT_DISPLAY_MODE, 'perScreenFront');
});

test('intensity never reaches fully opaque', () => {
    // A dim of 1.0 hides the desktop, leaving the user unable to see well
    // enough to switch it off again.
    assert.ok(MAX_INTENSITY < 1);
    assert.equal(clampIntensity(1), MAX_INTENSITY);
    assert.equal(clampIntensity(50), MAX_INTENSITY);
});

test('a negative intensity clamps to transparent', () => {
    assert.equal(clampIntensity(-3), MIN_INTENSITY);
});

test('a value that is not a number falls back rather than clamping', () => {
    // Clamping would silently pick a bound; these are absent values.
    for (const absent of [undefined, null, 'dark', '', {}, [], true, NaN]) {
        assert.equal(clampIntensity(absent), DEFAULT_INTENSITY, `intensity ${String(absent)}`);
        assert.equal(
            clampAnimationSeconds(absent),
            DEFAULT_ANIMATION_SECONDS,
            `animation ${String(absent)}`
        );
    }
});

test('infinities are numbers and do clamp', () => {
    assert.equal(clampIntensity(Infinity), MAX_INTENSITY);
    assert.equal(clampIntensity(-Infinity), MIN_INTENSITY);
});

test('an instant fade is clamped up because it reads as a flicker', () => {
    assert.equal(clampAnimationSeconds(0), MIN_ANIMATION_SECONDS);
    assert.equal(clampAnimationSeconds(30), MAX_ANIMATION_SECONDS);
    assert.equal(clampAnimationSeconds(0.3), 0.3);
});

test('valid values pass through untouched', () => {
    assert.equal(clampIntensity(0.7), 0.7);
    assert.equal(clampIntensity(0), 0);
    assert.equal(clampIntensity(MAX_INTENSITY), MAX_INTENSITY);
});

test('an unknown mode falls back to the default', () => {
    assert.equal(parseDisplayMode('nonsense'), DEFAULT_DISPLAY_MODE);
    assert.equal(parseDisplayMode(''), DEFAULT_DISPLAY_MODE);
    assert.equal(parseDisplayMode(undefined), DEFAULT_DISPLAY_MODE);
    assert.equal(parseDisplayMode(null), DEFAULT_DISPLAY_MODE);
    assert.equal(parseDisplayMode(42), DEFAULT_DISPLAY_MODE);
});

test('a known mode is accepted case-insensitively, as in the core', () => {
    assert.equal(parseDisplayMode('dimUnfocused'), 'dimUnfocused');
    assert.equal(parseDisplayMode('DIMUNFOCUSED'), 'dimUnfocused');
    assert.equal(parseDisplayMode('perScreenFront'), 'perScreenFront');
});
