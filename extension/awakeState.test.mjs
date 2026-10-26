import assert from 'node:assert/strict';
import test from 'node:test';

import { AWAKE_SWITCHES, awakeState, nextDeadline } from './awakeState.js';

const [keep, lid] = AWAKE_SWITCHES;
const NOW = 1_000_000;

test('an open-ended switch is active while enabled', () => {
    assert.equal(awakeState({ preventSleep: true }, keep, NOW).active, true);
    assert.equal(awakeState({ preventSleep: false }, keep, NOW).active, false);
    assert.equal(awakeState({}, keep, NOW).active, false);
});

test('a timed session is active until its deadline and expired after', () => {
    const running = awakeState({ lidAwakeEnabled: true, lidAwakeUntil: NOW + 1 }, lid, NOW);
    assert.equal(running.active, true);
    assert.equal(running.expired, false);
    const over = awakeState({ lidAwakeEnabled: true, lidAwakeUntil: NOW }, lid, NOW);
    assert.equal(over.active, false);
    assert.equal(over.expired, true);
});

test('a junk deadline is ignored, as the core does', () => {
    for (const until of [0, -5, 1.5, '123', null]) {
        const state = awakeState({ preventSleep: true, preventSleepUntil: until }, keep, NOW);
        assert.equal(state.untilMs, null);
        assert.equal(state.active, true);
    }
});

test('a switched-off session with a stale deadline is neither active nor expired', () => {
    const state = awakeState({ preventSleep: false, preventSleepUntil: NOW - 1 }, keep, NOW);
    assert.deepEqual([state.active, state.expired], [false, false]);
});

test('the next deadline is the soonest running one', () => {
    assert.equal(nextDeadline({}, NOW), null);
    assert.equal(nextDeadline({
        preventSleep: true, preventSleepUntil: NOW + 500,
        lidAwakeEnabled: true, lidAwakeUntil: NOW + 100,
    }, NOW), NOW + 100);
    assert.equal(nextDeadline({ preventSleep: true, preventSleepUntil: NOW - 1 }, NOW), null);
});
