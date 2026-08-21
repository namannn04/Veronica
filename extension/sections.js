/* Shared section widgets: usage, clipboard, and machine state.
 *
 * Both consumers use these: the default mode, which adds them into the real
 * clock dropdown's own column, and the full-replacement mode, which adds them
 * into Veronica's own notch popup. The behaviour must be identical either way,
 * so this is the one place it is written.
 */

import St from 'gi://St';
import Clutter from 'gi://Clutter';

import { entryText, recentEntries } from './clipboard.js';
import {
    band,
    countdown,
    emptyLabel,
    heading,
    meterRow,
    money,
    runJson,
    textRow,
    tokens,
} from './lib.js';

/** A titled group of rows that rebuilds itself from a `vr` document. */
export class Section {
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
        this._rows?.destroy_all_children();
    }

    add(child) {
        this._rows?.add_child(child);
    }

    /** Whether this section is still live, for a refresh to check after an await. */
    get isLive() {
        return this.actor !== null;
    }

    destroy() {
        this.actor?.destroy();
        this.actor = null;
        this._rows = null;
    }
}

export async function refreshUsageSection(section, cancellable) {
    if (!section?.isLive)
        return;

    const summary = await runJson(['usage', 'summary', '--days', '7'], cancellable);
    if (!section.isLive)
        return; // torn down while the subprocess ran

    section.clear();
    if (!summary) {
        section.add(emptyLabel('No usage collected yet'));
        return;
    }

    // Rate limits first: a window about to run out matters more than a
    // month's spend.
    const limits = await runJson(['usage', 'limits'], cancellable);
    if (!section.isLive)
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

    const sources = await runJson(['usage', 'sources', '--days', '7'], cancellable);
    if (!section.isLive)
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

/**
 * `closeMenu` is called after a row is clicked, so picking an entry also
 * dismisses whichever popup is currently showing it — the real dropdown's or
 * the notch's own.
 */
export async function refreshClipboardSection(section, cancellable, clipboardWatcher, closeMenu) {
    if (!section?.isLive)
        return;

    const rows = await recentEntries(5, cancellable);
    if (!section.isLive)
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
            entryText(row.id, cancellable)
                .then(text => {
                    if (text)
                        clipboardWatcher?.write(text);
                })
                .catch(() => {});
            closeMenu?.();
        });
        section.add(button);
    }
}

export async function refreshMachineSection(section, cancellable) {
    if (!section?.isLive)
        return;

    const diagnose = await runJson(['diagnose'], cancellable);
    if (!section.isLive)
        return;

    section.clear();
    if (!diagnose) {
        section.add(emptyLabel('Veronica is not installed'));
        return;
    }

    const sessionInfo = diagnose.session ?? {};
    const states = diagnose.capabilities?.states ?? {};
    const available = Object.values(states)
        .filter(state => state.state === 'available').length;
    const total = Object.keys(states).length;

    section.add(textRow('Session', `${sessionInfo.kind ?? 'unknown'} · ${sessionInfo.desktop ?? ''}`.trim(), true));
    if (total > 0)
        section.add(textRow('Capabilities', `${available} of ${total} available`, true));

    const extensions = Array.isArray(diagnose.extensions) ? diagnose.extensions : [];
    const enabled = extensions.filter(e => e.enabled).length;
    if (extensions.length > 0)
        section.add(textRow('Extensions', `${enabled} of ${extensions.length} on`, true));
}
