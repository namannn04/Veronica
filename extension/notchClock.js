/* The notch: Veronica's own replacement for the clock and its dropdown.
 *
 * The menu is intentionally not GNOME's date/calendar popup with extra rows.
 * It is the small 580px Edith shelf: tabs, now playing, two limit rings and
 * quick actions. Notifications remain real GNOME widgets, available from the
 * shelf's bell tab rather than permanently making the popup two columns wide.
 */

import Clutter from 'gi://Clutter';
import GLib from 'gi://GLib';
import GObject from 'gi://GObject';
import St from 'gi://St';

import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';

import { NotchPanel } from './notchPanel.js';
import { SystemStatsIndicator } from './systemStats.js';

/** How often the clock label is redrawn. Minute precision does not need a
 * faster tick, and a faster one would only cost battery for no visible change. */
const CLOCK_TICK_SECONDS = 15;

export const NotchButton = GObject.registerClass(
class NotchButton extends PanelMenu.Button {
    _init(clipboardWatcher, cancellable, settings) {
        super._init(0.5, 'Veronica', false);
        this._clipboardWatcher = clipboardWatcher;
        this._cancellable = cancellable;
        this._clockTimeoutId = 0;

        this._clockLabel = new St.Label({
            style_class: 'veronica-notch-clock',
            y_align: Clutter.ActorAlign.CENTER,
        });
        this.add_child(this._clockLabel);

        // Edith's menu bar CPU/memory readout, in the position Ubuntu has for
        // it. Governed by menuBarSystemStats; off means no timer at all.
        this._stats = new SystemStatsIndicator(settings);
        this.add_child(this._stats.actor);
        this._stats.enable();

        this._tick();
        this._clockTimeoutId = GLib.timeout_add_seconds(
            GLib.PRIORITY_DEFAULT_IDLE,
            CLOCK_TICK_SECONDS,
            () => {
                this._tick();
                return GLib.SOURCE_CONTINUE;
            }
        );

        this._buildMenu();
        this.menu.actor.add_style_class_name('veronica-notch-shell-menu');
        this._installNotchAnimation();

        this.menu.connect('open-state-changed', (_menu, isOpen) => {
            if (isOpen)
                this.refresh().catch(() => {});
            else
                this._notchPanel?.onMenuClosed();
        });
        // Restore persisted power actions at login without waiting for the
        // user to open the notch for the first time.
        this.refresh().catch(() => {});
    }

    _tick() {
        const now = GLib.DateTime.new_now_local();
        this._clockLabel.text = now.format('%b %-d  %H:%M') ?? '';
    }

    _buildMenu() {
        this._notchPanel = new NotchPanel(
            this._clipboardWatcher,
            this._cancellable,
            () => this.menu.close(),
            theme => this._applyTheme(theme)
        );
        const item = new PopupMenu.PopupBaseMenuItem({ reactive: false, can_focus: false });
        item.add_style_class_name('veronica-notch-root-item');
        item.add_child(this._notchPanel.actor);
        this.menu.addMenuItem(item);
    }

    _applyTheme(theme) {
        const actor = this.menu?.actor;
        if (!actor)
            return;
        for (const name of ['light', 'dark', 'midnight', 'aubergine', 'forest'])
            actor.remove_style_class_name(`veronica-theme-${name}`);
        actor.add_style_class_name(`veronica-theme-${theme}`);
    }

    /**
     * A two-layer Dynamic-Island morph.
     *
     * Animating text while the whole popup is heavily scaled caused the
     * one-frame shimmer on open. The black surface now expands at full opacity
     * while its contents fade in just behind it, matching macOS's separation
     * between shape morph and content reveal.
     */
    _installNotchAnimation() {
        const box = this.menu._boxPointer;
        if (!box)
            return;

        box.open = (animate, onComplete) => {
            box.remove_all_transitions();
            box.bin.remove_all_transitions();
            box.set_pivot_point(0.5, 0);
            box.opacity = 255;
            box.translation_y = animate ? -4 : 0;
            box.scale_x = animate ? 0.36 : 1;
            box.scale_y = animate ? 0.12 : 1;
            box.bin.opacity = animate ? 0 : 255;
            box._muteKeys = false;
            box._muteInput = true;
            box.show();
            box.ease({
                translation_y: 0,
                scale_x: 1,
                scale_y: 1,
                duration: animate ? 300 : 0,
                mode: Clutter.AnimationMode.EASE_OUT_EXPO,
                onComplete: () => {
                    box._muteInput = false;
                    onComplete?.();
                },
            });
            box.bin.ease({
                opacity: 255,
                delay: animate ? 72 : 0,
                duration: animate ? 170 : 0,
                mode: Clutter.AnimationMode.EASE_OUT_QUAD,
            });
        };

        box.close = (animate, onComplete) => {
            if (!box.visible)
                return;
            box._muteInput = true;
            box._muteKeys = true;
            box.remove_all_transitions();
            box.bin.remove_all_transitions();
            box.set_pivot_point(0.5, 0);
            box.bin.ease({
                opacity: animate ? 0 : 255,
                duration: animate ? 85 : 0,
                mode: Clutter.AnimationMode.EASE_OUT_QUAD,
            });
            box.ease({
                opacity: 255,
                translation_y: animate ? -4 : 0,
                scale_x: animate ? 0.36 : 1,
                scale_y: animate ? 0.12 : 1,
                duration: animate ? 210 : 0,
                mode: Clutter.AnimationMode.EASE_IN_EXPO,
                onComplete: () => {
                    box.hide();
                    box.opacity = 255;
                    box.bin.opacity = 255;
                    box.translation_y = 0;
                    box.scale_x = 1;
                    box.scale_y = 1;
                    onComplete?.();
                },
            });
        };
    }

    async refresh() {
        await this._notchPanel?.refresh();
    }

    cleanKeys() {
        this._notchPanel?.startCleanKeys();
    }

    pickColor() {
        this._notchPanel?.pickColor();
    }

    destroy() {
        if (this._clockTimeoutId) {
            GLib.Source.remove(this._clockTimeoutId);
            this._clockTimeoutId = 0;
        }
        this._stats?.disable();
        this._stats = null;
        this._notchPanel?.destroy();
        this._notchPanel = null;
        super.destroy();
    }
});
