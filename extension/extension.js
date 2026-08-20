/* Veronica, inside the shell.
 *
 * This does not draw a panel of its own. It adds sections to the top bar's real
 * clock dropdown — the one that already shows notifications, media and the
 * calendar — and one indicator to the real top bar. Everything the shell already
 * does well is left alone; Veronica only contributes what the shell has no idea
 * about: agent usage and spend, machine load, and quick toggles.
 *
 * Data comes from `vr ... --json`, so these readouts can never disagree with
 * the application's.
 */

import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import St from 'gi://St';
import Clutter from 'gi://Clutter';
import GObject from 'gi://GObject';

import { Extension } from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';

import {
    band,
    emptyLabel,
    findCli,
    heading,
    launchApp,
    meterRow,
    money,
    percent,
    runJson,
    textRow,
    tokens,
} from './lib.js';

/** How often the top bar indicator refreshes while the session is idle. */
const INDICATOR_INTERVAL_SECONDS = 30;

/** Style class the shell gives the clock dropdown's right-hand column. */
const CALENDAR_COLUMN_CLASS = 'datemenu-calendar-column';

/**
 * Find an actor by style class, breadth-first.
 *
 * Matching the shell's own CSS class is far more durable than reaching for
 * private fields: the theme depends on these names, so they change rarely and
 * visibly, whereas private field names move between releases without notice.
 */
function findByStyleClass(root, styleClass, depth = 0) {
    if (!root || depth > 8)
        return null;
    const classes = (root.style_class ?? '').split(/\s+/);
    if (classes.includes(styleClass))
        return root;
    const children = root.get_children ? root.get_children() : [];
    for (const child of children) {
        const found = findByStyleClass(child, styleClass, depth + 1);
        if (found)
            return found;
    }
    return null;
}

/**
 * The column of the clock dropdown that holds the calendar.
 *
 * The dropdown is an hbox: the message list on the left, and this column on the
 * right holding the today button, the calendar, and the shell's own optional
 * sections such as world clocks and weather. Veronica's sections go at the
 * bottom of it, which is where those optional sections already sit.
 *
 * Note it is an St.Widget, not an St.BoxLayout, so a search for a vertical box
 * will not find it.
 */
function findCalendarColumn(dateMenu) {
    if (!dateMenu?.menu?.box)
        return null;
    return findByStyleClass(dateMenu.menu.box, CALENDAR_COLUMN_CLASS);
}

/** A section of rows that rebuilds itself from a `vr` document. */
class Section {
    constructor(title) {
        this.actor = new St.BoxLayout({
            style_class: 'veronica-section',
            orientation: Clutter.Orientation.VERTICAL,
            x_expand: true,
        });
        this._rows = new St.BoxLayout({
            orientation: Clutter.Orientation.VERTICAL,
            x_expand: true,
        });
        this.actor.add_child(heading(title));
        this.actor.add_child(this._rows);
    }

    clear() {
        this._rows.destroy_all_children();
    }

    add(child) {
        this._rows.add_child(child);
    }

    destroy() {
        this.actor.destroy();
        this.actor = null;
        this._rows = null;
    }
}

/** The top bar indicator: today's spend, and the app on click. */
const VeronicaIndicator = GObject.registerClass(
class VeronicaIndicator extends PanelMenu.Button {
    _init() {
        super._init(0.5, 'Veronica', false);

        this._box = new St.BoxLayout({ style_class: 'veronica-indicator' });
        this._icon = new St.Icon({
            icon_name: 'utilities-system-monitor-symbolic',
            style_class: 'system-status-icon',
        });
        this._label = new St.Label({
            text: '',
            style_class: 'veronica-indicator-label',
            y_align: Clutter.ActorAlign.CENTER,
            visible: false,
        });
        this._box.add_child(this._icon);
        this._box.add_child(this._label);
        this.add_child(this._box);

        this._openItem = new PopupMenu.PopupMenuItem('Open Veronica');
        this._openItem.connect('activate', () => launchApp());
        this.menu.addMenuItem(this._openItem);

        this._refreshItem = new PopupMenu.PopupMenuItem('Refresh agent usage');
        this._refreshItem.connect('activate', () => {
            // Fire and forget: the collector takes seconds and the menu should
            // not block on it.
            runJson(['usage', 'refresh']).catch(() => {});
        });
        this.menu.addMenuItem(this._refreshItem);
    }

    setSpend(text) {
        if (text) {
            this._label.text = text;
            this._label.visible = true;
        } else {
            this._label.visible = false;
        }
    }
});

export default class VeronicaExtension extends Extension {
    enable() {
        this._cancellable = new Gio.Cancellable();
        this._timeoutId = 0;
        this._menuSignalId = 0;
        this._usageSection = null;
        this._machineSection = null;
        this._injectedInto = null;
        this._fallbackItem = null;

        this._indicator = new VeronicaIndicator();
        // Left of the system menu, so it reads as part of the status cluster
        // rather than competing with the clock.
        Main.panel.addToStatusArea(this.uuid, this._indicator, 0, 'right');

        console.debug(`veronica: enabling, vr at "${findCli() || 'not found'}"`);
        this._injectIntoClockDropdown();

        // Refresh when the dropdown opens, which is the only time the sections
        // are visible, plus a slow tick for the indicator.
        const dateMenu = Main.panel.statusArea.dateMenu;
        if (dateMenu) {
            this._menuSignalId = dateMenu.menu.connect('open-state-changed',
                (_menu, isOpen) => {
                    if (isOpen)
                        this._refreshSections().catch(() => {});
                });
        }

        // Populate once now, so the first open shows figures rather than
        // "Reading…" while the subprocess runs.
        this._refreshSections().catch(() => {});
        this._refreshIndicator().catch(() => {});
        this._timeoutId = GLib.timeout_add_seconds(
            GLib.PRIORITY_DEFAULT_IDLE,
            INDICATOR_INTERVAL_SECONDS,
            () => {
                this._refreshIndicator().catch(() => {});
                return GLib.SOURCE_CONTINUE;
            }
        );
    }

