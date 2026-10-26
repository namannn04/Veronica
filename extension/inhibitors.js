/* Process-owned inhibitors for notch power actions.
 *
 * The desktop app holds the same locks through zbus while it is running. The
 * Shell extension also owns a lock so a notch click takes effect immediately
 * and remains functional even when the app window/process has not been opened.
 *
 * Each switch is held by up to two processes. `systemd-inhibit` covers logind,
 * which is what the lid switch obeys. `gnome-session-inhibit` covers GNOME's
 * own idle handling: gsd-power blanks and suspends on gnome-session's
 * inhibitors and never reads logind's idle locks, so without it Keep Awake
 * left the screen to blank and the machine to suspend on schedule.
 */

import Gio from 'gi://Gio';
import GLib from 'gi://GLib';

const APP_ID = 'io.github.namannn04.Veronica';

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

/** The argument vectors that hold a switch, one process each. */
function commandsFor(definition) {
    const commands = [];
    const inhibit = GLib.find_program_in_path('systemd-inhibit');
    const sleep = GLib.find_program_in_path('sleep');
    if (inhibit && sleep) {
        commands.push([
            inhibit,
            `--what=${definition.what}`,
            '--who=Veronica',
            `--why=${definition.reason}`,
            '--mode=block',
            sleep,
            'infinity',
        ]);
    }
    const session = GLib.find_program_in_path('gnome-session-inhibit');
    if (session) {
        commands.push([
            session,
            `--app-id=${APP_ID}`,
            `--reason=${definition.reason}`,
            '--inhibit=idle',
            '--inhibit=suspend',
            '--inhibit-only',
        ]);
    }
    return commands;
}

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

        const commands = commandsFor(DEFINITIONS[key]);
        if (commands.length === 0)
            return false;

        const record = { processes: [], intentional: false };
        try {
            for (const argv of commands)
                record.processes.push(Gio.Subprocess.new(argv, Gio.SubprocessFlags.STDERR_PIPE));
        } catch (error) {
            console.debug(`veronica: cannot acquire ${key} inhibitor: ${error}`);
            this._kill(record);
            return false;
        }
        this._running.set(key, record);
        for (const process of record.processes) {
            process.wait_check_async(null, (source, result) => {
                try {
                    source.wait_check_finish(result);
                } catch (_error) {
                    // A forced exit while switching off is expected.
                }
                if (this._running.get(key) !== record)
                    return;
                // Either half dying means the switch no longer holds: drop
                // the other half too rather than claim a partial lock.
                this._running.delete(key);
                this._kill(record);
                if (!record.intentional)
                    this._onFailed?.(key);
            });
        }
        return true;
    }

    _stop(key) {
        const record = this._running.get(key);
        if (!record)
            return;
        record.intentional = true;
        this._running.delete(key);
        this._kill(record);
    }

    _kill(record) {
        for (const process of record.processes) {
            try {
                process.force_exit();
            } catch (_error) {
                // It may already have exited.
            }
        }
    }

    destroy() {
        for (const key of [...this._running.keys()])
            this._stop(key);
        this._onFailed = null;
    }
}
