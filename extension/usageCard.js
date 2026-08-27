/* The usage card: two rings plus a spend line, matching Edith's own notch —
 * a five-hour and a seven-day ring side by side, each showing a live
 * percentage and counting down to its reset.
 */

import Clutter from 'gi://Clutter';
import St from 'gi://St';

import { fetchUsageData } from './sections.js';
import { band, countdown, money, runJson } from './lib.js';
import { RingGauge } from './ring.js';

/** Ring gauges shown, in order. Matches the two windows Edith's own notch
 * shows (session, week) rather than every scoped window an account might have. */
const RING_WINDOWS = ['Session', 'Week'];
const PROVIDERS = ['Claude', 'Codex'];

function providerName(value) {
    return `${value}`.toLowerCase() === 'codex' ? 'Codex' : 'Claude';
}

export class UsageCard {
    constructor() {
        this.actor = new St.BoxLayout({
            style_class: 'veronica-card veronica-usage-card',
            orientation: Clutter.Orientation.VERTICAL,
            visible: false,
        });

        const rings = new St.BoxLayout({
            style_class: 'veronica-rings',
            y_expand: true,
            y_align: Clutter.ActorAlign.CENTER,
        });
        this._ringsBox = rings;
        this._private = false;
        this._provider = 'Claude';
        this._lastData = null;
        this._lastSpendText = '';

        const toolbar = new St.BoxLayout({ style_class: 'veronica-usage-toolbar' });
        this._providerButton = new St.Button({
            style_class: 'veronica-provider-switch',
            can_focus: true,
            accessible_name: 'Switch rate limit provider',
        });
        this._providerLabel = new St.Label({
            text: 'Claude  ▾',
            style_class: 'veronica-provider-label',
        });
        this._providerButton.set_child(this._providerLabel);
        this._providerButton.connect('clicked', () => this._cycleProvider());
        toolbar.add_child(this._providerButton);
        toolbar.add_child(new St.Widget({ x_expand: true }));
        this.actor.add_child(toolbar);

        this._rings = RING_WINDOWS.map(windowName => {
            const wrap = new St.BoxLayout({
                orientation: Clutter.Orientation.VERTICAL,
                style_class: 'veronica-ring-wrap',
            });
            const gauge = new RingGauge(60);
            const caption = new St.Label({
                style_class: 'veronica-ring-caption',
                x_align: Clutter.ActorAlign.CENTER,
            });
            const resets = new St.Label({
                style_class: 'veronica-ring-resets',
                x_align: Clutter.ActorAlign.CENTER,
            });
            wrap.add_child(gauge);
            wrap.add_child(caption);
            wrap.add_child(resets);
            rings.add_child(wrap);
            return { windowName, gauge, caption, resets, wrap };
        });
        this.actor.add_child(rings);

        this._spendLine = new St.Label({ style_class: 'veronica-usage-spend' });
        this.actor.add_child(this._spendLine);
    }

    async refresh(cancellable) {
        const data = await fetchUsageData(cancellable);
        if (!this.actor)
            return; // destroyed while the subprocess ran

        if (!data.summary && data.gauges.length === 0) {
            this.actor.visible = false;
            return;
        }
        this.actor.visible = true;

        this._lastData = data;
        this._render();
    }

    setProvider(value) {
        this._provider = providerName(value);
        if (this._providerLabel)
            this._providerLabel.text = `${this._provider}  ▾`;
        this._render();
    }

    _cycleProvider() {
        const index = PROVIDERS.indexOf(this._provider);
        const next = PROVIDERS[(index + 1) % PROVIDERS.length];
        this.setProvider(next);
        runJson(['config', 'set', 'limitsProvider', next.toLowerCase()]).catch(() => {});
    }

    _render() {
        if (!this.actor || !this._lastData)
            return;
        const data = this._lastData;
        const providerGauges = data.gauges.filter(gauge => gauge.provider === this._provider);

        for (const ring of this._rings) {
            const gauge = providerGauges.find(g => g.window === ring.windowName);
            ring.wrap.visible = !!gauge;
            if (!gauge)
                continue;
            ring.gauge.setValue(gauge.percent ?? 0, band(gauge.percent ?? 0));
            ring.caption.text = gauge.window;
            ring.resets.text = Number.isFinite(gauge.resetsInSecs)
                ? countdown(gauge.resetsInSecs)
                : '';
        }

        const totals = data.summary?.totals ?? {};
        const sessions = data.summary?.sessions ?? 0;
        this._lastSpendText = providerGauges.length > 0
            ? `${money(totals.cost ?? 0)} · ${sessions} ${sessions === 1 ? 'session' : 'sessions'} this week`
            : `${this._provider} limits unavailable`;
        this._spendLine.text = this._private ? 'Private while presenting' : this._lastSpendText;
        this._ringsBox.visible = !this._private;
    }

    setPrivate(privateMode) {
        this._private = privateMode;
        if (!this.actor)
            return;
        this._ringsBox.visible = !privateMode;
        this._spendLine.text = privateMode ? 'Private while presenting' : this._lastSpendText;
        this.actor.set_style_class_name(
            privateMode
                ? 'veronica-card veronica-usage-card presenter-private'
                : 'veronica-card veronica-usage-card'
        );
    }

    destroy() {
        this.actor?.destroy();
        this.actor = null;
        this._ringsBox = null;
        this._providerButton = null;
        this._providerLabel = null;
        this._lastData = null;
    }
}