    disable() {
        if (this._timeoutId) {
            GLib.Source.remove(this._timeoutId);
            this._timeoutId = 0;
        }
        if (this._cancellable) {
            this._cancellable.cancel();
            this._cancellable = null;
        }

        const dateMenu = Main.panel.statusArea.dateMenu;
        if (this._menuSignalId && dateMenu) {
            dateMenu.menu.disconnect(this._menuSignalId);
        }
        this._menuSignalId = 0;

        // Remove everything added to the shell's own widgets, or the sections
        // would survive a disable and stack up on the next enable.
        if (this._usageSection) {
            this._usageSection.destroy();
            this._usageSection = null;
        }
        if (this._machineSection) {
            this._machineSection.destroy();
            this._machineSection = null;
        }
        if (this._fallbackItem) {
            this._fallbackItem.destroy();
            this._fallbackItem = null;
        }
        this._injectedInto = null;

        if (this._indicator) {
            this._indicator.destroy();
            this._indicator = null;
        }
    }

    /** Add Veronica's sections to the real clock dropdown. */
    _injectIntoClockDropdown() {
        const dateMenu = Main.panel.statusArea.dateMenu;
        if (!dateMenu)
            return;

        this._usageSection = new Section('Agent usage · last 7 days');
        this._usageSection.add(emptyLabel('Reading…'));
        this._machineSection = new Section('This computer');
        this._machineSection.add(emptyLabel('Reading…'));

        const column = findCalendarColumn(dateMenu);
        if (column) {
            // Below the calendar and the events list, where the shell puts its
            // own optional sections such as clocks and weather.
            column.add_child(this._usageSection.actor);
            column.add_child(this._machineSection.actor);
            this._injectedInto = column;
            console.log('veronica: added sections to the clock dropdown');
            return;
        }

        // The dropdown was not the shape expected. Rather than fail, fall back
        // to a plain menu item so the readouts are still reachable.
        console.warn('veronica: clock dropdown layout not recognised, using a fallback item');
        this._fallbackItem = new PopupMenu.PopupBaseMenuItem({
            reactive: false,
            can_focus: false,
        });
        const wrapper = new St.BoxLayout({
            orientation: Clutter.Orientation.VERTICAL,
            x_expand: true,
        });
        wrapper.add_child(this._usageSection.actor);
        wrapper.add_child(this._machineSection.actor);
        this._fallbackItem.add_child(wrapper);
        dateMenu.menu.addMenuItem(this._fallbackItem);
    }

    /** Fill both sections from `vr`. */
    async _refreshSections() {
        await Promise.all([this._refreshUsage(), this._refreshMachine()]);
    }

    async _refreshUsage() {
        const section = this._usageSection;
        if (!section)
            return;

        const summary = await runJson(['usage', 'summary', '--days', '7'], this._cancellable);
        // The section may have been torn down while the subprocess ran.
        if (!this._usageSection || this._usageSection !== section)
            return;

        section.clear();
        if (!summary) {
            section.add(emptyLabel('No usage collected yet'));
            return;
        }

        const totals = summary.totals ?? {};
        console.debug(`veronica: usage ${money(totals.cost ?? 0)} over ${summary.activeDays ?? 0} days`);
        section.add(textRow('Spend', money(totals.cost ?? 0)));
        section.add(textRow('Tokens', tokens(totals.tokens ?? 0)));
        section.add(textRow('Sessions', `${summary.sessions ?? 0}`, true));

        const sources = await runJson(['usage', 'sources', '--days', '7'], this._cancellable);
        if (!this._usageSection || this._usageSection !== section)
            return;
        if (Array.isArray(sources) && sources.length > 0) {
            const highest = Math.max(...sources.map(s => s.cost ?? 0), 0);
            for (const source of sources.slice(0, 4)) {
                const cost = source.cost ?? 0;
                section.add(meterRow(
                    source.label || source.name,
                    highest > 0 ? cost / highest : 0,
                    money(cost),
                    null
                ));
            }
        }
    }

    async _refreshMachine() {
        const section = this._machineSection;
        if (!section)
            return;

        const diagnose = await runJson(['diagnose'], this._cancellable);
        if (!this._machineSection || this._machineSection !== section)
            return;

        section.clear();
        if (!diagnose) {
            section.add(emptyLabel('Veronica is not installed'));
            return;
        }

        const session = diagnose.session ?? {};
        const states = diagnose.capabilities?.states ?? {};
        const available = Object.values(states)
            .filter(state => state.state === 'available').length;
        const total = Object.keys(states).length;

        section.add(textRow('Session', `${session.kind ?? 'unknown'} · ${session.desktop ?? ''}`.trim(), true));
        if (total > 0)
            section.add(textRow('Capabilities', `${available} of ${total} available`, true));

        const extensions = Array.isArray(diagnose.extensions) ? diagnose.extensions : [];
        const enabled = extensions.filter(e => e.enabled).length;
        if (extensions.length > 0)
            section.add(textRow('Extensions', `${enabled} of ${extensions.length} on`, true));
    }

    async _refreshIndicator() {
        if (!this._indicator)
            return;
        const summary = await runJson(['usage', 'summary', '--days', '1'], this._cancellable);
        if (!this._indicator)
            return;
        const cost = summary?.totals?.cost;
        this._indicator.setSpend(
            Number.isFinite(cost) && cost > 0 ? money(cost) : ''
        );
    }
}
