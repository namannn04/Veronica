import assert from 'node:assert/strict';
import test from 'node:test';

import { presenterClick, presenterState } from './presenterGate.js';

test('nothing is private while the feature is disabled, whatever else is set', () => {
    const state = presenterState({ presenterMode: true, presenterAutoActive: true });
    assert.equal(state.enabled, false);
    assert.equal(state.active, false);
    assert.equal(state.usagePrivate, false);
    assert.equal(state.musicPrivate, false);
});

test('the manual switch blurs when enabled', () => {
    const state = presenterState({ presenterEnabled: true, presenterMode: true });
    assert.deepEqual([state.active, state.usagePrivate, state.musicPrivate], [true, true, true]);
});

test('a detected share blurs on its own, until dismissed or detection is off', () => {
    const sharing = { presenterEnabled: true, presenterAutoActive: true };
    assert.equal(presenterState(sharing).active, true);
    assert.equal(presenterState({ ...sharing, presenterAutoPaused: true }).active, false);
    assert.equal(presenterState({ ...sharing, presenterAutoEnabled: false }).active, false);
});

test('a revealed category stays visible', () => {
    const state = presenterState({
        presenterEnabled: true, presenterMode: true,
        presenterBlurMusic: false,
        presenterBlurMoney: false, presenterBlurUsage: false, presenterBlurAgents: false,
    });
    assert.equal(state.active, true);
    assert.equal(state.musicPrivate, false);
    assert.equal(state.usagePrivate, false);
    const onlyRings = presenterState({
        presenterEnabled: true, presenterMode: true,
        presenterBlurMoney: false, presenterBlurUsage: false,
    });
    assert.equal(onlyRings.usagePrivate, true, 'rate limits still blur');
});

test('a click undoes whatever is holding presenter mode on', () => {
    assert.deepEqual(presenterClick(presenterState({ presenterEnabled: true })), ['start']);
    assert.deepEqual(
        presenterClick(presenterState({ presenterEnabled: true, presenterMode: true })),
        ['stop']
    );
    assert.deepEqual(
        presenterClick(presenterState({ presenterEnabled: true, presenterAutoActive: true })),
        ['dismiss']
    );
    assert.deepEqual(
        presenterClick(presenterState({
            presenterEnabled: true, presenterMode: true, presenterAutoActive: true,
        })),
        ['stop', 'dismiss']
    );
});
