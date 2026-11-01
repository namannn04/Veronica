import { readFileSync } from 'node:fs';
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { spawnSync } from 'node:child_process';

function preview({ brokenBuffer = false, upload = true, width = 320, height = 180, stride = width * 4 } = {}) {
    const idles = [];
    const errors = [];
    let frames = 0;
    let redraws = 0;
    const sizes = [];
    const flips = [];
    const source = readFileSync(new URL('./cameraPreview.js', import.meta.url), 'utf8')
        .replace(/^import .*;\n/gm, '')
        .replace(/try \{\n    Gst = [\s\S]*?\n\}\n\nlet gstInitialised/, 'let gstInitialised')
        .replace(/export (class|function) /g, '$1 ')
        .replace('let GstVideo = null;', 'let GstVideo = testVideo;');
    const GLib = { Bytes: class { constructor(data) { this.data = data; } },
        PRIORITY_DEFAULT_IDLE: 0, SOURCE_REMOVE: false,
        idle_add: (_p, fn) => { idles.push(fn); return 1; }, Source: { remove() {} } };
    const St = { ImageContent: class {
        set_preferred_width() {} set_preferred_height() {}
        set_bytes(...args) {
        assert.equal(args[0].data.length, stride * height);
        assert.equal(args[2], width); assert.equal(args[3], height); assert.equal(args[4], stride);
        return upload;
    } } };
    const surface = { set_content() {}, queue_redraw() { redraws++; } };
    const { CameraPreview, cameraPipeline } = new Function('Clutter', 'Cogl', 'GLib', 'St', 'global', 'testVideo', `${source}\nreturn { CameraPreview, cameraPipeline };`)(
        { ContentGravity: { RESIZE_ASPECT: 0 } }, { PixelFormat: { RGBA_8888: 0 } }, GLib, St, {}, { VideoInfo: { new_from_caps: () => ({ width, height, stride: [stride], offset: [0] }) } });
    const camera = new CameraPreview(surface, () => { frames++; }, e => errors.push(e), size => sizes.push(size));
    const pipeline = { set_state() {}, get_by_name: () => ({ set_property: (_name, value) => flips.push(value) }) };
    camera._pipeline = pipeline;
    camera._sink = { try_pull_sample() {
        if (brokenBuffer) throw new Error('read failure');
        return { get_caps: () => ({}), get_buffer: () => ({ get_size: () => stride * height,
            extract_dup: () => new Uint8Array(stride * height) }) };
    } };
    // Avoid Gst teardown in this unit harness; check that the error path calls it.
    let stops = 0;
    camera.stop = () => { stops++; camera._pipeline = null; };
    return { camera, pipeline: cameraPipeline, errors, sizes, flips, flush: () => idles.forEach(fn => fn()),
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


test('4:3 and widescreen frames keep their source proportions within the preview bounds', () => {
    for (const [width, height] of [[320,240],[320,180],[320,320]]) {
        const h = preview({width,height}); h.camera._pullFrame();
        const size = h.sizes[0];
        assert.ok(Math.abs(size.width / size.height - width / height) < 1e-9);
        assert.ok(size.width <= 292 && size.height <= 164);
    }
});

test('horizontal correction is explicit and can be switched off for other cameras', () => {
    const h = preview(); h.camera.setFlipHorizontal(true); h.camera.setFlipHorizontal(false);
    assert.deepEqual(h.flips,[4,0]);
});

test('padded RGBA scanlines retain the negotiated stride', () => {
    const h = preview({width:320,height:240,stride:1296}); h.camera._pullFrame();
    assert.equal(h.frames(),1); assert.deepEqual(h.errors,[]);
});


test('real GStreamer preserves source geometry and reverses pixels only when requested', t => {
    const {pipeline} = preview();
    const script = `
import sys
try:
 import gi
 gi.require_version('Gst','1.0'); gi.require_version('GstVideo','1.0')
 from gi.repository import Gst,GstVideo
except (ImportError,ValueError):
 sys.exit(77)
Gst.init(None)
for width,height,expected_height in [(640,480,240),(1280,720,180)]:
 rows=[]
 for source in sys.argv[1:]:
  source=source.replace('v4l2src do-timestamp=true',f'videotestsrc num-buffers=1 pattern=smpte ! video/x-raw,width={width},height={height},pixel-aspect-ratio=1/1')
  pipeline=Gst.parse_launch(source)
  try:
   pipeline.set_state(Gst.State.PLAYING)
   sample=pipeline.get_by_name('veronica_camera_sink').emit('try-pull-sample',2*Gst.SECOND)
   assert sample is not None
   info=GstVideo.VideoInfo.new_from_caps(sample.get_caps())
   assert (info.width,info.height,info.par_n,info.par_d)==(320,expected_height,1,1)
   buffer=sample.get_buffer();data=buffer.extract_dup(0,buffer.get_size())
   rows.append(data[info.offset[0]:info.offset[0]+info.width*4])
  finally: pipeline.set_state(Gst.State.NULL)
 assert rows[1]==b''.join(rows[0][i:i+4] for i in range(len(rows[0])-4,-1,-4))
`;
    const result = spawnSync('python3',['-c',script,pipeline(false),pipeline(true)],{encoding:'utf8',timeout:15000});
    if (result.status === 77 || result.error?.code === 'ENOENT') return t.skip('GStreamer introspection is unavailable');
    assert.equal(result.status,0,result.stderr || String(result.error));
});
