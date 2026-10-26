/* Now-playing card: title, artist, and transport, from `vr media`.
 *
 * MPRIS itself is read by the application's Rust code, not by this extension;
 * this just displays the answer and forwards transport clicks to `vr`.
 */

import Clutter from 'gi://Clutter';
import GLib from 'gi://GLib';
import St from 'gi://St';

import { runJson, vbox } from './lib.js';

/** How long a player takes to publish the next track after a skip. */
const SETTLE_MS = 600;

export class NowPlayingCard {
    /** @param {(visible: boolean) => void} [onVisibilityChanged] */
    constructor(onVisibilityChanged = null) {
        this._onVisibilityChanged = onVisibilityChanged;
        this._cancellable = null;
        this._settleId = 0;
        this.actor = new St.BoxLayout({
            style_class: 'veronica-card veronica-now-playing',
            visible: false,
        });
        this._private = false;
        this._lastTitle = '';
        this._lastArtist = '';

        this._art = new St.Icon({
            icon_name: 'emblem-music-symbolic',
            style_class: 'veronica-now-art',
            y_align: Clutter.ActorAlign.CENTER,
        });
        this.actor.add_child(this._art);

        const text = vbox({
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
        this._previous = this._transportButton('media-skip-backward-symbolic', () => this._send('previous'));
        this._next = this._transportButton('media-skip-forward-symbolic', () => this._send('next'));
        controls.add_child(this._previous);
        controls.add_child(this._transportButton(null, () => this._send('toggle'), this._playPauseIcon));
        controls.add_child(this._next);
        this.actor.add_child(controls);
    }

    _transportButton(iconName, onClicked, existingIcon) {
        const icon = existingIcon ?? new St.Icon({ icon_name: iconName, style_class: 'veronica-now-icon' });
        const button = new St.Button({ style_class: 'veronica-now-btn', child: icon });
        button.connect('clicked', onClicked);
        return button;
    }

    /**
     * Forward a transport click and show what it did.
     *
     * `vr media` answers with the player's state after the action, which is
     * rendered at once. A player publishes the next track's metadata a moment
     * after a skip, so the card reads it once more when that has settled;
     * before, the old title stayed until the notch was reopened.
     */
    async _send(action) {
        const playing = await runJson(['media', action], this._cancellable);
        if (!this.actor)
            return;
        this._render(playing);
        if (this._settleId)
            GLib.Source.remove(this._settleId);
        this._settleId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, SETTLE_MS, () => {
            this._settleId = 0;
            this.refresh(this._cancellable).catch(() => {});
            return GLib.SOURCE_REMOVE;
        });
    }

    async refresh(cancellable) {
        this._cancellable = cancellable;
        const playing = await runJson(['media', 'status'], cancellable);
        if (!this.actor)
            return; // destroyed while the subprocess ran
        this._render(playing);
    }

    _render(playing) {
        if (!playing) {
            this._setVisible(false);
            return;
        }
        this._lastTitle = playing.title || 'Untitled';
        this._lastArtist = playing.artist || playing.identity || '';
        this._title.text = this._private ? 'Private' : this._lastTitle;
        this._artist.text = this._private ? 'Presenter mode' : this._lastArtist;
        this._playPauseIcon.icon_name = playing.status === 'playing'
            ? 'media-playback-pause-symbolic'
            : 'media-playback-start-symbolic';
        // A player that cannot skip (a browser tab often cannot) gets dimmed,
        // inert buttons rather than ones that silently do nothing.
        this._setEnabled(this._next, playing.canGoNext !== false);
        this._setEnabled(this._previous, playing.canGoPrevious !== false);
        this._setVisible(true);
    }

    _setEnabled(button, enabled) {
        button.reactive = enabled;
        button.can_focus = enabled;
        button.opacity = enabled ? 255 : 90;
    }

    _setVisible(visible) {
        this.actor.visible = visible;
        this._onVisibilityChanged?.(visible);
    }

    setPrivate(privateMode) {
        this._private = privateMode;
        if (!this.actor)
            return;
        this._title.text = privateMode ? 'Private' : this._lastTitle;
        this._artist.text = privateMode ? 'Presenter mode' : this._lastArtist;
        this.actor.set_style_class_name(
            privateMode
                ? 'veronica-card veronica-now-playing presenter-private'
                : 'veronica-card veronica-now-playing'
        );
    }

    destroy() {
        if (this._settleId) {
            GLib.Source.remove(this._settleId);
            this._settleId = 0;
        }
        this._onVisibilityChanged = null;
        this.actor?.destroy();
        this.actor = null;
    }
}
