/* Replace GNOME's clock/calendar dropdown with Veronica's compact notch.
 *
 * Ubuntu's Quick Settings already owns network, Bluetooth, volume and battery.
 * Veronica deliberately leaves that status area alone; drawing another copy
 * loses the native dropdown and produces duplicate icons beside the notch.
 * Only the stock clock is hidden, and disabling the feature restores it.
 */

import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import { NotchButton } from './notchClock.js';
const REPLACED_CLOCK = 'dateMenu';
const NOTCH_ROLE = 'veronica-notch';

export class PanelReplacement {
    constructor() {
        this._active = false;
        this._hidden = new Map();
        this._notch = null;
    }

    get isActive() {
        return this._active;
    }

    cleanKeys() {
        this._notch?.cleanKeys();
    }

    pickColor() {
        this._notch?.pickColor();
    }

    showClipboard() {
        this._notch?.showClipboard();
    }

    /** Hide only the stock date menu and show Veronica's own. Safe to call twice. */
    enable(clipboardWatcher, cancellable, settings) {
        if (this._active)
            return;

        this._hideStock(REPLACED_CLOCK);

        try {
            this._notch = new NotchButton(clipboardWatcher, cancellable, settings);
            Main.panel.addToStatusArea(NOTCH_ROLE, this._notch, 0, 'center');
        } catch (error) {
            console.debug(`veronica: cannot build the notch clock: ${error}`);
            this._notch = null;
        }

        this._active = true;
    }

    /** Restore the stock chrome exactly as it was. Safe to call twice. */
    disable() {
        if (!this._active)
            return;

        if (this._notch) {
            // addToStatusArea already parents the button; destroying it is
            // enough, there is nothing further to detach.
            this._notch.destroy();
            this._notch = null;
        }

        for (const [name, wasVisible] of this._hidden) {
            const actor = Main.panel.statusArea[name];
            if (actor)
                actor.visible = wasVisible;
        }
        this._hidden.clear();

        this._active = false;
    }

    _hideStock(name) {
        const actor = Main.panel.statusArea[name];
        if (actor) {
            this._hidden.set(name, actor.visible);
            actor.hide();
        }
    }
}
