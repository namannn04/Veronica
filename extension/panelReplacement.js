/* Replacing GNOME's own top-bar chrome: the status-area cluster (network,
 * Bluetooth, volume, battery) and the clock/calendar dropdown itself.
 *
 * This is the highest-risk piece of Veronica's top bar integration, so it is
 * gated behind an explicit setting (`topBarReplacement`, off by default) and
 * built to be trivially reversible: every stock actor is only ever hidden,
 * never destroyed, so disabling the extension — or this setting — brings the
 * originals back exactly as they were, with no state lost.
 *
 * The status cluster and the clock are independent: if one fails to build,
 * the other is unaffected.
 */

import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import St from 'gi://St';

import { BluetoothIndicator } from './bluetooth.js';
import { NetworkIndicator } from './network.js';
import { NotchButton } from './notchClock.js';
import { PowerIndicator } from './power.js';
import { VolumeIndicator } from './volume.js';

/**
 * Status-area names being hidden.
 *
 * GNOME 43 folded network, Bluetooth, volume, battery and the rest into a
 * single "Quick Settings" button — `quickSettings` — replacing the older
 * `aggregateMenu`. Hiding the wrong name is not an error, `Main.panel.statusArea`
 * simply returns undefined for it, which is exactly how this shipped once
 * already: no exception, just GNOME's real icons left showing right next to
 * Veronica's, duplicated. There is no schema-version guarantee here, so if a
 * future GNOME renames this again the same failure mode will recur silently.
 */
const REPLACED_STATUS_AREA = 'quickSettings';
const REPLACED_CLOCK = 'dateMenu';
const NOTCH_ROLE = 'veronica-notch';

export class PanelReplacement {
    constructor() {
        this._active = false;
        this._hidden = new Map();
        this._box = null;
        this._notch = null;
    }

    get isActive() {
        return this._active;
    }

    /** Hide the stock chrome and show Veronica's own. Safe to call twice. */
    enable(clipboardWatcher, cancellable) {
        if (this._active)
            return;

        this._hideStock(REPLACED_STATUS_AREA);
        this._hideStock(REPLACED_CLOCK);

        try {
            this._box = new St.BoxLayout({ style_class: 'veronica-status-box' });
            for (const Indicator of [NetworkIndicator, BluetoothIndicator, VolumeIndicator, PowerIndicator]) {
                try {
                    this._box.add_child(new Indicator());
                } catch (error) {
                    // One indicator failing to construct must not take the
                    // others down with it.
                    console.debug(`veronica: cannot build ${Indicator.name}: ${error}`);
                }
            }
            Main.panel._rightBox.add_child(this._box);
        } catch (error) {
            console.debug(`veronica: cannot build the status cluster: ${error}`);
        }

        try {
            this._notch = new NotchButton(clipboardWatcher, cancellable);
            Main.panel.addToStatusArea(NOTCH_ROLE, this._notch, 0, 'center');
        } catch (error) {
            // The status cluster above is independent and stays up even if
            // the clock replacement fails.
            console.debug(`veronica: cannot build the notch clock: ${error}`);
            this._notch = null;
        }

        this._active = true;
    }

    /** Restore the stock chrome exactly as it was. Safe to call twice. */
    disable() {
        if (!this._active)
            return;

        this._box?.destroy();
        this._box = null;

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
