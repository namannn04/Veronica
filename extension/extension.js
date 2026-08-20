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

import { ClipboardWatcher, entryText, recentEntries } from './clipboard.js';
import { PanelReplacement } from './panelReplacement.js';
import {
    band,
    countdown,
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

/**
 * How often to ask the provider for rate limits.
 *
 * Far slower than the spend readout: each call is a network request, and the
 * windows move on the scale of hours, so polling tightly would be rude to the
 * provider and pointless for the user.
 */
const LIMITS_INTERVAL_SECONDS = 300;

/**
 * How often to check whether the top-bar replacement setting changed.
 *
 * The setting lives in Veronica's own config file rather than a GSettings
 * schema, so there is no change signal to listen for; polling this slowly is
 * the plain alternative, and the check itself is a fast local read.
 */
const REPLACEMENT_CHECK_INTERVAL_SECONDS = 10;

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
        this._limitLabel = new St.Label({
            text: '',
            style_class: 'veronica-indicator-label',
            y_align: Clutter.ActorAlign.CENTER,
            visible: false,
        });
        this._box.add_child(this._icon);
        this._box.add_child(this._limitLabel);
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

    /** The most pressing rate limit, or null to hide it. */
    setLimit(limit) {
        if (!limit) {
            this._limitLabel.visible = false;
            return;
        }
        this._limitLabel.text = limit.text;
        // The band is carried by a style class rather than an inline colour, so
        // the theme can restyle it.
        for (const name of ['good', 'warning', 'critical'])
            this._limitLabel.remove_style_class_name(`veronica-band-${name}`);
        this._limitLabel.add_style_class_name(`veronica-band-${limit.band}`);
        this._limitLabel.visible = true;
        this.set_accessible_name(limit.tooltip);
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
        this._limitsTimeoutId = 0;
        this._menuSignalId = 0;
        this._usageSection = null;
        this._machineSection = null;
        this._clipboardSection = null;
        this._injectedInto = null;
        this._fallbackItem = null;

        this._clipboard = new ClipboardWatcher();
        if (this._clipboard.enable())
            console.debug('veronica: watching the clipboard');

        // Off by default: replacing the stock network/bluetooth/volume/battery
        // cluster is the highest-risk part of the top bar integration, so it
        // only activates once the user has explicitly opted in.
        this._panelReplacement = new PanelReplacement();
        this._applyReplacementSetting().catch(() => {});
        this._replacementTimeoutId = GLib.timeout_add_seconds(
            GLib.PRIORITY_DEFAULT_IDLE,
            REPLACEMENT_CHECK_INTERVAL_SECONDS,
            () => {
                this._applyReplacementSetting().catch(() => {});
                return GLib.SOURCE_CONTINUE;
            }
        );

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
        this._refreshLimitIndicator().catch(() => {});
        this._limitsTimeoutId = GLib.timeout_add_seconds(
            GLib.PRIORITY_DEFAULT_IDLE,
            LIMITS_INTERVAL_SECONDS,
            () => {
                this._refreshLimitIndicator().catch(() => {});
                return GLib.SOURCE_CONTINUE;
            }
        );
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
        for (const name of ['_timeoutId', '_limitsTimeoutId']) {
            if (this[name]) {
                GLib.Source.remove(this[name]);
                this[name] = 0;
            }
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
        if (this._clipboardSection) {
            this._clipboardSection.destroy();
            this._clipboardSection = null;
        }
        if (this._clipboard) {
            this._clipboard.disable();
            this._clipboard = null;
        }
        if (this._replacementTimeoutId) {
            GLib.Source.remove(this._replacementTimeoutId);
            this._replacementTimeoutId = 0;
        }
        if (this._panelReplacement) {
            // Always disabled on the way out, regardless of the setting, so
            // the stock icons are guaranteed to come back.
            this._panelReplacement.disable();
            this._panelReplacement = null;
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
        this._clipboardSection = new Section('Clipboard');
        this._clipboardSection.add(emptyLabel('Nothing copied yet'));

        const column = findCalendarColumn(dateMenu);
        if (column) {
            // Below the calendar and the events list, where the shell puts its
            // own optional sections such as clocks and weather.
            column.add_child(this._usageSection.actor);
            column.add_child(this._clipboardSection.actor);
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
        wrapper.add_child(this._clipboardSection.actor);
        wrapper.add_child(this._machineSection.actor);
        this._fallbackItem.add_child(wrapper);
        dateMenu.menu.addMenuItem(this._fallbackItem);
    }

    /** Match the live top-bar replacement state to the stored setting. */
    async _applyReplacementSetting() {
        if (!this._panelReplacement)
            return;
        const settings = await runJson(['config', 'get', 'topBarReplacement'], this._cancellable);
        const wanted = settings === true;
        if (wanted && !this._panelReplacement.isActive) {
            this._panelReplacement.enable();
            console.log('veronica: replaced the network/bluetooth/volume/battery cluster');
        } else if (!wanted && this._panelReplacement.isActive) {
            this._panelReplacement.disable();
            console.log('veronica: restored the stock status area');
        }
    }

    /** Fill both sections from `vr`. */
    async _refreshSections() {
        await Promise.all([
            this._refreshUsage(),
            this._refreshClipboard(),
            this._refreshMachine(),
        ]);
    }

    async _refreshClipboard() {
        const section = this._clipboardSection;
        if (!section)
            return;

        const rows = await recentEntries(5, this._cancellable);
        if (!this._clipboardSection || this._clipboardSection !== section)
            return;

        section.clear();
        if (rows.length === 0) {
            section.add(emptyLabel('Nothing copied yet'));
            return;
        }

        for (const row of rows) {
            // The list carries previews only; the full text is fetched on click,
            // so a large copy is never held in the panel.
            const button = new St.Button({
                style_class: 'veronica-clip',
                x_expand: true,
                can_focus: true,
                label: row.preview,
            });
            button.connect('clicked', () => {
                entryText(row.id, this._cancellable)
                    .then(text => {
                        if (text)
                            this._clipboard?.write(text);
                    })
                    .catch(() => {});
                Main.panel.statusArea.dateMenu?.menu?.close();
            });
            section.add(button);
        }
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

        // Rate limits first: a window about to run out matters more than a
        // month's spend.
        const limits = await runJson(['usage', 'limits'], this._cancellable);
        if (!this._usageSection || this._usageSection !== section)
            return;
        for (const gauge of (limits?.gauges ?? []).slice(0, 4)) {
            const resets = Number.isFinite(gauge.resetsInSecs)
                ? ` · ${countdown(gauge.resetsInSecs)}`
                : '';
            section.add(meterRow(
                `${gauge.provider} ${gauge.window}`,
                (gauge.percent ?? 0) / 100,
                `${Math.round(gauge.percent ?? 0)}%${resets}`,
                band(gauge.percent ?? 0)
            ));
        }

        const totals = summary.totals ?? {};
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

    /**
     * Show the window closest to running out, tinted by its band.
     *
     * Highest risk rather than highest percentage: 60% with minutes left is more
     * urgent than 80% with a week to go, and risk already accounts for that.
     */
    async _refreshLimitIndicator() {
        if (!this._indicator)
            return;
        const report = await runJson(['usage', 'limits'], this._cancellable);
        if (!this._indicator)
            return;

        const gauges = report?.gauges ?? [];
        if (gauges.length === 0) {
            this._indicator.setLimit(null);
            return;
        }
        const pressing = gauges.reduce(
            (worst, gauge) => ((gauge.risk ?? 0) > (worst.risk ?? 0) ? gauge : worst),
            gauges[0]
        );
        this._indicator.setLimit({
            text: `${Math.round(pressing.percent ?? 0)}%`,
            band: band(pressing.percent ?? 0),
            tooltip: `${pressing.provider} ${pressing.window}: ${Math.round(pressing.percent ?? 0)}%`
                + (Number.isFinite(pressing.resetsInSecs)
                    ? `, resets in ${countdown(pressing.resetsInSecs)}`
                    : ''),
        });
    }
}
