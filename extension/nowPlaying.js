/* Now-playing card: title, artist, and transport, from `vr media`.
 *
 * MPRIS itself is read by the application's Rust code, not by this extension;
 * this just displays the answer and forwards transport clicks to `vr`.
 */

import Clutter from 'gi://Clutter';
import St from 'gi://St';

import { runJson } from './lib.js';

export class NowPlayingCard {
    constructor() {
        this.actor = new St.BoxLayout({
            style_class: 'veronica-card veronica-now-playing',
            visible: false,
        });

        this._art = new St.Icon({
            icon_name: 'emblem-music-symbolic',
            style_class: 'veronica-now-art',
        });
        this.actor.add_child(this._art);

        const text = new St.BoxLayout({
            orientation: Clutter.Orientation.VERTICAL,
            x_expand: true,
            y_align: Clutter.ActorAlign.CENTER,
            style_class: 'veronica-now-text',
        });
        this._title = new St.Label({ style_class: 'veronica-now-title' });
        this._artist = new St.Label({ style_class: 'veronica-now-artist' });
        text.add_child(this._title);
        text.add_child(this._artist);
        this.actor.add_child(text);

        const controls = new St.BoxLayout({
            style_class: 'veronica-now-controls',
            y_align: Clutter.ActorAlign.CENTER,
        });
        this._playPauseIcon = new St.Icon({
            icon_name: 'media-playback-start-symbolic',
            style_class: 'veronica-now-icon',
        });
        controls.add_child(this._transportButton('media-skip-backward-symbolic', () => this._send('previous')));
        controls.add_child(this._transportButton(null, () => this._send('toggle'), this._playPauseIcon));
        controls.add_child(this._transportButton('media-skip-forward-symbolic', () => this._send('next')));
        this.actor.add_child(controls);
    }

    _transportButton(iconName, onClicked, existingIcon) {
        const icon = existingIcon ?? new St.Icon({ icon_name: iconName, style_class: 'veronica-now-icon' });
        const button = new St.Button({ style_class: 'veronica-now-btn', child: icon });
        button.connect('clicked', onClicked);
        return button;
    }

    _send(action) {
        runJson(['media', action]).catch(() => {});
        // Optimistic: reflect the likely new state immediately rather than
        // waiting for the next poll, which the caller may not trigger for a
        // while.
        if (action === 'toggle') {
            const playing = this._playPauseIcon.icon_name === 'media-playback-start-symbolic';
            this._playPauseIcon.icon_name = playing
                ? 'media-playback-pause-symbolic'
                : 'media-playback-start-symbolic';
        }
    }

    async refresh(cancellable) {
        const playing = await runJson(['media', 'status'], cancellable);
        if (!this.actor)
            return; // destroyed while the subprocess ran
        if (!playing) {
            this.actor.visible = false;
            return;
        }
        this._title.text = playing.title || 'Untitled';
        this._artist.text = playing.artist || playing.identity || '';
        this._playPauseIcon.icon_name = playing.status === 'playing'
            ? 'media-playback-pause-symbolic'
            : 'media-playback-start-symbolic';
        this.actor.visible = true;
    }

    destroy() {
        this.actor?.destroy();
        this.actor = null;
    }
}
