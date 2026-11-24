import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import GnomeDesktop from 'gi://GnomeDesktop';
import GObject from 'gi://GObject';
import St from 'gi://St';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';

import { PopupAnimation } from 'resource:///org/gnome/shell/ui/boxpointer.js';

import { installNotchAnimation } from './notchAnimation.js';
import { attachNotchBanners } from './notchBanners.js';
import { NotchPanel } from './notchPanel.js';
import { THEMES } from './themes.js';
import { SystemStatsIndicator } from './systemStats.js';

export const NotchButton = GObject.registerClass(
class NotchButton extends PanelMenu.Button {
    _init(clipboardWatcher, cancellable, settings, keyboardCleaner) {
        super._init(0.5, 'Veronica', false);
        this._clipboardWatcher = clipboardWatcher;
        this._cancellable = cancellable;
        this._settings = settings;
        this._keyboardCleaner = keyboardCleaner;
        this._wallClockId = 0;
        this._clockFormatId = 0;
        this._openRefreshId = 0;
        this._motionCleanup = null;
        this.add_style_class_name('veronica-notch-button');

        this._clockLabel = new St.Label({
            style_class: 'veronica-notch-clock',
            y_align: Clutter.ActorAlign.CENTER,
        });
        this.add_child(this._clockLabel);

        this._stats = new SystemStatsIndicator(settings);
        this.add_child(this._stats.actor);
        this._stats.enable();

        this._wallClock = new GnomeDesktop.WallClock();
        this._wallClockId = this._wallClock.connect('notify::clock', () => this._tick());
        this._interfaceSettings = new Gio.Settings({ schema_id: 'org.gnome.desktop.interface' });
        this._clockFormatId = this._interfaceSettings.connect(
            'changed::clock-format', () => this._tick());
        this._tick();

        try {
            this._buildMenu();
            this.menu.actor.add_style_class_name('veronica-notch-shell-menu');
            this._installNotchAnimation();
            this._bannerPresentation = attachNotchBanners(Main.messageTray, this, this._notchTheme ?? 'dark');
        } catch (error) {
            this.destroy();
            throw error;
        }

        this.menu.connect('open-state-changed', (_menu, isOpen) => {
            if (isOpen) {
                if (!this._motionCleanup)
                    this._queueOpenRefresh();
            } else {
                if (this._openRefreshId)
                    GLib.Source.remove(this._openRefreshId);
                this._openRefreshId = 0;
                this._notchPanel?.onMenuClosed();
            }
        });
    }

    _tick() {
        const zone = this._wallClock?.timezone ?? GLib.TimeZone.new_local();
        const now = GLib.DateTime.new_now(zone);
        const twelveHour = this._interfaceSettings?.get_string('clock-format') === '12h';
        this._clockLabel.text = now.format(twelveHour ? '%b %-d  %-l:%M %p' : '%b %-d  %H:%M') ?? '';
    }

    _buildMenu() {
        this._notchPanel = new NotchPanel(
            this._clipboardWatcher,
            this._cancellable,
            () => this.menu.close(),
            theme => this._applyTheme(theme),
            this._settings,
            this._keyboardCleaner,
            (tab = 'files') => {
                this._notchPanel?.showTab(tab);
                this.menu.open();
            }
        );
        const item = new PopupMenu.PopupBaseMenuItem({ reactive: false, can_focus: false });
        item.add_style_class_name('veronica-notch-root-item');
        item.add_child(this._notchPanel.actor);
        this.menu.addMenuItem(item);
    }

    _applyTheme(theme) {
        this._notchTheme = theme;
        this._bannerPresentation?.setTheme(theme);
        const actor = this.menu?.actor;
        if (!actor)
            return;
        for (const name of THEMES)
            actor.remove_style_class_name(`veronica-theme-${name}`);
        actor.add_style_class_name(`veronica-theme-${theme}`);
    }

    _installNotchAnimation() {
        this._motionCleanup = installNotchAnimation(this.menu._boxPointer, {
            buttonSize: () => ({ width: this.width, height: this.height }),
            enabled: () => St.Settings.get().enable_animations,
            modes: Clutter.AnimationMode,
            flags: PopupAnimation,
            onOpened: () => this._queueOpenRefresh(),
        });
    }

    _queueOpenRefresh() {
        if (this._openRefreshId)
            GLib.Source.remove(this._openRefreshId);
        this._openRefreshId = GLib.idle_add(GLib.PRIORITY_DEFAULT_IDLE, () => {
            this._openRefreshId = 0;
            if (this.menu?.isOpen)
                this.refresh().catch(() => {});
            return GLib.SOURCE_REMOVE;
        });
    }

    async refresh() {
        await this._notchPanel?.refresh();
    }

    pickColor() {
        return this._notchPanel?.pickColor() ?? false;
    }

    showClipboard() {
        if (!this._notchPanel?.showTab('clipboard'))
            return false;
        this.menu.open();
        return true;
    }

    toggleNotifications() {
        if (this.menu.isOpen) {
            this.menu.close();
            return;
        }
        this._notchPanel?.showTab('notifications');
        this.menu.open();
    }

    closeMenu() {
        this.menu.close();
    }

    destroy() {
        this._bannerPresentation?.destroy();
        this._bannerPresentation = null;
        this._motionCleanup?.();
        this._motionCleanup = null;
        if (this._openRefreshId)
            GLib.Source.remove(this._openRefreshId);
        this._openRefreshId = 0;
        if (this._wallClockId) {
            this._wallClock.disconnect(this._wallClockId);
            this._wallClockId = 0;
        }
        this._wallClock = null;
        if (this._clockFormatId) {
            this._interfaceSettings.disconnect(this._clockFormatId);
            this._clockFormatId = 0;
        }
        this._interfaceSettings = null;
        this._stats?.disable();
        this._stats = null;
        this._notchPanel?.destroy();
        this._notchPanel = null;
        this._settings = null;
        this._keyboardCleaner = null;
        super.destroy();
    }
});
