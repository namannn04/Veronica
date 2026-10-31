import { readFileSync } from 'node:fs';
import assert from 'node:assert/strict';
import { test } from 'node:test';

function preview({ brokenBuffer = false, upload = true } = {}) {
    const idles = [];
    const errors = [];
    let frames = 0;
    let redraws = 0;
    const source = readFileSync(new URL('./cameraPreview.js', import.meta.url), 'utf8')
        .replace(/^import .*;\n/gm, '')
        .replace(/try \{\n    Gst = [\s\S]*?\n\}\n\nlet gstInitialised/, 'let gstInitialised')
        .replace('export class CameraPreview', 'class CameraPreview');
    const GLib = { Bytes: class { constructor(data) { this.data = data; } },
        PRIORITY_DEFAULT_IDLE: 0, SOURCE_REMOVE: false,
        idle_add: (_p, fn) => { idles.push(fn); return 1; }, Source: { remove() {} } };
    const St = { ImageContent: class { set_bytes(...args) {
        assert.equal(args[0].data.length, 320 * 180 * 4);
        return upload;
    } } };
    const surface = { set_content() {}, queue_redraw() { redraws++; } };
    const CameraPreview = new Function('Clutter', 'Cogl', 'GLib', 'St', 'global', `${source}\nreturn CameraPreview;`)(
        { ContentGravity: { RESIZE_ASPECT: 0 } }, { PixelFormat: { RGBA_8888: 0 } }, GLib, St, {});
    const camera = new CameraPreview(surface, () => { frames++; }, e => errors.push(e));
    const pipeline = { set_state() {} };
    camera._pipeline = pipeline;
    camera._sink = { try_pull_sample() {
        if (brokenBuffer) throw new Error('read failure');
        return { get_buffer: () => ({ get_size: () => 320 * 180 * 4,
            extract_dup: () => new Uint8Array(320 * 180 * 4) }) };
    } };
    // Avoid Gst teardown in this unit harness; check that the error path calls it.
    let stops = 0;
    camera.stop = () => { stops++; camera._pipeline = null; };
    return { camera, errors, flush: () => idles.forEach(fn => fn()),
        frames: () => frames, redraws: () => redraws, stops: () => stops };
}

test('successful texture upload reveals the preview and schedules redraws', () => {
    const h = preview(); h.camera._pullFrame(); h.camera._pullFrame();
    assert.equal(h.frames(), 1);
    assert.equal(h.redraws(), 2);
    assert.deepEqual(h.errors, []);
});

test('a failed texture upload never reports a working preview', () => {
    const h = preview({ upload: false }); h.camera._pullFrame(); h.flush();
    assert.equal(h.frames(), 0); assert.equal(h.stops(), 1);
    assert.match(h.errors[0], /upload/);
});

test('buffer extraction errors stop capture and reach the visible error callback', () => {
    const h = preview({ brokenBuffer: true }); h.camera._pullFrame(); h.flush();
    assert.equal(h.stops(), 1); assert.match(h.errors[0], /read failure/);
});
