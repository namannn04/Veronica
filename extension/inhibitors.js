/* Process-owned logind inhibitors for notch power actions.
 *
 * The desktop app holds the same locks through zbus while it is running. The
 * Shell extension also owns a lock so a notch click takes effect immediately
 * and remains functional even when the app window/process has not been opened.
 */

import Gio from 'gi://Gio';
import GLib from 'gi://GLib';

const DEFINITIONS = {
    preventSleep: {
        what: 'idle',
        reason: 'Keep Awake is enabled',
    },
    lidAwakeEnabled: {
        what: 'idle:handle-lid-switch',
        reason: 'Lid Awake is enabled',
    },
};

export class PowerInhibitors {
    constructor(onFailed) {
        this._running = new Map();
        this._onFailed = onFailed;
    }

    set(key, enabled) {
        if (!DEFINITIONS[key])
            return false;
        if (!enabled) {
            this._stop(key);
            return true;
        }
        if (this._running.has(key))
            return true;

        const inhibit = GLib.find_program_in_path('systemd-inhibit');
        const sleep = GLib.find_program_in_path('sleep');
        if (!inhibit || !sleep)
            return false;

        const definition = DEFINITIONS[key];
        try {
            const process = Gio.Subprocess.new([
                inhibit,
                `--what=${definition.what}`,
                '--who=Veronica',
                `--why=${definition.reason}`,
                '--mode=block',
                sleep,
                'infinity',
            ], Gio.SubprocessFlags.STDERR_PIPE);
            const record = { process, intentional: false };
            this._running.set(key, record);
            process.wait_check_async(null, (source, result) => {
                try {
                    source.wait_check_finish(result);
                } catch (_error) {
                    // A forced exit while switching off is expected.
                }
                if (this._running.get(key) !== record)
                    return;
                this._running.delete(key);
                if (!record.intentional)
                    this._onFailed?.(key);
            });
            return true;
        } catch (error) {
            console.debug(`veronica: cannot acquire ${key} inhibitor: ${error}`);
            return false;
        }
    }

    _stop(key) {
        const record = this._running.get(key);
        if (!record)
            return;
        record.intentional = true;
        this._running.delete(key);
        try {
            record.process.force_exit();
        } catch (_error) {
            // It may have exited between the map lookup and force_exit().
        }
    }

    destroy() {
        for (const key of [...this._running.keys()])
            this._stop(key);
        this._onFailed = null;
    }
}
