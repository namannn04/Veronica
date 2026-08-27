/* Veronica's settings, read from inside the shell.
 *
 * The extension owns no settings storage: the desktop app and `vr config` write
 * one JSON file, and this reads that same file so a switch flipped anywhere is
 * seen everywhere.
 *
 * It reads the file rather than running `vr config get`, because anything that
 * needs a value on a timer — the CPU readout re-checking whether it is switched
 * on — would otherwise spawn a process every few seconds to learn one boolean.
 * A file monitor also reacts the moment the file changes rather than at the next
 * poll.
 *
 * The path is derived the same way `AppDirectories` derives it, honouring
 * XDG_CONFIG_HOME with the spec's fallback.
 */

import Gio from 'gi://Gio';
import GLib from 'gi://GLib';

const APP_DIR = 'veronica';
const decoder = new TextDecoder();

/** Where the settings file lives, honouring XDG_CONFIG_HOME. */
export function settingsPath() {
    return GLib.build_filenamev([GLib.get_user_config_dir(), APP_DIR, 'settings.json']);
}

/**
 * Parse a settings document.
 *
 * A missing or damaged file is an empty document rather than an error: the shell
 * must not lose its clock because a config file was mid-write.
 */
export function parseSettings(text) {
    if (!text)
        return {};
    try {
        const parsed = JSON.parse(text);
        return parsed && typeof parsed === 'object' && !Array.isArray(parsed) ? parsed : {};
    } catch {
        return {};
    }
}

export class SettingsWatcher {
    constructor() {
        this._values = {};
        this._file = null;
        this._monitor = null;
        this._changedId = 0;
        this._listeners = new Set();
    }

    enable() {
        this._file = Gio.File.new_for_path(settingsPath());
        this._read();
        try {
            this._monitor = this._file.monitor_file(Gio.FileMonitorFlags.NONE, null);
            this._changedId = this._monitor.connect('changed', () => this._read());
        } catch (error) {
            // Without a monitor the values simply stay as they were at login,
            // which is better than failing to enable at all.
            console.debug(`veronica: cannot watch the settings file: ${error}`);
        }
        return true;
    }

    disable() {
        if (this._monitor && this._changedId)
            this._monitor.disconnect(this._changedId);
        this._monitor?.cancel();
        this._monitor = null;
        this._changedId = 0;
        this._file = null;
        this._listeners.clear();
        this._values = {};
    }

    /** Every stored value, as last read. */
    get values() {
        return this._values;
    }

    /** One value, or `fallback` when the key is absent. */
    get(key, fallback = undefined) {
        const value = this._values[key];
        return value === undefined ? fallback : value;
    }

    /** A boolean, treating anything non-boolean as absent. */
    bool(key, fallback = false) {
        const value = this._values[key];
        return typeof value === 'boolean' ? value : fallback;
    }

    /**
     * Call `listener` on every change, and once immediately so a caller does not
     * have to handle "not read yet".
     *
     * Returns a function that stops listening.
     */
    subscribe(listener) {
        this._listeners.add(listener);
        listener(this._values);
        return () => this._listeners.delete(listener);
    }

    _read() {
        let text = null;
        try {
            const [ok, bytes] = this._file.load_contents(null);
            if (ok)
                text = decoder.decode(bytes);
        } catch {
            // Absent on a first launch, or momentarily replaced by the atomic
            // rename the app uses to save. Either way, nothing to report.
        }
        const next = parseSettings(text);
        // A file monitor fires several times for one atomic replace, so only a
        // real change is forwarded.
        if (JSON.stringify(next) === JSON.stringify(this._values))
            return;
        this._values = next;
        for (const listener of this._listeners) {
            try {
                listener(next);
            } catch (error) {
                console.debug(`veronica: a settings listener failed: ${error}`);
            }
        }
    }
}
