/* Keyboard-cleaning modal, independent of the optional top-bar shelf. */

import Clutter from 'gi://Clutter';
import GLib from 'gi://GLib';
import St from 'gi://St';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';

export class KeyboardCleaner {
    constructor() {
        this._overlay = null;
        this._grab = null;
        this._idleId = 0;
    }

    start() {
        if (this._overlay || this._idleId)
            return;
        this._idleId = GLib.idle_add(GLib.PRIORITY_DEFAULT_IDLE, () => {
            this._idleId = 0;
            if (this._overlay)
                return GLib.SOURCE_REMOVE;

            const overlay = new St.Widget({
                style_class: 'veronica-clean-overlay',
                reactive: true,
                can_focus: true,
                layout_manager: new Clutter.BinLayout(),
            });
            overlay.set_position(0, 0);
            overlay.set_size(global.stage.width, global.stage.height);
            overlay.connect('key-press-event', () => Clutter.EVENT_STOP);
            overlay.connect('key-release-event', () => Clutter.EVENT_STOP);

            const card = new St.BoxLayout({
                orientation: Clutter.Orientation.VERTICAL,
                style_class: 'veronica-clean-card',
                x_align: Clutter.ActorAlign.CENTER,
                y_align: Clutter.ActorAlign.CENTER,
            });
            card.add_child(new St.Icon({ icon_name: 'input-keyboard-symbolic' }));
            card.add_child(new St.Label({ text: 'Keyboard cleaning mode' }));
            card.add_child(new St.Label({
                text: 'Keys are blocked. Use the pointer to finish.',
                style_class: 'veronica-clean-detail',
            }));
            const done = new St.Button({
                style_class: 'veronica-clean-done',
                label: 'Done',
                can_focus: true,
            });
            done.connect('clicked', () => this.stop());
            card.add_child(done);
            overlay.add_child(card);

            this._overlay = overlay;
            Main.layoutManager.addTopChrome(overlay);
            this._grab = Main.pushModal(overlay);
            return GLib.SOURCE_REMOVE;
        });
    }

    stop() {
        if (this._idleId) {
            GLib.Source.remove(this._idleId);
            this._idleId = 0;
        }
        if (this._grab) {
            Main.popModal(this._grab);
            this._grab = null;
        }
        if (this._overlay) {
            Main.layoutManager.removeChrome(this._overlay);
            this._overlay.destroy();
            this._overlay = null;
        }
    }
}
