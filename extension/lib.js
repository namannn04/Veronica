/* Shared helpers: running `vr`, and small widget builders.
 *
 * The extension deliberately owns no domain logic. Everything it shows comes
 * from `vr ... --json`, so the shell and the app can never disagree about a
 * number, and the extension stays small enough to audit.
 */

import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import St from 'gi://St';
import Clutter from 'gi://Clutter';

// GLib 2.86 moved DesktopAppInfo to GioUnix, and GJS warns on every use of
// the old Gio name. Older releases may not ship GioUnix at all, so it is
// loaded when present and Gio's copy is the fallback.
const DesktopAppInfo = await import('gi://GioUnix')
    .then(module => module.default.DesktopAppInfo)
    .catch(() => Gio.DesktopAppInfo);

// GJS async methods take a callback unless promisified. The shell promisifies a
// handful of these itself, so this is guarded: promisifying twice throws.
try {
    if (!Gio.Subprocess.prototype.communicate_utf8_async_promisified) {
        Gio._promisify(Gio.Subprocess.prototype, 'communicate_utf8_async');
        Gio.Subprocess.prototype.communicate_utf8_async_promisified = true;
    }
} catch (error) {
    console.debug(`veronica: communicate_utf8_async already promisified: ${error}`);
}

/**
 * Where the CLI may live, tried in order after PATH.
 *
 * A shell extension does not inherit a login shell's PATH, and Veronica may be
 * installed system-wide by the package or locally by a developer, so both are
 * checked.
 */
const CLI_FALLBACKS = [
    '/usr/bin/vr',
    '/usr/local/bin/vr',
    `${GLib.get_home_dir()}/.local/bin/vr`,
    // The Flatpak writes a launcher for its sandboxed `vr` next to the copy of
    // this extension it installs; the Debian package has no such file.
    GLib.build_filenamev([
        GLib.path_get_dirname(GLib.filename_from_uri(import.meta.url)[0]),
        'vr',
    ]),
];

/**
 * Locate the `vr` binary once.
 *
 * A shell extension does not inherit a login shell's PATH reliably, so an
 * absolute path is resolved up front and the result cached.
 */
let cachedCli = null;
export function findCli() {
    if (cachedCli !== null)
        return cachedCli;

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

/**
 * Run `vr` with the given arguments and parse its JSON.
 *
 * Never throws: the shell must not break because a subprocess failed, so a
 * failure resolves to null and the caller shows an empty state. `vr` writes
 * exactly one JSON document to stdout and logs to stderr, which is what makes
 * this safe to parse directly.
 */
export async function runJson(args, cancellable = null) {
    const cli = findCli();
    if (!cli)
        return null;

    try {
        const proc = Gio.Subprocess.new(
            [cli, ...args, '--json'],
            Gio.SubprocessFlags.STDOUT_PIPE | Gio.SubprocessFlags.STDERR_PIPE
        );
        const [stdout, stderr] = await proc.communicate_utf8_async(null, cancellable);
        if (!proc.get_successful()) {
            console.debug(`veronica: vr ${args.join(' ')} failed: ${stderr?.trim() ?? ''}`);
            return null;
        }
        if (!stdout || !stdout.trim())
            return null;
        return JSON.parse(stdout);
    } catch (error) {
        // A cancelled call during teardown is expected, not a fault.
        if (error instanceof Gio.IOErrorEnum && error.code === Gio.IOErrorEnum.CANCELLED)
            return null;
        console.debug(`veronica: cannot run vr ${args.join(' ')}: ${error}`);
        return null;
    }
}

/** Launch the desktop application, preferring its desktop entry. */
export function launchApp() {
    // The Debian package installs Veronica.desktop; the Flatpak exports its
    // entry under the application id.
    const app = DesktopAppInfo.new('Veronica.desktop') ??
        DesktopAppInfo.new('io.github.namannn04.Veronica.desktop');
    if (app) {
        app.launch([], null);
        return;
    }
    // Installed without a desktop entry: fall back to the binary.
    try {
        Gio.Subprocess.new(['veronica'], Gio.SubprocessFlags.NONE);
    } catch (error) {
        console.debug(`veronica: cannot launch the app: ${error}`);
    }
}

/** Native URI drop surface, opened from the notch Files tab. */
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

// -- formatting -------------------------------------------------------------

export function money(value) {
    const amount = Number.isFinite(value) ? value : 0;
    return `$${amount.toFixed(2)}`;
}

/** Token counts, three significant figures, matching the app and the CLI. */
export function tokens(value) {
    const units = [
        [1e12, 'T'],
        [1e9, 'B'],
        [1e6, 'M'],
        [1e3, 'K'],
    ];
    // Precision is decided after rounding, as the app's formatter does:
    // deciding first printed 999,999 as "1000K".
    const decimals = scaled => scaled >= 100 ? 0 : scaled >= 10 ? 1 : 2;
    const round = (scaled, places) => Math.round(scaled * 10 ** places) / 10 ** places;
    for (let index = 0; index < units.length; index++) {
        const [scale, suffix] = units[index];
        if (value >= scale) {
            const scaled = value / scale;
            const rounded = round(scaled, decimals(scaled));
            if (rounded >= 1000 && index > 0)
                return `1.00${units[index - 1][1]}`;
            return `${rounded.toFixed(decimals(rounded))}${suffix}`;
        }
    }
    const whole = Math.round(value);
    return whole >= 1000 ? '1.00K' : `${whole}`;
}

export function percent(value) {
    return `${Math.round(Number.isFinite(value) ? value : 0)}%`;
}

/** Status band for a 0-100 reading, matching the app's ring thresholds. */
export function band(value) {
    if (value >= 85)
        return 'critical';
    if (value >= 60)
        return 'warning';
    return 'good';
}

// -- widgets ----------------------------------------------------------------

/**
 * A vertical `St.BoxLayout`, on every supported shell.
 *
 * GNOME 48 added `orientation` to `St.BoxLayout` and deprecated `vertical`;
 * GNOME 46 and 47 have only `vertical`, and GJS throws on a property the class
 * does not have. Passing `orientation` there aborts the whole notch build.
 */
const BOX_HAS_ORIENTATION = 'orientation' in St.BoxLayout.prototype;

export function vbox(params = {}) {
    return new St.BoxLayout(BOX_HAS_ORIENTATION
        ? { ...params, orientation: Clutter.Orientation.VERTICAL }
        : { ...params, vertical: true });
}

export function heading(text) {
    return new St.Label({ text, style_class: 'veronica-heading' });
}

/**
 * A label / meter / value row.
 *
 * The meter's fill is sized as a fraction of the track. `St` has no percentage
 * width, so the fill is given an explicit pixel width against a fixed track.
 */
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
        // Always a sliver, so "present but tiny" is distinguishable from zero.
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

/** A plain label / value row with no meter. */
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

/** A duration as a compact countdown, e.g. "2h 14m". */
export function countdown(seconds) {
    if (!Number.isFinite(seconds) || seconds <= 0)
        return 'now';
    const days = Math.floor(seconds / 86400);
    const hours = Math.floor((seconds % 86400) / 3600);
    const minutes = Math.floor((seconds % 3600) / 60);
    if (days > 0)
        return `${days}d ${hours}h`;
    if (hours > 0)
        return `${hours}h ${minutes}m`;
    if (minutes > 0)
        return `${minutes}m`;
    return `${Math.floor(seconds)}s`;
}
