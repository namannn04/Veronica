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
    const app = Gio.DesktopAppInfo.new('Veronica.desktop');
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
    for (const [scale, suffix] of units) {
        if (value >= scale) {
            const scaled = value / scale;
            if (scaled >= 100)
                return `${scaled.toFixed(0)}${suffix}`;
            if (scaled >= 10)
                return `${scaled.toFixed(1)}${suffix}`;
            return `${scaled.toFixed(2)}${suffix}`;
        }
    }
    return `${Math.round(value)}`;
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
