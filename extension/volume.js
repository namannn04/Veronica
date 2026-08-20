/* Volume, through Gvc — the same PipeWire/PulseAudio mixer binding
 * gnome-shell's own status/volume.js uses, so behaviour matches exactly what
 * the stock indicator does: scroll to adjust, click to mute.
 */

import Clutter from 'gi://Clutter';
import GObject from 'gi://GObject';
import Gvc from 'gi://Gvc';
import St from 'gi://St';

/** Volume step per scroll notch, matching the stock indicator's feel. */
const VOLUME_STEP = 0.05;

export const VolumeIndicator = GObject.registerClass(
class VolumeIndicator extends St.BoxLayout {
    _init() {
        super._init({
            style_class: 'veronica-status-item',
            reactive: true,
            track_hover: true,
            visible: false,
        });

        this._icon = new St.Icon({ style_class: 'veronica-status-icon' });
        this.add_child(this._icon);

        this._control = null;
        this._sink = null;
        this._notifyIds = [];

        try {
            this._control = new Gvc.MixerControl({ name: 'Veronica' });
            // The id `default-sink-changed` carries is not trustworthy — Gvc
            // signals -1 while it is still resolving the sink, sometimes more
            // than once. Asking the control for its current default sink
            // directly is what gnome-shell's own volume indicator does, and it
            // is correct regardless of what the signal argument says.
            this._control.connect('default-sink-changed', () => this._trackSink());
            this._control.connect('state-changed', () => this._trackSink());
            this._control.open();
        } catch (error) {
            console.debug(`veronica: Gvc unavailable: ${error}`);
        }

        this.connect('scroll-event', (_actor, event) => this._onScroll(event));
        this.connect('button-press-event', () => this._toggleMute());
    }

    _trackSink() {
        for (const notifyId of this._notifyIds)
            this._sink?.disconnect(notifyId);
        this._notifyIds = [];

        this._sink = this._control.get_default_sink() ?? null;
        if (this._sink) {
            this._notifyIds.push(this._sink.connect('notify::volume', () => this._refresh()));
            this._notifyIds.push(this._sink.connect('notify::is-muted', () => this._refresh()));
        }
        this._refresh();
    }

    _refresh() {
        if (!this._sink) {
            this.visible = false;
            return;
        }
        const max = this._control.get_vol_max_norm();
        const fraction = max > 0 ? this._sink.volume / max : 0;
        const muted = this._sink.is_muted;
        if (!this._logged) {
            this._logged = true;
            console.debug(`veronica: volume ${Math.round(fraction * 100)}%${muted ? ' (muted)' : ''}`);
        }
        this._icon.icon_name = this._iconName(fraction, muted);
        this.visible = true;
    }

    _iconName(fraction, muted) {
        if (muted || fraction <= 0)
            return 'audio-volume-muted-symbolic';
        if (fraction < 0.34)
            return 'audio-volume-low-symbolic';
        if (fraction < 0.67)
            return 'audio-volume-medium-symbolic';
        return 'audio-volume-high-symbolic';
    }

    _onScroll(event) {
        if (!this._sink)
            return Clutter.EVENT_PROPAGATE;
        const direction = event.get_scroll_direction();
        const max = this._control.get_vol_max_norm();
        const delta = max * VOLUME_STEP;
        if (direction === Clutter.ScrollDirection.UP)
            this._sink.volume = Math.min(max, this._sink.volume + delta);
        else if (direction === Clutter.ScrollDirection.DOWN)
            this._sink.volume = Math.max(0, this._sink.volume - delta);
        else
            return Clutter.EVENT_PROPAGATE;
        this._sink.push_volume();
        return Clutter.EVENT_STOP;
    }

    _toggleMute() {
        if (this._sink)
            this._sink.change_is_muted(!this._sink.is_muted);
    }

    destroy() {
        for (const notifyId of this._notifyIds)
            this._sink?.disconnect(notifyId);
        this._control = null;
        this._sink = null;
        super.destroy();
    }
});
