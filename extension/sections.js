/* Shared section widgets and data: usage, clipboard, and machine state.
 *
 * Both consumers use these: the default mode, which adds rows into the real
 * clock dropdown's own column, and the full-replacement notch, which renders
 * the same data as cards and rings. The fetch is shared so the two can never
 * disagree about a figure; only the presentation differs per surface.
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
        if (title)
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

/**
 * Fetch usage summary, rate-limit gauges and source breakdown in one shot.
 * Returns null fields rather than throwing when `vr` has nothing to say.
 */
export async function fetchUsageData(cancellable) {
    const summary = await runJson(['usage', 'summary', '--days', '7'], cancellable);
    const limits = await runJson(['usage', 'limits'], cancellable);
    const sources = await runJson(['usage', 'sources', '--days', '7'], cancellable);
    return {
        summary,
        gauges: Array.isArray(limits?.gauges) ? limits.gauges : [],
        sources: Array.isArray(sources) ? sources : [],
    };
}

export async function refreshUsageSection(section, cancellable) {
    if (!section?.isLive)
        return;

    const data = await fetchUsageData(cancellable);
    if (!section.isLive)
        return; // torn down while the subprocess ran

    section.clear();
    if (!data.summary) {
        section.add(emptyLabel('No usage collected yet'));
        return;
    }

    // Rate limits first: a window about to run out matters more than a
    // month's spend.
    for (const gauge of data.gauges.slice(0, 4)) {
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

    const totals = data.summary.totals ?? {};
    section.add(textRow('Spend', money(totals.cost ?? 0)));
    section.add(textRow('Tokens', tokens(totals.tokens ?? 0)));
    section.add(textRow('Sessions', `${data.summary.sessions ?? 0}`, true));

    if (data.sources.length > 0) {
        const highest = Math.max(...data.sources.map(s => s.cost ?? 0), 0);
        for (const source of data.sources.slice(0, 4)) {
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
            style_class: 'veronica-clip-row',
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

/**
 * One line summarising machine state, for the compact notch layout.
 *
 * `getLabel` is called after the subprocess returns rather than the label
 * being passed directly, so a caller whose label was destroyed in the
 * meantime can simply return null instead of needing its own liveness flag.
 */
export async function refreshMachineLine(getLabel, cancellable) {
    const diagnose = await runJson(['diagnose'], cancellable);
    const label = getLabel();
    if (!label)
        return;
    if (!diagnose) {
        label.text = 'Veronica is not installed';
        return;
    }
    const states = diagnose.capabilities?.states ?? {};
    const available = Object.values(states).filter(s => s.state === 'available').length;
    const total = Object.keys(states).length;
    const extensions = Array.isArray(diagnose.extensions) ? diagnose.extensions : [];
    const enabled = extensions.filter(e => e.enabled).length;
    const session = diagnose.session ?? {};
    label.text =
        `${session.kind ?? 'unknown'} · ${session.desktop ?? ''} · `
        + `${available}/${total} capabilities · ${enabled}/${extensions.length} extensions`;
}
