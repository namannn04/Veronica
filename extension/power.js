/* Battery, from UPower.
 *
 * The same library gnome-shell's own status/power.js uses. A desktop with no
 * battery reports no devices at all, which is handled as "nothing to show"
 * rather than an error.
 */

import GObject from 'gi://GObject';
import St from 'gi://St';
import UPowerGlib from 'gi://UPowerGlib';

/** UPower device "type" values that represent the machine's own battery. */
const BATTERY_TYPE = 2; // UPowerGlib.DeviceKind.BATTERY

export const PowerIndicator = GObject.registerClass(
class PowerIndicator extends St.BoxLayout {
    _init() {
        super._init({ style_class: 'veronica-status-item', visible: false });

        this._icon = new St.Icon({ style_class: 'veronica-status-icon' });
        this._label = new St.Label({
            style_class: 'veronica-status-label',
            y_align: 2 /* Clutter.ActorAlign.CENTER */,
        });
        this.add_child(this._icon);
        this.add_child(this._label);

        this._client = null;
        this._device = null;
        this._changedId = 0;

        try {
            this._client = UPowerGlib.Client.new();
            this._client.connect('device-added', () => this._findBattery());
            this._client.connect('device-removed', () => this._findBattery());
            this._findBattery();
        } catch (error) {
            // No UPower on this system: the indicator simply never shows.
            console.debug(`veronica: UPower unavailable: ${error}`);
        }
    }

    _findBattery() {
        if (this._device && this._changedId) {
            this._device.disconnect(this._changedId);
            this._changedId = 0;
        }
        this._device = this._client
            .get_devices()
            .find(device => device.kind === BATTERY_TYPE) ?? null;

        if (this._device) {
            this._changedId = this._device.connect('notify', () => this._refresh());
        }
        this._refresh();
    }

    _refresh() {
        if (!this._device) {
            this.visible = false;
            return;
        }
        const percent = Math.round(this._device.percentage);
        const charging = this._device.state === UPowerGlib.DeviceState.CHARGING;
        const critical = this._device.state === UPowerGlib.DeviceState.EMPTY
            || (percent <= 10 && !charging);

        if (!this._logged) {
            this._logged = true;
            console.debug(`veronica: battery ${percent}% (${this._device.state})`);
        }
        this._icon.icon_name = this._iconName(percent, charging);
        this._label.text = `${percent}%`;
        this._label.remove_style_class_name('veronica-band-critical');
        if (critical)
            this._label.add_style_class_name('veronica-band-critical');
        this.visible = true;
    }

    _iconName(percent, charging) {
        const step = Math.max(0, Math.min(100, Math.round(percent / 10) * 10));
        const suffix = charging ? '-charging' : '';
        return `battery-level-${step}${suffix}-symbolic`;
    }

    destroy() {
        if (this._device && this._changedId)
            this._device.disconnect(this._changedId);
        this._client = null;
        this._device = null;
        super.destroy();
    }
});
