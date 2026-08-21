/* The notch: Veronica's own replacement for the clock and its dropdown.
 *
 * Only used when the user has opted into full top-bar replacement. It reuses
 * GNOME's own Calendar, DBusEventSource and CalendarMessageList classes —
 * the exact widgets the stock dropdown is built from — so the calendar and
 * notifications are the real thing, not a reimplementation. The card layout
 * (now-playing beside usage rings, compact action tiles) follows the same
 * visual language as the macOS app this is a port of, adapted to what a
 * GNOME popup menu can hold.
 */

import Clutter from 'gi://Clutter';
import GLib from 'gi://GLib';
import GObject from 'gi://GObject';
import St from 'gi://St';

import * as Calendar from 'resource:///org/gnome/shell/ui/calendar.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';

import { NowPlayingCard } from './nowPlaying.js';
import {
    refreshClipboardSection,
    refreshMachineLine,
} from './sections.js';
import { UsageCard } from './usageCard.js';
import { emptyLabel, launchApp, runJson } from './lib.js';

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
        // exactly what the user already knows. Width-constrained so an empty
        // "No Notifications" placeholder does not dominate the popup.
        //
        // CalendarMessageList's own _init() sets style_class itself, ignoring
        // whatever the constructor is given, so the class has to be added
        // after construction rather than passed in — confirmed live, the
        // constructor-supplied class was silently dropped and the list came
        // out at its unconstrained default width (432px).
        this._messageList = new Calendar.CalendarMessageList();
        this._messageList.add_style_class_name('veronica-notch-messages');
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

        // Now-playing and usage rings side by side, mirroring the app's own
        // home view: the two things worth a glance sit together, everything
        // else is a tap away.
        const glanceRow = new St.BoxLayout({ style_class: 'veronica-glance-row' });
        this._nowPlaying = new NowPlayingCard();
        this._usage = new UsageCard();
        glanceRow.add_child(this._nowPlaying.actor);
        glanceRow.add_child(this._usage.actor);
        column.add_child(glanceRow);

        this._machineLine = new St.Label({ style_class: 'veronica-machine-line' });
        column.add_child(this._machineLine);

        this._clipboardCard = new St.BoxLayout({
            style_class: 'veronica-card veronica-clip-card',
            orientation: Clutter.Orientation.VERTICAL,
        });
        this._clipboardRows = new St.BoxLayout({ orientation: Clutter.Orientation.VERTICAL });
        this._clipboardCard.add_child(this._clipboardRows);
        // refreshClipboardSection expects a Section-shaped object; this
        // adapter gives it one without pulling in the heading/wrapper the
        // dropdown's plainer rows use.
        this._clipboardSection = {
            get isLive() { return this.actor !== null; },
            actor: this._clipboardCard,
            clear: () => this._clipboardRows.destroy_all_children(),
            add: child => this._clipboardRows.add_child(child),
            destroy: () => {},
        };
        column.add_child(this._clipboardCard);

        column.add_child(this._actionsRow());

        layout.add_child(column);

        const item = new PopupMenu.PopupBaseMenuItem({ reactive: false, can_focus: false });
        item.add_child(layout);
        this.menu.addMenuItem(item);
    }

    _actionsRow() {
        const row = new St.BoxLayout({ style_class: 'veronica-actions-row' });
        row.add_child(this._actionTile('view-app-grid-symbolic', 'Open Veronica', () => {
            this.menu.close();
            launchApp();
        }));
        row.add_child(this._actionTile('view-refresh-symbolic', 'Refresh usage', () => {
            runJson(['usage', 'refresh']).catch(() => {});
        }));
        return row;
    }

    _actionTile(iconName, label, onClicked) {
        const button = new St.Button({ style_class: 'veronica-action-tile', can_focus: true, x_expand: true });
        const content = new St.BoxLayout({ style_class: 'veronica-action-content' });
        content.add_child(new St.Icon({ icon_name: iconName, style_class: 'veronica-action-icon' }));
        content.add_child(new St.Label({ text: label, style_class: 'veronica-action-label', y_align: Clutter.ActorAlign.CENTER }));
        button.set_child(content);
        button.connect('clicked', onClicked);
        return button;
    }

    async refresh() {
        this._clipboardSection.clear();
        this._clipboardSection.add(emptyLabel('Reading…'));
        await Promise.all([
            this._usage.refresh(this._cancellable),
            refreshClipboardSection(
                this._clipboardSection,
                this._cancellable,
                this._clipboardWatcher,
                () => this.menu.close()
            ),
            refreshMachineLine(() => this._machineLine, this._cancellable),
            this._nowPlaying.refresh(this._cancellable),
        ]);
    }

    destroy() {
        if (this._clockTimeoutId) {
            GLib.Source.remove(this._clockTimeoutId);
            this._clockTimeoutId = 0;
        }
        this._eventSource?.destroy();
        this._usage?.destroy();
        this._nowPlaying?.destroy();
        // Nulled rather than left dangling, so refreshMachineLine's getter
        // correctly reports "gone" instead of writing to a destroyed actor.
        this._machineLine = null;
        super.destroy();
    }
});
