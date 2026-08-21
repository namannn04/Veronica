/* The usage card: two rings plus a spend line, matching Edith's own notch —
 * a five-hour and a seven-day ring side by side, each showing a live
 * percentage and counting down to its reset.
 */

import Clutter from 'gi://Clutter';
import St from 'gi://St';

import { fetchUsageData } from './sections.js';
import { band, countdown, money } from './lib.js';
import { RingGauge } from './ring.js';

/** Ring gauges shown, in order. Matches the two windows Edith's own notch
 * shows (session, week) rather than every scoped window an account might have. */
const RING_WINDOWS = ['Session', 'Week'];

export class UsageCard {
    constructor() {
        this.actor = new St.BoxLayout({
            style_class: 'veronica-card veronica-usage-card',
            orientation: Clutter.Orientation.VERTICAL,
            visible: false,
        });

        const rings = new St.BoxLayout({ style_class: 'veronica-rings' });
        this._rings = RING_WINDOWS.map(windowName => {
            const wrap = new St.BoxLayout({
                orientation: Clutter.Orientation.VERTICAL,
                style_class: 'veronica-ring-wrap',
            });
            const gauge = new RingGauge(52);
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

        for (const ring of this._rings) {
            const gauge = data.gauges.find(g => g.window === ring.windowName);
            ring.wrap.visible = !!gauge;
            if (!gauge)
                continue;
            ring.gauge.setValue(gauge.percent ?? 0, band(gauge.percent ?? 0));
            ring.caption.text = `${gauge.provider} ${gauge.window}`;
            ring.resets.text = Number.isFinite(gauge.resetsInSecs)
                ? countdown(gauge.resetsInSecs)
                : '';
        }

        const totals = data.summary?.totals ?? {};
        const sessions = data.summary?.sessions ?? 0;
        this._spendLine.text =
            `${money(totals.cost ?? 0)} · ${sessions} ${sessions === 1 ? 'session' : 'sessions'} this week`;
    }

    destroy() {
        this.actor?.destroy();
        this.actor = null;
    }
}
