import Clutter from 'gi://Clutter';
import GLib from 'gi://GLib';
import St from 'gi://St';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';

import { cpuPercent, memoryPercent, parseCpuTicks } from './procStats.js';

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
        this.actor.add_child(
            new St.Label({
                text: '·',
                style_class: 'veronica-stats-separator',
            })
        );
        this.actor.add_child(this._memory);
    }

    enable() {
        this._unsubscribe =
            this._settings?.subscribe(() => this._apply(this._settings.bool('menuBarSystemStats', false))) ?? null;
    }

    disable() {
        this._stopTicking();
        this._unsubscribe?.();
        this._unsubscribe = null;
        this._settings = null;
    }

    _apply(enabled) {
        if (enabled === this._enabled) {
            return;
        }
        this._enabled = enabled;
        if (enabled) {
            this._startTicking();
        } else {
            this._stopTicking();
        }
    }

    _startTicking() {
        if (this._tickId) {
            return;
        }
        this._previous = this._readCpu();
        this._cpu.text = 'CPU –';
        this._memory.text = 'MEM –';
        this.actor.visible = true;
        this._tickId = GLib.timeout_add_seconds(GLib.PRIORITY_DEFAULT_IDLE, TICK_SECONDS, () => {
            this._tick();
            return GLib.SOURCE_CONTINUE;
        });
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

        if (cpu !== null) {
            this._cpu.text = `CPU ${Math.round(cpu)}%`;
        }
        if (memory !== null) {
            this._memory.text = `MEM ${Math.round(memory)}%`;
        }
    }
}

export class StandaloneStats {
    constructor(settings) {
        this._settings = settings;
        this._button = null;
        this._stats = null;
    }

    show() {
        if (this._button) {
            return;
        }
        this._stats = new SystemStatsIndicator(this._settings);
        this._button = new PanelMenu.Button(0.0, 'Veronica system stats', true);
        this._button.add_child(this._stats.actor);
        this._stats.enable();
        Main.panel.addToStatusArea('veronica-system-stats', this._button, 0, 'right');
    }

    hide() {
        this._stats?.disable();
        this._stats = null;
        this._button?.destroy();
        this._button = null;
    }
}
