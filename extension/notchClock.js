/* The notch: Veronica's own replacement for the clock and its dropdown.
 *
 * Only used when the user has opted into full top-bar replacement. It reuses
 * GNOME's own Calendar, DBusEventSource and CalendarMessageList classes —
 * the exact widgets the stock dropdown is built from — so the calendar and
 * notifications are the real thing, not a reimplementation. What Veronica
 * adds beside them is what the shell has no notion of: agent usage and rate
 * limits, now-playing, clipboard history, and machine state.
 */

import Clutter from 'gi://Clutter';
import GLib from 'gi://GLib';
import GObject from 'gi://GObject';
import St from 'gi://St';

import * as Calendar from 'resource:///org/gnome/shell/ui/calendar.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';

import { NowPlayingCard } from './nowPlaying.js';
import { Section, refreshClipboardSection, refreshMachineSection, refreshUsageSection } from './sections.js';
import { emptyLabel } from './lib.js';

/** How often the clock label is redrawn. Minute precision does not need a
 * faster tick, and a faster one would only cost battery for no visible change. */
const CLOCK_TICK_SECONDS = 15;

export const NotchButton = GObject.registerClass(
class NotchButton extends PanelMenu.Button {
    _init(clipboardWatcher, cancellable) {
        super._init(0.5, 'Veronica', false);
        this._clipboardWatcher = clipboardWatcher;
        this._cancellable = cancellable;
        this._clockTimeoutId = 0;

        this._clockLabel = new St.Label({
            style_class: 'veronica-notch-clock',
            y_align: Clutter.ActorAlign.CENTER,
        });
        this.add_child(this._clockLabel);
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

        this.menu.connect('open-state-changed', (_menu, isOpen) => {
            if (isOpen)
                this.refresh().catch(() => {});
        });
    }

    _tick() {
        const now = GLib.DateTime.new_now_local();
        this._clockLabel.text = now.format('%b %-d  %H:%M') ?? '';
    }

    _buildMenu() {
        const layout = new St.BoxLayout({ style_class: 'veronica-notch-menu' });

        // The real notification list — the same widget the stock dropdown
        // shows, so notification behaviour (grouping, dismissal, actions) is
        // exactly what the user already knows.
        this._messageList = new Calendar.CalendarMessageList();
        layout.add_child(this._messageList);

        const column = new St.BoxLayout({
            orientation: Clutter.Orientation.VERTICAL,
            style_class: 'veronica-notch-column',
        });

        // The real calendar, backed by the same event source the stock
        // dropdown uses, so recurring events and every configured calendar
        // show up exactly as they do there.
        this._eventSource = new Calendar.DBusEventSource();
        this._calendar = new Calendar.Calendar();
        this._calendar.setEventSource(this._eventSource);
        column.add_child(this._calendar);

        this._nowPlaying = new NowPlayingCard();
        column.add_child(this._nowPlaying.actor);

        this._usageSection = new Section('Agent usage · last 7 days');
        this._usageSection.add(emptyLabel('Reading…'));
        this._clipboardSection = new Section('Clipboard');
        this._clipboardSection.add(emptyLabel('Nothing copied yet'));
        this._machineSection = new Section('This computer');
        this._machineSection.add(emptyLabel('Reading…'));
        column.add_child(this._usageSection.actor);
        column.add_child(this._clipboardSection.actor);
        column.add_child(this._machineSection.actor);

        layout.add_child(column);

        const item = new PopupMenu.PopupBaseMenuItem({ reactive: false, can_focus: false });
        item.add_child(layout);
        this.menu.addMenuItem(item);
    }

    async refresh() {
        await Promise.all([
            refreshUsageSection(this._usageSection, this._cancellable),
            refreshClipboardSection(
                this._clipboardSection,
                this._cancellable,
                this._clipboardWatcher,
                () => this.menu.close()
            ),
            refreshMachineSection(this._machineSection, this._cancellable),
            this._nowPlaying.refresh(this._cancellable),
        ]);
    }

    destroy() {
        if (this._clockTimeoutId) {
            GLib.Source.remove(this._clockTimeoutId);
            this._clockTimeoutId = 0;
        }
        this._eventSource?.destroy();
        this._usageSection?.destroy();
        this._clipboardSection?.destroy();
        this._machineSection?.destroy();
        this._nowPlaying?.destroy();
        super.destroy();
    }
});
