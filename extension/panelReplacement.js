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

    pickColor() {
        if (!this._notch)
            return false;
        return this._notch.pickColor();
    }

    showClipboard() {
        if (!this._notch)
            return false;
        return this._notch.showClipboard();
    }

    /** Hide only the stock date menu and show Veronica's own. Safe to call twice. */
    enable(clipboardWatcher, cancellable, settings, keyboardCleaner) {
        if (this._active)
            return;

        this._hideStock(REPLACED_CLOCK);

        try {
            this._notch = new NotchButton(
                clipboardWatcher, cancellable, settings, keyboardCleaner
            );
            Main.panel.addToStatusArea(NOTCH_ROLE, this._notch, 0, 'center');
        } catch (error) {
            console.debug(`veronica: cannot build the notch clock: ${error}`);
            // Built but not placed: it still owns timers and subscriptions.
            this._notch?.destroy();
            this._notch = null;
            for (const [name, wasVisible] of this._hidden) {
                const actor = Main.panel.statusArea[name];
                if (actor)
                    actor.visible = wasVisible;
            }
            this._hidden.clear();
            return;
        }

        this._redirectCalendar();
        this._active = true;
    }

    /**
     * Point the shell's calendar calls at the notch while it stands in for
     * the date menu.
     *
     * Super+V calls `Main.panel.toggleCalendar()`, and clicking a notification
     * or a media player calls `closeCalendar()`. Both target the hidden
     * `dateMenu`, whose `_toggleMenu`/`_closeMenu` bail out because it is not
     * mapped: the shortcut did nothing and the notch stayed open after a click.
     */
    _redirectCalendar() {
        const panel = Main.panel;
        this._stockCalendar = {
            toggle: Object.getOwnPropertyDescriptor(panel, 'toggleCalendar'),
            close: Object.getOwnPropertyDescriptor(panel, 'closeCalendar'),
        };
        panel.toggleCalendar = () => this._notch?.toggleNotifications();
        panel.closeCalendar = () => this._notch?.closeMenu();
    }

    _restoreCalendar() {
        if (!this._stockCalendar)
            return;
        const panel = Main.panel;
        // The originals live on the prototype; deleting the own property
        // uncovers them unless something had already set its own before us.
        for (const [name, descriptor] of [
            ['toggleCalendar', this._stockCalendar.toggle],
            ['closeCalendar', this._stockCalendar.close],
        ]) {
            if (descriptor)
                Object.defineProperty(panel, name, descriptor);
            else
                delete panel[name];
        }
        this._stockCalendar = null;
    }

    /** Restore the stock chrome exactly as it was. Safe to call twice. */
    disable() {
        if (!this._active)
            return;

        this._restoreCalendar();

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
