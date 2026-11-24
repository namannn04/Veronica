import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import St from 'gi://St';

const DesktopAppInfo = await import('gi://GioUnix')
    .then(module => module.default.DesktopAppInfo)
    .catch(() => Gio.DesktopAppInfo);

try {
    if (!Gio.Subprocess.prototype.communicate_utf8_async_promisified) {
        Gio._promisify(Gio.Subprocess.prototype, 'communicate_utf8_async');
        Gio.Subprocess.prototype.communicate_utf8_async_promisified = true;
    }
} catch (error) {
    console.debug(`veronica: communicate_utf8_async already promisified: ${error}`);
}

const CLI_FALLBACKS = [
    '/usr/bin/vr',
    '/usr/local/bin/vr',
    `${GLib.get_home_dir()}/.local/bin/vr`,
    GLib.build_filenamev([
        GLib.path_get_dirname(GLib.filename_from_uri(import.meta.url)[0]),
        'vr',
    ]),
];

export function brandIcon() {
    const path = GLib.build_filenamev([
        GLib.path_get_dirname(GLib.filename_from_uri(import.meta.url)[0]),
        'veronica-logo.png',
    ]);
    if (!GLib.file_test(path, GLib.FileTest.EXISTS)) {
        return null;
    }
    return Gio.FileIcon.new(Gio.File.new_for_path(path));
}

let cachedCli = null;
export function findCli() {
    if (cachedCli !== null) {
        return cachedCli;
    }

    const onPath = GLib.find_program_in_path('vr');
    if (onPath) {
        cachedCli = onPath;
        return cachedCli;
    }
    for (const candidate of CLI_FALLBACKS) {
        if (GLib.file_test(candidate, GLib.FileTest.IS_EXECUTABLE)) {
            cachedCli = candidate;
            return cachedCli;
        }
    }
    cachedCli = '';
    return cachedCli;
}

export function resetCliCache() {
    cachedCli = null;
}

export async function runJson(args, cancellable = null, { terminateOnCancel = false } = {}) {
    if (cancellable?.is_cancelled()) { return null; }
    const cli = findCli();
    if (!cli) {
        return null;
    }

    let proc = null, cancelledId = 0, deadlineId = 0;
    try {
        proc = Gio.Subprocess.new(
            [cli, ...args, '--json'],
            Gio.SubprocessFlags.STDOUT_PIPE | Gio.SubprocessFlags.STDERR_PIPE
        );
        if (terminateOnCancel) {
            const stop = () => { try { proc.force_exit(); } catch {} };
            if (cancellable?.is_cancelled()) { stop(); return null; }
            if (cancellable) { cancelledId = cancellable.connect(stop); }
            deadlineId = GLib.timeout_add_seconds(GLib.PRIORITY_DEFAULT, 30, () => { deadlineId = 0; stop(); return GLib.SOURCE_REMOVE; });
        }
        const [stdout, stderr] = await proc.communicate_utf8_async(null, cancellable);
        if (!proc.get_successful()) {
            console.debug(`veronica: vr ${args.join(' ')} failed: ${stderr?.trim() ?? ''}`);
            return null;
        }
        if (!stdout || !stdout.trim()) {
            return null;
        }
        return JSON.parse(stdout);
    } catch (error) {
        if (error instanceof Gio.IOErrorEnum && error.code === Gio.IOErrorEnum.CANCELLED) {
            return null;
        }
        console.debug(`veronica: cannot run vr ${args.join(' ')}: ${error}`);
        return null;
    } finally {
        if (cancelledId) { cancellable.disconnect(cancelledId); }
        if (deadlineId) { GLib.Source.remove(deadlineId); }
    }
}

export function launchApp() {
    const app = DesktopAppInfo.new('Veronica.desktop') ??
        DesktopAppInfo.new('io.github.namannn04.Veronica.desktop');
    if (app) {
        app.launch([], null);
        return;
    }
    try {
        Gio.Subprocess.new(['veronica'], Gio.SubprocessFlags.NONE);
    } catch (error) {
        console.debug(`veronica: cannot launch the app: ${error}`);
    }
}

export function launchFileShelf() {
    const app = DesktopAppInfo.new('Veronica.desktop') ??
        DesktopAppInfo.new('io.github.namannn04.Veronica.desktop');
    if (app?.list_actions().includes('FileShelf')) {
        app.launch_action('FileShelf', null);
        return;
    }
    try {
        Gio.Subprocess.new(['veronica', '--file-shelf'], Gio.SubprocessFlags.NONE);
    } catch (error) {
        console.debug(`veronica: cannot open file shelf: ${error}`);
    }
}

