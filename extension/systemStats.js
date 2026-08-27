/* Live CPU and memory beside the clock.
 *
 * Edith puts a CPU/memory readout in the macOS menu bar. Ubuntu's equivalent
 * position is the top bar, which Veronica's notch already occupies, so the
 * readout goes there rather than into a second tray item — a StatusNotifierItem
 * cannot draw live text on GNOME anyway.
 *
 * The numbers are read straight out of procfs rather than by running `vr`:
 * spawning a process every few seconds to learn the CPU load would cost more
 * than the load it reports. The arithmetic is the same as `vr system snapshot`
 * performs — a delta of /proc/stat jiffies, and MemAvailable against MemTotal —
 * so the two never disagree by more than one sample. It lives in procStats.js,
 * free of any shell import, so it can be tested on its own.
 *
 * Governed by `menuBarSystemStats`, which the desktop app and `vr config` both
 * write and the shell reads through the settings watcher. Off means no timer at
 * all, not a hidden label.
 */

import Clutter from 'gi://Clutter';
import GLib from 'gi://GLib';
import St from 'gi://St';

import { cpuPercent, memoryPercent, parseCpuTicks } from './procStats.js';

/** How often the readout is refreshed. Matches Edith's two-second cadence. */
const TICK_SECONDS = 2;

const decoder = new TextDecoder();

function readText(path) {
    try {
        const [ok, bytes] = GLib.file_get_contents(path);
        return ok ? decoder.decode(bytes) : null;
    } catch {
        return null;
    }
}

export class SystemStatsIndicator {
    /** @param {import('./settings.js').SettingsWatcher} settings */
    constructor(settings) {
        this._settings = settings;
        this._unsubscribe = null;
        this._previous = null;
        this._tickId = 0;
        this._enabled = false;

        this.actor = new St.BoxLayout({
            style_class: 'veronica-stats',
            y_align: Clutter.ActorAlign.CENTER,
            visible: false,
        });
        this._cpu = new St.Label({ style_class: 'veronica-stats-value' });
        this._memory = new St.Label({ style_class: 'veronica-stats-value' });
        this.actor.add_child(this._cpu);
        this.actor.add_child(new St.Label({
            text: '·',
            style_class: 'veronica-stats-separator',
        }));
        this.actor.add_child(this._memory);
    }

    enable() {
        // Subscribing fires once immediately, so the readout is correct at login
        // and then reacts the moment the setting changes anywhere.
        this._unsubscribe = this._settings?.subscribe(
            () => this._apply(this._settings.bool('menuBarSystemStats', false))
        ) ?? null;
    }

    disable() {
        this._stopTicking();
        this._unsubscribe?.();
        this._unsubscribe = null;
        this._settings = null;
    }

    _apply(enabled) {
        if (enabled === this._enabled)
            return;
        this._enabled = enabled;
        if (enabled)
            this._startTicking();
        else
            this._stopTicking();
    }

    _startTicking() {
        if (this._tickId)
            return;
        // Seed the CPU baseline, then wait one interval: a first reading has no
        // delta to compare against, so showing a number now would be a guess.
        this._previous = this._readCpu();
        this._cpu.text = 'CPU —';
        this._memory.text = 'MEM —';
        this.actor.visible = true;
        this._tickId = GLib.timeout_add_seconds(
            GLib.PRIORITY_DEFAULT_IDLE,
            TICK_SECONDS,
            () => {
                this._tick();
                return GLib.SOURCE_CONTINUE;
            }
        );
    }

    _stopTicking() {
        if (this._tickId) {
            GLib.Source.remove(this._tickId);
            this._tickId = 0;
        }
        this._previous = null;
        this.actor.visible = false;
    }

    _readCpu() {
        return parseCpuTicks(readText('/proc/stat'));
    }

    _tick() {
        const current = this._readCpu();
        const cpu = cpuPercent(this._previous, current);
        this._previous = current ?? this._previous;
        const memory = memoryPercent(readText('/proc/meminfo'));

        // An unreadable sample keeps the last figure rather than flashing a dash:
        // one missed tick is not news.
        if (cpu !== null)
            this._cpu.text = `CPU ${Math.round(cpu)}%`;
        if (memory !== null)
            this._memory.text = `MEM ${Math.round(memory)}%`;
    }
}
