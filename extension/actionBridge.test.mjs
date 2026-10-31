import { readFileSync } from 'node:fs';
import assert from 'node:assert/strict';
import { test } from 'node:test';

function harness() {
    const timers = new Map();
    let next = 1;
    let focusChanged;
    let pastes = 0;
    const display = {
        focus_window: null,
        connect: (_signal, callback) => { focusChanged = callback; return 1; },
        disconnect: () => { focusChanged = null; },
    };
    const window = title => ({
        above: false, live: true,
        get_title: () => title,
        get_compositor_private() { return this.live; },
        make_above() { this.above = true; },
        focus() { display.focus_window = this; focusChanged?.(); },
    });
    const Gio = {
        DBusExportedObject: { wrapJSObject: () => ({ export() {}, unexport() {} }) },
        BusType: { SESSION: 0 }, BusNameOwnerFlags: { NONE: 0 }, bus_own_name: () => 1,
        bus_unown_name() {},
    };
    const GLib = {
        PRIORITY_DEFAULT: 0, SOURCE_REMOVE: false,
        timeout_add: (_priority, _delay, fn) => { const id = next++; timers.set(id, fn); return id; },
        Source: { remove: id => timers.delete(id) },
    };
    const source = readFileSync(new URL('./actionBridge.js', import.meta.url), 'utf8')
        .replace(/^import .*;\n/gm, '').replace(/export (const|class) /g, '$1 ');
    const Bridge = new Function('Clutter', 'Gio', 'GLib', 'global', `${source}\nreturn ActionBridge;`)(
        {}, Gio, GLib, { display, get_current_time: () => 1 });
    const bridge = new Bridge({});
    bridge._pasteInPlace = () => { pastes++; assert.notEqual(display.focus_window, picker); };
    const target = window('Editor');
    const picker = window('Veronica Emoji Picker');
    target.focus(); bridge.enable(); picker.focus();
    const invocation = () => ({
        result: null,
        return_value() { this.result = 'ok'; },
        return_dbus_error(_name, message) { this.result = message; },
    });
    const tick = () => {
        const [id, fn] = timers.entries().next().value;
        timers.delete(id); fn();
    };
    return { bridge, target, picker, window, display, timers, tick, invocation, pastes: () => pastes };
}

test('picker stays above while repeated paste focuses the editor and returns', () => {
    const h = harness();
    assert.equal(h.picker.above, true);
    for (let i = 0; i < 3; i++) {
        const call = h.invocation(); h.bridge._pasteEmoji(call);
        assert.equal(h.display.focus_window, h.target);
        h.tick(); h.tick();
        assert.equal(call.result, 'ok');
        assert.equal(h.display.focus_window, h.picker);
    }
    assert.equal(h.pastes(), 3);
});

test('missing target fails without sending keys', () => {
    const h = harness(); h.target.live = false;
    const call = h.invocation(); h.bridge._pasteEmoji(call);
    assert.match(call.result, /Focus the text field/);
    assert.equal(h.pastes(), 0);
});

test('focus changed by the user prevents paste into another application', () => {
    const h = harness(); const call = h.invocation(); h.bridge._pasteEmoji(call);
    h.window('Other app').focus(); h.tick();
    assert.match(call.result, /could not receive focus/);
    assert.equal(h.pastes(), 0);
});

test('disable cancels pending paste and releases the D-Bus invocation', () => {
    const h = harness(); const call = h.invocation(); h.bridge._pasteEmoji(call);
    h.bridge.disable();
    assert.equal(h.timers.size, 0);
    assert.match(call.result, /disabled/);
});
