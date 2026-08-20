/* Replacing the status-area cluster: network, bluetooth, volume, battery.
 *
 * This is the highest-risk piece of Veronica's top bar integration, so it is
 * gated behind an explicit setting (`topBarReplacement`, off by default) and
 * built to be trivially reversible: the stock `aggregateMenu` is only ever
 * hidden, never destroyed, so disabling the extension — or this setting —
 * brings the original icons back exactly as they were, with no state lost.
 *
 * Each indicator degrades independently: if one GObject binding is missing,
 * that indicator simply never becomes visible, and the others are unaffected.
 */

import St from 'gi://St';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';

import { BluetoothIndicator } from './bluetooth.js';
import { NetworkIndicator } from './network.js';
import { PowerIndicator } from './power.js';
import { VolumeIndicator } from './volume.js';

/** Indicators replaced, and the shell's name for the actor being hidden. */
const REPLACED_STATUS_AREA = 'aggregateMenu';

export class PanelReplacement {
    constructor() {
        this._active = false;
        this._hiddenActor = null;
        this._wasVisible = true;
        this._box = null;
    }

    get isActive() {
        return this._active;
    }

    /** Hide the stock cluster and show Veronica's own. Safe to call twice. */
    enable() {
        if (this._active)
            return;

        const stock = Main.panel.statusArea[REPLACED_STATUS_AREA];
        if (stock) {
            this._hiddenActor = stock;
            this._wasVisible = stock.visible;
            stock.hide();
        }

        this._box = new St.BoxLayout({ style_class: 'veronica-status-box' });
        for (const Indicator of [NetworkIndicator, BluetoothIndicator, VolumeIndicator, PowerIndicator]) {
            try {
                this._box.add_child(new Indicator());
            } catch (error) {
                // One indicator failing to construct must not take the others
                // down with it.
                console.debug(`veronica: cannot build ${Indicator.name}: ${error}`);
            }
        }
        Main.panel._rightBox.add_child(this._box);

        this._active = true;
    }

    /** Restore the stock cluster exactly as it was. Safe to call twice. */
    disable() {
        if (!this._active)
            return;

        this._box?.destroy();
        this._box = null;

        if (this._hiddenActor) {
            this._hiddenActor.visible = this._wasVisible;
            this._hiddenActor = null;
        }

        this._active = false;
    }
}
