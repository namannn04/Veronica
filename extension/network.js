/* Network status, through NetworkManager's own GObject binding.
 *
 * Shows connectivity at a glance: wired, wifi with signal strength, or
 * offline. Reimplementing the access-point picker is out of scope — clicking
 * opens the system network settings, which already does that well.
 */

import GObject from 'gi://GObject';
import Gio from 'gi://Gio';
import NM from 'gi://NM';
import St from 'gi://St';

export const NetworkIndicator = GObject.registerClass(
class NetworkIndicator extends St.BoxLayout {
    _init() {
        super._init({
            style_class: 'veronica-status-item',
            reactive: true,
            track_hover: true,
            visible: false,
        });

        this._icon = new St.Icon({ style_class: 'veronica-status-icon' });
        this.add_child(this._icon);

        this._client = null;
        this._signalIds = [];

        this.connect('button-press-event', () => this._openSettings());

        NM.Client.new_async(null, (_source, result) => {
            try {
                this._client = NM.Client.new_finish(result);
                this._signalIds.push(
                    this._client.connect('notify::primary-connection', () => this._refresh()),
                    this._client.connect('notify::state', () => this._refresh()),
                    this._client.connect('notify::connectivity', () => this._refresh()),
                );
                this._refresh();
            } catch (error) {
                console.debug(`veronica: NetworkManager unavailable: ${error}`);
            }
        }, null);
    }

    _refresh() {
        if (!this._client) {
            this.visible = false;
            return;
        }
        const primary = this._client.get_primary_connection();
        if (!primary) {
            this._icon.icon_name = 'network-offline-symbolic';
            this.visible = true;
            return;
        }

        const activeDevice = this._client
            .get_devices()
            .find(device => device.get_active_connection()?.get_uuid() === primary.get_uuid());

        const iconName = this._iconName(activeDevice, primary.get_connection_type());
        if (!this._logged) {
            this._logged = true;
            console.debug(`veronica: network ${primary.get_connection_type()} -> ${iconName}`);
        }
        this._icon.icon_name = iconName;
        this.visible = true;
    }

    _iconName(device, connectionType) {
        if (connectionType === '802-3-ethernet')
            return 'network-wired-symbolic';

        if (device && device instanceof NM.DeviceWifi) {
            const ap = device.get_active_access_point();
            const strength = ap ? ap.get_strength() : 0;
            const step = strength > 80 ? 'excellent'
                : strength > 55 ? 'good'
                : strength > 30 ? 'ok'
                : strength > 5 ? 'weak'
                : 'none';
            return `network-wireless-signal-${step}-symbolic`;
        }

        if (connectionType?.startsWith('vpn') || connectionType === 'wireguard')
            return 'network-vpn-symbolic';

        return 'network-wired-symbolic';
    }

    _openSettings() {
        Gio.Subprocess.new(
            ['gnome-control-center', 'network'],
            Gio.SubprocessFlags.NONE
        );
    }

    destroy() {
        for (const id of this._signalIds)
            this._client?.disconnect(id);
        this._client = null;
        super.destroy();
    }
});
