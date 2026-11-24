import assert from 'node:assert/strict';
import { test } from 'node:test';

import { actionArgs, HIDDEN, notchActivities, parseSurfaces, surfaceView } from './surfaceModel.js';

const surface = (id, template, state, extra = {}) => ({
    extension: 'media',
    id,
    placement: 'notch',
    template,
    priority: 0,
    actions: [],
    state,
    ...extra,
});

test('only well-formed surfaces survive parsing', () => {
    const good = surface('a', 'text', { text: 'hi' });
    assert.deepEqual(parseSurfaces(JSON.stringify([good, { id: 'x' }, surface('b', 'chart', {})])), [good]);
    assert.deepEqual(parseSurfaces('not json'), []);
    assert.deepEqual(parseSurfaces({ surfaces: [] }), []);
});

test('the notch shows its activities by priority, at most four', () => {
    const list = [
        surface('low', 'text', { text: 'a' }, { priority: 1 }),
        surface('home', 'text', { text: 'b' }, { placement: 'home', priority: 99 }),
        surface('high', 'text', { text: 'c' }, { priority: 50 }),
        ...['d', 'e', 'f'].map(id => surface(id, 'text', { text: id })),
    ];
    assert.deepEqual(
        notchActivities(list).map(item => item.id),
        ['high', 'low', 'd', 'e']
    );
});

test('every template turns into rows the notch can draw', () => {
    assert.deepEqual(surfaceView(surface('limit', 'ring', { value: 0.42, label: 'Session' })).rows, [
        { kind: 'meter', label: 'Session', fraction: 0.42, value: '42%' },
    ]);
    assert.deepEqual(surfaceView(surface('spend', 'stat', { value: 12, caption: 'today' }, { title: 'Spend' })).rows, [
        { kind: 'text', label: 'Spend', value: '12' },
        { kind: 'note', value: 'today' },
    ]);
    assert.deepEqual(surfaceView(surface('quiet', 'text', { text: 'All quiet' })).rows, [
        { kind: 'note', value: 'All quiet' },
    ]);
    const list = surfaceView(surface('hosts', 'list', { items: [{ title: 'box', detail: 'up' }, { title: 'nas' }] }));
    assert.equal(list.heading, 'hosts');
    assert.deepEqual(list.rows, [
        { kind: 'text', label: 'box', value: 'up' },
        { kind: 'text', label: 'nas', value: '' },
    ]);
    const playing = surfaceView(surface('now', 'nowPlaying', { title: 'Song', artist: 'Band', playing: true }));
    assert.equal(playing.playing, true);
    assert.deepEqual(playing.rows, [{ kind: 'text', label: 'Song', value: 'Band' }]);
    assert.deepEqual(surfaceView(surface('meter', 'meter', { value: 7 })).rows[0].fraction, 1);
});

test('private categories are masked while presenting', () => {
    const song = surface('now', 'nowPlaying', { title: 'Song', artist: 'Band' }, { blurCategory: 'music' });
    const masked = surfaceView(song, { musicPrivate: true, usagePrivate: false });
    assert.equal(masked.hidden, true);
    assert.deepEqual(masked.rows, [{ kind: 'text', label: HIDDEN, value: HIDDEN }]);
    assert.equal(surfaceView(song, { musicPrivate: false }).hidden, false);
    const spend = surface('spend', 'meter', { value: 0.9 }, { blurCategory: 'money' });
    assert.deepEqual(surfaceView(spend, { usagePrivate: true }).rows[0], {
        kind: 'meter',
        label: 'spend',
        fraction: 0,
        value: HIDDEN,
    });
});

test('actions keep only runnable commands', () => {
    const view = surfaceView(
        surface(
            'now',
            'actions',
            {},
            {
                actions: [
                    { id: 'next', run: ['media', 'next'] },
                    { id: 'broken', run: [] },
                    { id: 'toggle', label: 'Play or pause', run: ['media', 'toggle'] },
                ],
            }
        )
    );
    assert.deepEqual(
        view.actions.map(action => [action.label, actionArgs(action)]),
        [
            ['next', ['media', 'next']],
            ['Play or pause', ['media', 'toggle']],
        ]
    );
});
