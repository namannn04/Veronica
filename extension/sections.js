import St from 'gi://St';

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
    vbox,
} from './lib.js';

export class Section {
    constructor(title) {
        this.actor = vbox({
            style_class: 'veronica-section',
            x_expand: true,
        });
        this._rows = vbox({
            x_expand: true,
        });
        if (title) {
            this.actor.add_child(heading(title));
        }
        this.actor.add_child(this._rows);
    }

    clear() {
        this._rows?.destroy_all_children();
    }

    add(child) {
        this._rows?.add_child(child);
    }

    get isLive() {
        return this.actor !== null;
    }

    destroy() {
        this.actor?.destroy();
        this.actor = null;
        this._rows = null;
    }
}

export async function fetchUsageData(cancellable) {
    const summary = await runJson(['usage', 'summary', '--days', '7'], cancellable, { terminateOnCancel: true });
    const limits = await runJson(['usage', 'limits'], cancellable, { terminateOnCancel: true });
    const sources = await runJson(['usage', 'sources', '--days', '7'], cancellable, { terminateOnCancel: true });
    return {
        summary,
        gauges: Array.isArray(limits?.gauges) ? limits.gauges : [],
        sources: Array.isArray(sources) ? sources : [],
    };
}

export async function refreshUsageSection(section, cancellable) {
    if (!section?.isLive) {
        return;
    }

    const data = await fetchUsageData(cancellable);
    if (!section.isLive) {
        return;
    }

    section.clear();
    if (!data.summary) {
        section.add(emptyLabel('No usage collected yet'));
        return;
    }

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

export async function refreshClipboardSection(section, cancellable, clipboardWatcher, closeMenu) {
    if (!section?.isLive) {
        return;
    }

    const rows = await recentEntries(5, cancellable);
    if (!section.isLive) {
        return;
    }

    section.clear();
    if (rows.length === 0) {
        section.add(emptyLabel('Nothing copied yet'));
        return;
    }

    for (const row of rows) {
        const button = new St.Button({
            style_class: 'veronica-clip-row',
            x_expand: true,
            can_focus: true,
            label: row.preview,
        });
        button.connect('clicked', () => {
            entryText(row.id, cancellable)
                .then(text => {
                    if (text) {
                        clipboardWatcher?.write(text);
                    }
                })
                .catch(() => {});
            closeMenu?.();
        });
        section.add(button);
    }
}

export async function refreshMachineSection(section, cancellable) {
    if (!section?.isLive) {
        return;
    }

    const diagnose = await runJson(['diagnose'], cancellable);
    if (!section.isLive) {
        return;
    }

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
    if (total > 0) {
        section.add(textRow('Capabilities', `${available} of ${total} available`, true));
    }

    const extensions = Array.isArray(diagnose.extensions) ? diagnose.extensions : [];
    const enabled = extensions.filter(e => e.enabled).length;
    if (extensions.length > 0) {
        section.add(textRow('Extensions', `${enabled} of ${extensions.length} on`, true));
    }
}

export async function refreshMachineLine(getLabel, cancellable) {
    const diagnose = await runJson(['diagnose'], cancellable);
    const label = getLabel();
    if (!label) {
        return;
    }
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
