/* Small D-Bus bridge used by Veronica's desktop app quick actions.
 *
 * Clean Keys must execute inside GNOME Shell: only Shell can take the modal
 * keyboard grab. WriteClipboard is here for the same reason in reverse — on
 * Wayland only the compositor may set the selection without a focused window,
 * so the app and the CLI reach the clipboard through here rather than needing
 * wl-clipboard installed.
 *
 * Pick Color is exported too, so the notch and the app share one entry point,
 * but the pick itself is `vr color pick`, which records the swatch.
 *
 * PasteInPlace is here for the third variation on the same constraint: only the
 * compositor may synthesise input into a window it does not own. It creates a
 * virtual keyboard on the default seat and sends one Ctrl+V, which is what
 * "paste in place" means — the caller has already put the text on the clipboard.
 * Doing it through Clutter rather than the RemoteDesktop portal means it works
 * on Wayland with no per-session approval dialog.
 */

import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';

export const ACTION_BUS = 'io.github.namannn04.Veronica.Shell';
export const ACTION_PATH = '/io/github/namannn04/Veronica/Shell';
export const ACTION_INTERFACE = 'io.github.namannn04.Veronica.ShellActions';

const XML = `
<node>
  <interface name="${ACTION_INTERFACE}">
    <method name="CleanKeys"/>
    <method name="PickColor"/>
    <method name="ShowClipboard"/>
    <method name="WriteClipboard">
      <arg type="s" direction="in" name="text"/>
    </method>
    <method name="PasteInPlace"/>
  </interface>
</node>`;

export class ActionBridge {
    constructor(actions) {
        this._actions = actions;
        this._ownerId = 0;
        this._object = Gio.DBusExportedObject.wrapJSObject(XML, {
            CleanKeys: () => this._actions?.cleanKeys?.(),
            PickColor: () => this._actions?.pickColor?.(),
            ShowClipboard: () => this._actions?.showClipboard?.(),
            WriteClipboard: text => this._actions?.writeClipboard?.(text),
            PasteInPlace: () => this._pasteInPlace(),
        });
    }

    enable() {
        if (this._ownerId)
            return;
        this._ownerId = Gio.bus_own_name(
            Gio.BusType.SESSION,
            ACTION_BUS,
            Gio.BusNameOwnerFlags.NONE,
            connection => this._object?.export(connection, ACTION_PATH),
            null,
            () => console.debug('veronica: Shell action D-Bus name was lost')
        );
    }

    /**
     * Send one Ctrl+V through a virtual keyboard on the default seat.
     *
     * The device is created per call and disposed straight after: holding one
     * open for the session would keep a keyboard registered on the seat for a
     * feature used once every few minutes.
     *
     * The release events are sent in the reverse order of the presses, and the
     * modifier is released last, so a failure part-way cannot leave Ctrl stuck
     * down — which would make the desktop unusable until the next key press.
     */
    _pasteInPlace() {
        const seat = Clutter.get_default_backend().get_default_seat();
        const keyboard = seat.create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
        const now = global.get_current_time() * 1000;
        try {
            keyboard.notify_keyval(now, Clutter.KEY_Control_L, Clutter.KeyState.PRESSED);
            keyboard.notify_keyval(now, Clutter.KEY_v, Clutter.KeyState.PRESSED);
            keyboard.notify_keyval(now, Clutter.KEY_v, Clutter.KeyState.RELEASED);
        } finally {
            keyboard.notify_keyval(now, Clutter.KEY_Control_L, Clutter.KeyState.RELEASED);
        }
    }

    disable() {
        this._object?.unexport();
        if (this._ownerId)
            Gio.bus_unown_name(this._ownerId);
        this._ownerId = 0;
        this._actions = null;
    }
}
