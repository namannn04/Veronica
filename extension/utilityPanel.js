/* Inline image and QR tasks. Pixel work stays in a short-lived CLI subprocess. */
import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import St from 'gi://St';
import { launchNotchTools, runJson, vbox } from './lib.js';

const MAX_BYTES = 20 * 1024 * 1024;
export class UtilityPanel {
    constructor(kind, parentCancellable, reopen) {
        this.kind = kind;
        this._parent = parentCancellable;
        this._reopen = reopen;
        this._generation = 0;
        this._job = null;
        this._path = null;
        this._result = null;
        this.actor = vbox({ style_class: 'veronica-shelf-panel veronica-utility-panel', x_expand: true });
        const card = new St.BoxLayout({ style_class: 'veronica-utility-card', x_expand: true });
        const left = vbox({ style_class: 'veronica-utility-controls', x_expand: true });
        left.add_child(new St.Label({ text: kind === 'image' ? 'Image converter' : 'QR code', style_class: 'veronica-utility-title' }));
        this._status = new St.Label({ text: kind === 'image' ? 'PNG, JPG / JPEG or SVG. Everything stays local.' : 'A direct link. No tracking, no expiry.', style_class: 'veronica-utility-detail' });
        this._status.clutter_text.line_wrap = true;
        left.add_child(this._status);
        if (kind === 'image') {
            const intake = new St.BoxLayout({ style_class: 'veronica-utility-row' });
            intake.add_child(this._button('Choose image', () => this._choose()));
            intake.add_child(this._button('Paste', () => this._paste()));
            left.add_child(intake);
            const formats = new St.BoxLayout({ style_class: 'veronica-utility-row' });
            this._formats = ['png', 'jpg', 'svg'].map(format => {
                const button = this._button(format.toUpperCase(), () => this._convert(format));
                button.reactive = false; formats.add_child(button); return button;
            });
            left.add_child(formats);
            const desktop = this._button('Drag & drop window ↗', () => launchNotchTools());
            desktop.add_style_class_name('veronica-utility-secondary');
            left.add_child(desktop);
        } else {
            this._entry = new St.Entry({ hint_text: 'Paste a link…', can_focus: true, style_class: 'veronica-utility-entry', x_expand: true });
            this._entry.clutter_text.connect('activate', () => this._generate());
            left.add_child(this._entry);
            left.add_child(this._button('Generate QR', () => this._generate()));
        }
        this._preview = new St.Icon({ icon_name: kind === 'image' ? 'image-x-generic-symbolic' : 'view-grid-symbolic', style_class: 'veronica-utility-preview', icon_size: 112, y_align: Clutter.ActorAlign.CENTER });
        card.add_child(left); card.add_child(this._preview); this.actor.add_child(card);
        this._exports = new St.BoxLayout({ style_class: 'veronica-utility-row', visible: false });
        this._exports.add_child(this._button('Download', () => this._save()));
        this._exports.add_child(this._button('Copy image', () => this._copy()));
        this.actor.add_child(this._exports);
    }
    _button(label, action) {
        const button = new St.Button({ label, can_focus: true, style_class: 'veronica-utility-button' });
        button.connect('clicked', () => { Promise.resolve(action()).catch(error => this._error(error)); });
        return button;
    }
    _error(error) { if (this.actor) this._status.text = String(error?.message ?? error); }
    async _run(args) {
        this._job?.cancel(); this._job = new Gio.Cancellable();
        const token = ++this._generation;
        this._exports.hide(); this._result = null; this._status.text = 'Working locally…';
        const result = await runJson(['utility', ...args], this._job, { terminateOnCancel: true });
        if (!this.actor || token !== this._generation) return;
        this._job = null;
        if (!result?.preview) { this._error('Could not process this image. Use a valid image up to 20 MiB / 16 megapixels.'); return; }
        this._result = result;
        this._preview.gicon = Gio.FileIcon.new(Gio.File.new_for_path(result.preview));
        this._status.text = result.artifact ? 'Ready. Download the file or copy its pixels.' : `${result.format} · ${result.width} × ${result.height}. Choose an output format.`;
        for (const button of this._formats ?? []) button.reactive = true;
        this._exports.visible = Boolean(result.artifact);
    }
    async _choose() {
        const proc = Gio.Subprocess.new(['zenity', '--file-selection', '--title=Choose an image', '--file-filter=Images | *.png *.jpg *.jpeg *.svg *.PNG *.JPG *.JPEG *.SVG'], Gio.SubprocessFlags.STDOUT_PIPE | Gio.SubprocessFlags.STDERR_PIPE);
        const [stdout] = await proc.communicate_utf8_async(null, this._parent);
        if (this.actor && proc.get_successful() && stdout?.trim()) {
            this._path = stdout.trim(); await this._run(['image', this._path]);
        }
        if (this.actor) this._reopen?.();
    }
    _paste() {
        const clipboard = St.Clipboard.get_default();
        if (!clipboard.get_content) { this._error('Use Paste in the drag & drop window on this GNOME version.'); return; }
        clipboard.get_content(St.ClipboardType.CLIPBOARD, 'image/png', async (_clipboard, bytes) => {
            try {
                if (!this.actor) return;
                if (!bytes || bytes.get_size() === 0) { this._error('Copy an image first, then choose Paste.'); return; }
                if (bytes.get_size() > MAX_BYTES) { this._error('Clipboard image must be at most 20 MiB.'); return; }
                const root = GLib.build_filenamev([GLib.get_user_cache_dir(), 'veronica', 'notch-input']);
                GLib.mkdir_with_parents(root, 0o700);
                const path = GLib.build_filenamev([root, `${GLib.uuid_string_random()}.png`]);
                const file = Gio.File.new_for_path(path);
                await new Promise((resolve, reject) => file.replace_contents_bytes_async(bytes, null, false, Gio.FileCreateFlags.PRIVATE, this._parent, (source, result) => {
                    try { source.replace_contents_finish(result); resolve(); } catch (error) { reject(error); }
                }));
                if (!this.actor) { GLib.unlink(path); return; }
                // Keep just this panel's current clipboard input until replaced/destroyed.
                if (this._pastePath) GLib.unlink(this._pastePath);
                this._pastePath = path; this._path = path;
                await this._run(['image', path]);
            } catch (error) { this._error(error); }
        });
    }
    _convert(format) { return this._path ? this._run(['image', this._path, format]) : undefined; }
    _generate() {
        const link = this._entry.get_text().trim();
        if (!link) { this._error('Paste a link first.'); return; }
        return this._run(['qr', link]);
    }
    async _copy() {
        const path = this._result?.copyPath;
        if (!path) return;
        const file = Gio.File.new_for_path(path);
        const info = await new Promise((resolve, reject) => file.query_info_async('standard::size', Gio.FileQueryInfoFlags.NONE, GLib.PRIORITY_DEFAULT, this._parent, (source, result) => {
            try { resolve(source.query_info_finish(result)); } catch (error) { reject(error); }
        }));
        if (info.get_size() > MAX_BYTES) { this._error('Use Download for this large image.'); return; }
        const bytes = await new Promise((resolve, reject) => file.load_bytes_async(this._parent, (source, result) => {
            try { resolve(source.load_bytes_finish(result)[0]); } catch (error) { reject(error); }
        }));
        if (!this.actor) return;
        St.Clipboard.get_default().set_content(St.ClipboardType.CLIPBOARD, 'image/png', bytes);
        this._status.text = 'Image copied.';
    }
    async _save() {
        const path = this._result?.artifact;
        if (!path) return;
        const proc = Gio.Subprocess.new(['zenity', '--file-selection', '--save', '--confirm-overwrite', `--filename=veronica.${path.split('.').pop()}`, '--title=Download image'], Gio.SubprocessFlags.STDOUT_PIPE | Gio.SubprocessFlags.STDERR_PIPE);
        const [stdout] = await proc.communicate_utf8_async(null, this._parent);
        if (this.actor && proc.get_successful() && stdout?.trim()) {
            const source = Gio.File.new_for_path(path), target = Gio.File.new_for_path(stdout.trim());
            await new Promise((resolve, reject) => source.copy_async(target, Gio.FileCopyFlags.OVERWRITE, GLib.PRIORITY_DEFAULT, this._parent, null, (file, result) => {
                try { file.copy_finish(result); resolve(); } catch (error) { reject(error); }
            }));
            this._status.text = 'Image saved.';
        }
        if (this.actor) this._reopen?.();
    }
    close() { this._generation++; this._job?.cancel(); this._job = null; }
    destroy() {
        this.close();
        if (this._pastePath) GLib.unlink(this._pastePath);
        this.actor?.destroy(); this.actor = null; this._reopen = null; this._result = null;
    }
}
