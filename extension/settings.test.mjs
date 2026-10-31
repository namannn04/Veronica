import { readFileSync } from 'node:fs';
import assert from 'node:assert/strict';
import { test } from 'node:test';

function harness() {
    let text = '{"preventSleep":true}';
    let changed;
    const timers = new Map();
    let id = 0;
    let directoryWatches = 0;
    const monitor = { connect: (_s, fn) => { changed = fn; return 1; }, disconnect() {}, cancel() {} };
    const file = { load_contents() { if (text === null) throw Error('replaced');
        return [true, new TextEncoder().encode(text)]; },
        get_parent: () => ({ get_path: () => '/config/veronica', monitor_directory() {
            directoryWatches++; return monitor;
        } }) };
    const GLib = { get_user_config_dir: () => '/config', build_filenamev: a => a.join('/'),
        mkdir_with_parents() {}, PRIORITY_DEFAULT_IDLE: 0, SOURCE_REMOVE: false,
        timeout_add: (_p, _t, fn) => { timers.set(++id, fn); return id; },
        Source: { remove: id => timers.delete(id) } };
    const Gio = { File: { new_for_path: () => file }, FileMonitorFlags: { NONE: 0 } };
    const source = readFileSync(new URL('./settings.js', import.meta.url), 'utf8')
        .replace(/^import .*;\n/gm, '').replace(/export (function|class) /g, '$1 ');
    const Watcher = new Function('Gio', 'GLib', `${source}\nreturn SettingsWatcher;`)(Gio, GLib);
    const watcher = new Watcher(); watcher.enable();
    return { watcher, timers, directoryWatches: () => directoryWatches,
        set: value => { text = value; },
        replace() { changed(monitor, { get_basename: () => '.settings.tmp' }, { get_basename: () => 'settings.json' }); },
        flush() { const callbacks = [...timers.values()]; timers.clear(); callbacks.forEach(fn => fn()); } };
}

test('unreadable and invalid snapshots do not release an active awake switch', () => {
    const h = harness(); const seen = []; h.watcher.subscribe(v => seen.push(v));
    for (const value of [null, '{', '[]', 'null']) {
        h.set(value); h.replace(); h.flush();
        assert.equal(h.watcher.values.preventSleep, true);
    }
    assert.equal(seen.length, 1);
});

test('directory watch survives repeated replacements and forwards explicit off', () => {
    const h = harness(); assert.equal(h.directoryWatches(), 1);
    h.set('{"preventSleep":false}'); h.replace(); h.flush();
    assert.equal(h.watcher.values.preventSleep, false);
    h.set('{"preventSleep":true}'); h.replace(); h.replace();
    assert.equal(h.timers.size, 1); h.flush();
    assert.equal(h.watcher.values.preventSleep, true);
});

test('disable cancels a queued reload', () => {
    const h = harness(); h.replace(); h.watcher.disable();
    assert.equal(h.timers.size, 0);
});
