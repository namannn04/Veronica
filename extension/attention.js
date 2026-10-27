import Gio from 'gi://Gio';
import GLib from 'gi://GLib';

import { findCli } from './lib.js';

const PULSE_SECONDS = 10;

export class AttentionTracker {
    constructor(settings) {
        this._settings = settings;
        this._timer = 0;
        this._running = false;
    }

    enable() {
        if (this._timer)
            return;
        this._timer = GLib.timeout_add_seconds(GLib.PRIORITY_DEFAULT, PULSE_SECONDS, () => {
            this._pulse();
            return GLib.SOURCE_CONTINUE;
        });
        this._pulse();
    }

    disable() {
        if (this._timer)
            GLib.source_remove(this._timer);
        this._timer = 0;
        this._running = false;
        this._settings = null;
    }

    _pulse() {
        // The feature (its page and this tracker) and tracking itself are two
        // switches. Absent, tracking follows the feature, as it did when the
        // two were one key.
        const tracking = this._settings?.bool('attentionTrackingEnabled', true);
        if (this._running || !this._settings?.bool('tabAttentionEnabled', false) || !tracking)
            return;
        const cli = findCli();
        if (!cli)
            return;
        const window = global.display.focus_window;
        const idleMs = global.backend.get_core_idle_monitor().get_idletime();
        const threshold = Number(this._settings.get('attentionIdleSeconds', 300)) * 1000;
        const idle = !window || idleMs >= threshold;
        const application = idle ? 'Away' : (window.get_wm_class() || window.get_wm_class_instance() || 'Unknown');
        // `--name=value` keeps a value that starts with '-' a value: a
        // terminal titled "-bash: ~" was read as flags, and the pulse failed.
        const args = [cli, 'attention', 'record', `--application=${application}`, `--seconds=${PULSE_SECONDS}`];
        if (idle)
            args.push('--idle');
        else if (this._settings.get('attentionPrivacy', 'applications') === 'detailed')
            args.push(`--title=${window.get_title() || ''}`);
        this._running = true;
        try {
            const process = Gio.Subprocess.new(args, Gio.SubprocessFlags.STDOUT_SILENCE | Gio.SubprocessFlags.STDERR_SILENCE);
            process.wait_async(null, (source, result) => {
                this._running = false;
                try { source.wait_finish(result); } catch { /* next pulse retries */ }
            });
        } catch {
            this._running = false;
        }
    }
}
