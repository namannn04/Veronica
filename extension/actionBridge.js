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
 */

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

    disable() {
        this._object?.unexport();
        if (this._ownerId)
            Gio.bus_unown_name(this._ownerId);
        this._ownerId = 0;
        this._actions = null;
    }
}
