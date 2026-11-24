import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Meta from 'gi://Meta';
import St from 'gi://St';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';

import { isSecretCopy } from './clipboardPolicy.js';
import { findCli, runJson } from './lib.js';

const SELF_WRITE_GRACE_MS = 400;

export class ClipboardWatcher {
    constructor() {
        this._selection = null;
        this._ownerChangedId = 0;
        this._lastText = null;
        this._lastWriteAt = 0;
        this._pending = false;
        this._generation = 0;
    }

    enable() {
        const selection = global.display?.get_selection?.();
        if (!selection) {
            console.debug('veronica: no selection to watch; clipboard history is off');
            return false;
        }
        this._selection = selection;
        this._ownerChangedId = selection.connect('owner-changed',
            (_selection, type) => {
                if (type !== Meta.SelectionType.SELECTION_CLIPBOARD) {
                    return;
                }
                this._onClipboardChanged();
            });
        return true;
    }

    disable() {
        if (this._selection && this._ownerChangedId) {
            this._selection.disconnect(this._ownerChangedId);
        }
        this._selection = null;
        this._ownerChangedId = 0;
        this._lastText = null;
        this._pending = false;
        this._generation += 1;
    }

    _onClipboardChanged() {
        if (GLib.get_monotonic_time() / 1000 - this._lastWriteAt < SELF_WRITE_GRACE_MS) {
            return;
        }
        if (this._pending) {
            return;
        }
        if (isSecretCopy(this._mimeTypes())) {
            return;
        }
        this._pending = true;
        const generation = this._generation;

        St.Clipboard.get_default().get_text(St.ClipboardType.CLIPBOARD, (_clipboard, text) => {
            if (generation !== this._generation) {
                return;
            }
            this._pending = false;
            if (!text?.trim()) {
                return;
            }
            if (text === this._lastText) {
                return;
            }
            this._lastText = text;
            console.debug(`veronica: recording a copy of ${text.length} characters`);
            this._record(text);
        });
    }

    _mimeTypes() {
        try {
            return this._selection?.get_mimetypes(Meta.SelectionType.SELECTION_CLIPBOARD) ?? [];
        } catch (error) {
            console.debug(`veronica: cannot list clipboard types: ${error}`);
            return [];
        }
    }

    _record(text) {
        const cli = findCli();
        if (!cli) {
            return;
        }
        try {
            const proc = Gio.Subprocess.new(
                [cli, 'clipboard', 'record', '--exact'],
                Gio.SubprocessFlags.STDIN_PIPE | Gio.SubprocessFlags.STDERR_PIPE
            );
            proc.communicate_utf8_async(text, null, (source, result) => {
                try {
                    source.communicate_utf8_finish(result);
                } catch (error) {
                    console.debug(`veronica: cannot record a copy: ${error}`);
                }
            });
        } catch (error) {
            console.debug(`veronica: cannot run the recorder: ${error}`);
        }
    }

    write(text, notify = true) {
        this._lastWriteAt = GLib.get_monotonic_time() / 1000;
        this._lastText = text;
        St.Clipboard.get_default().set_text(St.ClipboardType.CLIPBOARD, text);
        if (notify) {
            Main.notify('Veronica', 'Copied to the clipboard');
        }
    }
}

export async function recentEntries(limit, cancellable) {
    const rows = await runJson(
        ['clipboard', 'list', '--limit', String(limit)],
        cancellable, { terminateOnCancel: true }
    );
    return Array.isArray(rows) ? rows : [];
}

export async function entryText(id, cancellable) {
    const entry = await runJson(['clipboard', 'get', String(id)], cancellable);
    return entry?.text ?? null;
}