export function launchNotchTools() {
    const app = DesktopAppInfo.new('Veronica.desktop') ??
        DesktopAppInfo.new('io.github.namannn04.Veronica.desktop');
    if (app?.list_actions().includes('NotchTools')) {
        app.launch_action('NotchTools', null);
        return;
    }
    Gio.Subprocess.new(['veronica', '--notch-tools'], Gio.SubprocessFlags.NONE);
}

export function money(value) {
    const amount = Number.isFinite(value) ? value : 0;
    return `$${amount.toFixed(2)}`;
}

export function tokens(value) {
    const units = [
        [1e12, 'T'],
        [1e9, 'B'],
        [1e6, 'M'],
        [1e3, 'K'],
    ];
    const decimals = scaled => scaled >= 100 ? 0 : scaled >= 10 ? 1 : 2;
    const round = (scaled, places) => Math.round(scaled * 10 ** places) / 10 ** places;
    for (let index = 0; index < units.length; index++) {
        const [scale, suffix] = units[index];
        if (value >= scale) {
            const scaled = value / scale;
            const rounded = round(scaled, decimals(scaled));
            if (rounded >= 1000 && index > 0) {
                return `1.00${units[index - 1][1]}`;
            }
            return `${rounded.toFixed(decimals(rounded))}${suffix}`;
        }
    }
    const whole = Math.round(value);
    return whole >= 1000 ? '1.00K' : `${whole}`;
}

export function percent(value) {
    return `${Math.round(Number.isFinite(value) ? value : 0)}%`;
}

export function band(value) {
    if (value >= 85) {
        return 'critical';
    }
    if (value >= 60) {
        return 'warning';
    }
    return 'good';
}

const BOX_HAS_ORIENTATION = 'orientation' in St.BoxLayout.prototype;

export function vbox(params = {}) {
    return new St.BoxLayout(BOX_HAS_ORIENTATION
        ? { ...params, orientation: Clutter.Orientation.VERTICAL }
        : { ...params, vertical: true });
}

export function heading(text) {
    return new St.Label({ text, style_class: 'veronica-heading' });
}

export function meterRow(labelText, fraction, valueText, statusBand) {
    const row = new St.BoxLayout({
        style_class: 'veronica-row',
        x_expand: true,
        y_align: Clutter.ActorAlign.CENTER,
    });

    row.add_child(new St.Label({
        text: labelText,
        style_class: 'veronica-row-label',
        y_align: Clutter.ActorAlign.CENTER,
    }));

    const trackWidth = 110;
    const track = new St.Widget({
        style_class: 'veronica-meter',
        width: trackWidth,
        y_align: Clutter.ActorAlign.CENTER,
        layout_manager: new Clutter.BinLayout(),
    });
    const clamped = Math.max(0, Math.min(1, Number.isFinite(fraction) ? fraction : 0));
    const fill = new St.Widget({
        style_class: `veronica-meter-fill${statusBand ? ` ${statusBand}` : ''}`,
        width: Math.max(3, Math.round(trackWidth * clamped)),
        x_align: Clutter.ActorAlign.START,
    });
    track.add_child(fill);
    row.add_child(track);

    row.add_child(new St.Label({
        text: valueText,
        style_class: 'veronica-row-value',
        x_expand: true,
        x_align: Clutter.ActorAlign.END,
        y_align: Clutter.ActorAlign.CENTER,
    }));

    return row;
}

export function textRow(labelText, valueText, dim = false) {
    const row = new St.BoxLayout({ style_class: 'veronica-row', x_expand: true });
    row.add_child(new St.Label({
        text: labelText,
        style_class: `veronica-row-label${dim ? ' veronica-dim' : ''}`,
    }));
    row.add_child(new St.Label({
        text: valueText,
        style_class: 'veronica-row-value',
        x_expand: true,
        x_align: Clutter.ActorAlign.END,
    }));
    return row;
}

export function emptyLabel(text) {
    return new St.Label({ text, style_class: 'veronica-empty' });
}

export function countdown(seconds) {
    if (!Number.isFinite(seconds) || seconds <= 0) {
        return 'now';
    }
    const days = Math.floor(seconds / 86400);
    const hours = Math.floor((seconds % 86400) / 3600);
    const minutes = Math.floor((seconds % 3600) / 60);
    if (days > 0) {
        return `${days}d ${hours}h`;
    }
    if (hours > 0) {
        return `${hours}h ${minutes}m`;
    }
    if (minutes > 0) {
        return `${minutes}m`;
    }
    return `${Math.floor(seconds)}s`;
}

export function launchServices() {
    const app = DesktopAppInfo.new('Veronica.desktop') ??
        DesktopAppInfo.new('io.github.namannn04.Veronica.desktop');
    if (app?.list_actions().includes('Services')) {
        app.launch_action('Services', null);
        return;
    }
    Gio.Subprocess.new(['veronica', '--services'], Gio.SubprocessFlags.NONE);
}
