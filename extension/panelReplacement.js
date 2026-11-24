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
        if (!this._notch) {
            return false;
        }
        return this._notch.pickColor();
    }

    showClipboard() {
        if (!this._notch) {
            return false;
        }
        return this._notch.showClipboard();
    }

    enable(clipboardWatcher, cancellable, settings, keyboardCleaner) {
        if (this._active) {
            return;
        }

        this._hideStock(REPLACED_CLOCK);

        try {
            this._notch = new NotchButton(
                clipboardWatcher, cancellable, settings, keyboardCleaner
            );
            Main.panel.addToStatusArea(NOTCH_ROLE, this._notch, 0, 'center');
        } catch (error) {
            console.debug(`veronica: cannot build the notch clock: ${error}`);
            this._notch?.destroy();
            this._notch = null;
            for (const [name, wasVisible] of this._hidden) {
                const actor = Main.panel.statusArea[name];
                if (actor) {
                    actor.visible = wasVisible;
                }
            }
            this._hidden.clear();
            return;
        }

        this._redirectCalendar();
        this._active = true;
    }

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
        if (!this._stockCalendar) {
            return;
        }
        const panel = Main.panel;
        for (const [name, descriptor] of [
            ['toggleCalendar', this._stockCalendar.toggle],
            ['closeCalendar', this._stockCalendar.close],
        ]) {
            if (descriptor) {
                Object.defineProperty(panel, name, descriptor);
            }
            else {
                delete panel[name];
            }
        }
        this._stockCalendar = null;
    }

    disable() {
        if (!this._active) {
            return;
        }

        this._restoreCalendar();

        if (this._notch) {
            this._notch.destroy();
            this._notch = null;
        }

        for (const [name, wasVisible] of this._hidden) {
            const actor = Main.panel.statusArea[name];
            if (actor) {
                actor.visible = wasVisible;
            }
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
