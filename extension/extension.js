/* Veronica's GNOME Shell entry point.
 *
 * GNOME already owns Wi-Fi, Bluetooth, volume, battery and Quick Settings.
 * Veronica never creates copies of those actors and never inserts content into
 * GNOME's calendar/notification dropdown. It replaces only the centre clock
 * button with the compact Edith shelf; disabling the extension restores the
 * stock clock and its original menu unchanged.
 */

import Gio from 'gi://Gio';

import { Extension } from 'resource:///org/gnome/shell/extensions/extension.js';

import { ClipboardWatcher } from './clipboard.js';
import { FocusDim } from './focusDim.js';
import { ActionBridge } from './actionBridge.js';
import { AttentionTracker } from './attention.js';
import { findCli } from './lib.js';
import { PanelReplacement } from './panelReplacement.js';
import { SettingsWatcher } from './settings.js';

export default class VeronicaExtension extends Extension {
    enable() {
        this._cancellable = new Gio.Cancellable();

        // One watcher for the whole extension, so anything needing a stored
        // value reacts to a change rather than polling for one.
        this._settings = new SettingsWatcher();
        this._settings.enable();

        // Dimming behind the focused window needs the compositor, which is why
        // it lives here rather than in the app.
        this._focusDim = new FocusDim(this._settings);
        this._focusDim.enable();

        this._attention = new AttentionTracker(this._settings);
        this._attention.enable();

        this._clipboard = new ClipboardWatcher();
        if (this._clipboard.enable())
            console.debug('veronica: watching the clipboard');

        this._panelReplacement = new PanelReplacement();
        this._panelReplacement.enable(this._clipboard, this._cancellable, this._settings);
        this._actionBridge = new ActionBridge({
            cleanKeys: () => this._panelReplacement?.cleanKeys(),
            pickColor: () => this._panelReplacement?.pickColor(),
            // Silent: the caller reports the copy, so a second banner from
            // inside the shell would be a duplicate.
            writeClipboard: text => this._clipboard?.write(text, false),
        });
        this._actionBridge.enable();
        console.debug(`veronica: compact Edith notch enabled; vr at "${findCli() || 'not found'}"`);
    }

    disable() {
        this._attention?.disable();
        this._attention = null;
        this._actionBridge?.disable();
        this._actionBridge = null;

        this._cancellable?.cancel();
        this._cancellable = null;

        this._panelReplacement?.disable();
        this._panelReplacement = null;

        this._clipboard?.disable();
        this._clipboard = null;

        this._focusDim?.disable();
        this._focusDim = null;

        this._settings?.disable();
        this._settings = null;
    }
}
