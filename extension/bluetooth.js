/* Bluetooth status, over BlueZ's D-Bus interface directly.
 *
 * GnomeBluetooth's higher-level client targets device pairing UI, which is
 * more than a status glyph needs, so this reads the adapter and its connected
 * devices straight from org.bluez the same way the shell's own indicator does
 * underneath its abstraction. Configuration (pairing, new devices) opens the
 * system Bluetooth settings rather than being rebuilt here.
 */

import GObject from 'gi://GObject';
import Gio from 'gi://Gio';
import St from 'gi://St';

const BLUEZ_BUS = 'org.bluez';
const OBJECT_MANAGER_PATH = '/';

export const BluetoothIndicator = GObject.registerClass(
class BluetoothIndicator extends St.BoxLayout {
    _init() {
        super._init({
            style_class: 'veronica-status-item',
            reactive: true,
            track_hover: true,
            visible: false,
        });

        this._icon = new St.Icon({ style_class: 'veronica-status-icon' });
        this.add_child(this._icon);

        this._proxy = null;
        this._signalId = 0;
        this.connect('button-press-event', () => this._openSettings());
        this.connect('destroy', () => this._teardown());

        this._connect();
    }

    async _connect() {
        try {
            const connection = Gio.DBus.system;
            this._proxy = await new Promise((resolve, reject) => {
                Gio.DBusProxy.new(
                    connection,
                    Gio.DBusProxyFlags.NONE,
                    null,
                    BLUEZ_BUS,
                    OBJECT_MANAGER_PATH,
                    'org.freedesktop.DBus.ObjectManager',
                    null,
                    (_source, result) => {
                        try {
                            resolve(Gio.DBusProxy.new_finish(result));
                        } catch (error) {
                            reject(error);
                        }
                    }
                );
            });
            this._signalId = this._proxy.connect('g-signal', () => this._refresh());
            this._refresh();
        } catch (error) {
            // bluetoothd not running, or no adapter: nothing to show.
            console.debug(`veronica: BlueZ unavailable: ${error}`);
        }
    }

    _refresh() {
        if (!this._proxy) {
            this.visible = false;
            return;
        }
        this._proxy.call(
            'GetManagedObjects', null, Gio.DBusCallFlags.NONE, -1, null,
            (source, result) => {
                let objects;
                try {
                    [objects] = source.call_finish(result).recursiveUnpack();
                } catch (error) {
                    this.visible = false;
                    return;
                }
                this._applyObjects(objects);
            }
        );
    }

    _applyObjects(objects) {
        let poweredAdapter = false;
        let connectedCount = 0;

        for (const interfaces of Object.values(objects)) {
            const adapter = interfaces['org.bluez.Adapter1'];
            if (adapter?.Powered?.unpack?.())
                poweredAdapter = true;

            const device = interfaces['org.bluez.Device1'];
            if (device?.Connected?.unpack?.())
                connectedCount += 1;
        }

        if (!poweredAdapter) {
            this._icon.icon_name = 'bluetooth-disabled-symbolic';
        } else if (connectedCount > 0) {
            this._icon.icon_name = 'bluetooth-active-symbolic';
        } else {
            this._icon.icon_name = 'bluetooth-symbolic';
        }
        if (!this._logged) {
            this._logged = true;
            console.debug(`veronica: bluetooth powered=${poweredAdapter} connected=${connectedCount}`);
        }
        this.visible = true;
    }

    _openSettings() {
        Gio.Subprocess.new(
            ['gnome-control-center', 'bluetooth'],
            Gio.SubprocessFlags.NONE
        );
    }

    _teardown() {
        if (this._proxy && this._signalId)
            this._proxy.disconnect(this._signalId);
        this._proxy = null;
    }
});
