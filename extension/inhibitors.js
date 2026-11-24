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

function sessionManagerPresent() {
    try {
        const [owned] = Gio.DBus.session.call_sync(
            'org.freedesktop.DBus', '/org/freedesktop/DBus', 'org.freedesktop.DBus',
            'NameHasOwner', new GLib.Variant('(s)', ['org.gnome.SessionManager']),
            new GLib.VariantType('(b)'), Gio.DBusCallFlags.NONE, 1000, null
        ).deepUnpack();
        return owned;
    } catch (error) {
        console.debug(`veronica: cannot ask the bus for gnome-session: ${error}`);
        return false;
    }
}

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
    if (session && sessionManagerPresent()) {
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
        if (!DEFINITIONS[key]) {
            return false;
        }
        if (!enabled) {
            this._stop(key);
            return true;
        }
        if (this._running.has(key)) {
            return true;
        }

        const commands = commandsFor(DEFINITIONS[key]);
        if (commands.length === 0) {
            return false;
        }

        const record = { processes: [], intentional: false };
        try {
            for (const argv of commands) {
                record.processes.push(Gio.Subprocess.new(argv, Gio.SubprocessFlags.STDERR_PIPE));
            }
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
                }
                if (this._running.get(key) !== record) {
                    return;
                }
                this._running.delete(key);
                this._kill(record);
                if (!record.intentional) {
                    this._onFailed?.(key);
                }
            });
        }
        return true;
    }

    _stop(key) {
        const record = this._running.get(key);
        if (!record) {
            return;
        }
        record.intentional = true;
        this._running.delete(key);
        this._kill(record);
    }

    _kill(record) {
        for (const process of record.processes) {
            try {
                process.force_exit();
            } catch (_error) {
            }
        }
    }

    destroy() {
        for (const key of [...this._running.keys()]) {
            this._stop(key);
        }
        this._onFailed = null;
    }
}
