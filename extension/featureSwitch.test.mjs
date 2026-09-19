import assert from 'node:assert/strict';
import { test } from 'node:test';

import { booleanSetting, FeatureSwitch } from './featureSwitch.js';

test('only real booleans override an extension default', () => {
    for (const absent of [undefined, null, 0, 1, '', 'false', {}, []]) {
        assert.equal(booleanSetting({ feature: absent }, 'feature', true), true);
        assert.equal(booleanSetting({ feature: absent }, 'feature', false), false);
    }
    assert.equal(booleanSetting({ feature: true }, 'feature', false), true);
    assert.equal(booleanSetting({ feature: false }, 'feature', true), false);
});

test('a featured extension starts on a missing first-launch setting', () => {
    const calls = [];
    const feature = new FeatureSwitch(
        'notchShelfEnabled', true,
        () => calls.push('start'),
        () => calls.push('stop')
    );

    feature.update({});
    feature.update({});
    assert.deepEqual(calls, ['start'], 'repeated settings notifications are idempotent');
});

test('settings changes stop and restore a feature without a restart', () => {
    const calls = [];
    const feature = new FeatureSwitch(
        'clipboardEnabled', true,
        () => calls.push('start'),
        () => calls.push('stop')
    );

    feature.update({ clipboardEnabled: true });
    feature.update({ clipboardEnabled: false });
    feature.update({ clipboardEnabled: false });
    feature.update({ clipboardEnabled: true });
    feature.disable();
    feature.disable();
    assert.deepEqual(calls, ['start', 'stop', 'start', 'stop']);
});

test('an unfeatured extension stays stopped until explicitly enabled', () => {
    const calls = [];
    const feature = new FeatureSwitch(
        'colorPickerEnabled', false,
        () => calls.push('start'),
        () => calls.push('stop')
    );

    feature.update({});
    assert.deepEqual(calls, []);
    feature.update({ colorPickerEnabled: true });
    assert.deepEqual(calls, ['start']);
});
