/* Clipboard capture, from inside the compositor.
 *
 * On Wayland only the focused window may read the selection, so no background
 * process can keep a clipboard history. The shell is the compositor, so it can;
 * this watches the selection and pipes each new entry to `vr clipboard record`,
 * which owns the deduplication, cap and storage.
 *
 * Writing back is the same story in reverse: `St.Clipboard.set_text` is the only
 * way to put something on the Wayland clipboard without a focused window.
 */

import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Meta from 'gi://Meta';
import St from 'gi://St';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';

import { findCli, runJson } from './lib.js';

/**
 * Ignore a selection change this soon after Veronica set it itself.
 *
 * Setting the clipboard raises owner-changed, which would re-record the entry
 * that was just pasted. Recording it is harmless — the history deduplicates —
 * but it needlessly reorders the list under the user's cursor.
 */
const SELF_WRITE_GRACE_MS = 400;

export class ClipboardWatcher {
    constructor() {
        this._selection = null;
        this._ownerChangedId = 0;
        this._lastText = null;
        this._lastWriteAt = 0;
        this._pending = false;
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
                if (type !== Meta.SelectionType.SELECTION_CLIPBOARD)
                    return;
                this._onClipboardChanged();
            });
        return true;
    }

    disable() {
        if (this._selection && this._ownerChangedId)
            this._selection.disconnect(this._ownerChangedId);
        this._selection = null;
        this._ownerChangedId = 0;
        this._lastText = null;
    }

    _onClipboardChanged() {
        if (GLib.get_monotonic_time() / 1000 - this._lastWriteAt < SELF_WRITE_GRACE_MS)
            return;
        // Reading is asynchronous; ignore overlapping changes rather than
        // queueing them, since only the latest clipboard content matters.
        if (this._pending)
            return;
        this._pending = true;

        St.Clipboard.get_default().get_text(St.ClipboardType.CLIPBOARD, (_clipboard, text) => {
            this._pending = false;
            if (!text || !text.trim())
                return;
            // The compositor can raise owner-changed for the same content, for
            // instance when focus moves between windows.
            if (text === this._lastText)
                return;
            this._lastText = text;
            console.debug(`veronica: recording a copy of ${text.length} characters`);
            this._record(text);
        });
    }

    /** Pipe the text to `vr clipboard record` on stdin. */
    _record(text) {
        const cli = findCli();
        if (!cli)
            return;
        try {
            const proc = Gio.Subprocess.new(
                [cli, 'clipboard', 'record'],
                Gio.SubprocessFlags.STDIN_PIPE | Gio.SubprocessFlags.STDERR_PIPE
            );
            // Text goes on stdin so no shell quoting is involved and the content
            // can be anything at all, including newlines and quotes.
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

    /** Put text back on the clipboard. */
    write(text) {
        this._lastWriteAt = GLib.get_monotonic_time() / 1000;
        this._lastText = text;
        St.Clipboard.get_default().set_text(St.ClipboardType.CLIPBOARD, text);
        Main.notify('Veronica', 'Copied to the clipboard');
    }
}

/** Recent history entries, for the dropdown's clipboard list. */
export async function recentEntries(limit, cancellable) {
    const rows = await runJson(
        ['clipboard', 'list', '--limit', String(limit)],
        cancellable
    );
    return Array.isArray(rows) ? rows : [];
}

/** One entry's full text, which the preview deliberately does not carry. */
export async function entryText(id, cancellable) {
    const entry = await runJson(['clipboard', 'get', String(id)], cancellable);
    return entry?.text ?? null;
}
