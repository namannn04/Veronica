/* Live in-notch camera preview.
 *
 * GNOME Shell has no camera widget of its own. GStreamer provides frames via
 * AppSink and St.ImageContent uploads the latest RGBA frame to Clutter. The
 * pipeline is deliberately short-lived: the owning panel stops it whenever
 * the Camera tab or the notch itself is closed.
 */

import Clutter from 'gi://Clutter';
import Cogl from 'gi://Cogl';
import GLib from 'gi://GLib';
import Gst from 'gi://Gst';
// Importing GstApp registers AppSink's pull_sample method with GJS.
import GstApp from 'gi://GstApp';
import St from 'gi://St';

const FRAME_WIDTH = 320;
const FRAME_HEIGHT = 180;
const FRAME_STRIDE = FRAME_WIDTH * 4;

let gstInitialised = false;

function ensureGst() {
    if (gstInitialised)
        return;
    Gst.init(null);
    gstInitialised = true;
}

export class CameraPreview {
    constructor(surface, onFrame, onError) {
        this._surface = surface;
        this._onFrame = onFrame;
        this._onError = onError;
        this._pipeline = null;
        this._sink = null;
        this._bus = null;
        this._busSignalId = 0;
        this._framePollId = 0;
        this._startupTimeoutId = 0;
        this._hasFrame = false;

        this._content = new St.ImageContent({
            preferred_width: FRAME_WIDTH,
            preferred_height: FRAME_HEIGHT,
        });
        this._surface.set_content(this._content);
        this._surface.content_gravity = Clutter.ContentGravity.RESIZE_ASPECT;
    }

    get isRunning() {
        return this._pipeline !== null;
    }

    start() {
        if (this.isRunning)
            return;
        ensureGst();
        this._hasFrame = false;

        try {
            this._pipeline = Gst.parse_launch(
                'v4l2src do-timestamp=true ! videoconvert ! videoscale ! ' +
                `video/x-raw,format=RGBA,width=${FRAME_WIDTH},height=${FRAME_HEIGHT} ! ` +
                'appsink name=veronica_camera_sink emit-signals=false sync=false max-buffers=1 drop=true'
            );
            this._sink = this._pipeline.get_by_name('veronica_camera_sink');
            if (!this._sink)
                throw new Error('The camera preview sink could not be created');

            this._bus = this._pipeline.get_bus();
            this._bus.add_signal_watch();
            this._busSignalId = this._bus.connect('message', (_bus, message) => {
                if (message.type !== Gst.MessageType.ERROR)
                    return;
                const [error] = message.parse_error();
                this._reportError(error?.message || 'The camera could not be opened');
            });

            const result = this._pipeline.set_state(Gst.State.PLAYING);
            if (result === Gst.StateChangeReturn.FAILURE)
                throw new Error('The camera refused to start');
            // AppSink emits new-sample from GStreamer's worker thread, while
            // GJS only permits JS callbacks on Shell's main thread. Polling
            // non-blockingly here is both safe and naturally capped at 15fps.
            this._framePollId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 67, () => {
                this._pullFrame();
                return this.isRunning ? GLib.SOURCE_CONTINUE : GLib.SOURCE_REMOVE;
            });
            // Some broken V4L2 drivers enter PLAYING but never deliver a frame
            // or an error. Fail visibly instead of leaving "Starting camera…"
            // on screen forever.
            this._startupTimeoutId = GLib.timeout_add_seconds(
                GLib.PRIORITY_DEFAULT,
                8,
                () => {
                    this._startupTimeoutId = 0;
                    if (this.isRunning && !this._hasFrame)
                        this._reportError('No camera frame arrived within 8 seconds');
                    return GLib.SOURCE_REMOVE;
                }
            );
        } catch (error) {
            this.stop();
            throw error;
        }
    }

    _pullFrame() {
        if (!this.isRunning)
            return;

        const sample = this._sink.try_pull_sample(0);
        const buffer = sample?.get_buffer();
        if (!buffer)
            return;

        const data = buffer.extract_dup(0, buffer.get_size());
        if (data.length < FRAME_STRIDE * FRAME_HEIGHT)
            return;

        try {
            const bytes = new GLib.Bytes(data);
            const args = [
                bytes,
                Cogl.PixelFormat.RGBA_8888,
                FRAME_WIDTH,
                FRAME_HEIGHT,
                FRAME_STRIDE,
            ];
            // GNOME 48+ requires the compositor's CoglContext explicitly.
            if (this._content.set_bytes.length === 6) {
                const context = global.stage?.context?.get_backend?.()?.get_cogl_context?.();
                if (!context)
                    throw new Error('GNOME could not provide a video rendering context');
                args.unshift(context);
            }
            this._content.set_bytes(...args);
            if (!this._hasFrame) {
                this._hasFrame = true;
                if (this._startupTimeoutId) {
                    GLib.Source.remove(this._startupTimeoutId);
                    this._startupTimeoutId = 0;
                }
                this._onFrame?.();
            }
        } catch (error) {
            this._reportError(`${error}`);
        }
    }

    _reportError(message) {
        GLib.idle_add(GLib.PRIORITY_DEFAULT_IDLE, () => {
            if (!this._surface)
                return GLib.SOURCE_REMOVE;
            this.stop();
            this._onError?.(message);
            return GLib.SOURCE_REMOVE;
        });
    }

    stop() {
        if (this._startupTimeoutId) {
            GLib.Source.remove(this._startupTimeoutId);
            this._startupTimeoutId = 0;
        }
        if (this._framePollId) {
            GLib.Source.remove(this._framePollId);
            this._framePollId = 0;
        }
        if (this._bus && this._busSignalId) {
            this._bus.disconnect(this._busSignalId);
            this._busSignalId = 0;
        }
        if (this._bus)
            this._bus.remove_signal_watch();
        if (this._pipeline)
            this._pipeline.set_state(Gst.State.NULL);
        this._pipeline = null;
        this._sink = null;
        this._bus = null;
        this._hasFrame = false;
    }

    destroy() {
        this.stop();
        this._surface?.set_content(null);
        this._surface = null;
        this._content = null;
        this._onFrame = null;
        this._onError = null;
    }
}

// Keep the namespace import alive; without it GJS does not install AppSink's
// convenience methods on instances returned by Gst.parse_launch().
void GstApp;
