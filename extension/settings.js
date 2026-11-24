import Gio from 'gi://Gio';
import GLib from 'gi://GLib';

const APP_DIR = 'veronica';
const decoder = new TextDecoder();

export function settingsPath() {
    return GLib.build_filenamev([GLib.get_user_config_dir(), APP_DIR, 'settings.json']);
}

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
        this._reloadId = 0;
        this._listeners = new Set();
    }

    enable() {
        this._file = Gio.File.new_for_path(settingsPath());
        this._read();
        try {
            const parent = this._file.get_parent();
            GLib.mkdir_with_parents(parent.get_path(), 0o700);
            this._monitor = parent.monitor_directory(Gio.FileMonitorFlags.NONE, null);
            this._changedId = this._monitor.connect('changed', (_monitor, file, other) => {
                if (file?.get_basename() !== 'settings.json' && other?.get_basename() !== 'settings.json')
                    return;
                if (this._reloadId)
                    GLib.Source.remove(this._reloadId);
                this._reloadId = GLib.timeout_add(GLib.PRIORITY_DEFAULT_IDLE, 80, () => {
                    this._reloadId = 0;
                    this._read();
                    return GLib.SOURCE_REMOVE;
                });
            });
        } catch (error) {
            console.debug(`veronica: cannot watch the settings file: ${error}`);
        }
        return true;
    }

    disable() {
        if (this._reloadId)
            GLib.Source.remove(this._reloadId);
        this._reloadId = 0;
        if (this._monitor && this._changedId)
            this._monitor.disconnect(this._changedId);
        this._monitor?.cancel();
        this._monitor = null;
        this._changedId = 0;
        this._file = null;
        this._listeners.clear();
        this._values = {};
    }

    get values() {
        return this._values;
    }

    get(key, fallback = undefined) {
        const value = this._values[key];
        return value === undefined ? fallback : value;
    }

    bool(key, fallback = false) {
        const value = this._values[key];
        return typeof value === 'boolean' ? value : fallback;
    }

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
        }
        if (text === null)
            return;
        let parsed;
        try { parsed = JSON.parse(text); } catch { return; }
        if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed))
            return;
        const next = parsed;
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
