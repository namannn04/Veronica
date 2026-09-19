/* Keystroke Highlight: a keycap on screen for every key press.
 *
 * Edith installs a listen-only `CGEventTap`, which macOS grants after the user
 * approves Input Monitoring. Wayland has no such grant to give — a client
 * cannot observe key presses meant for another window, by design, and no
 * permission changes that. The compositor can, so on Ubuntu both the reading
 * and the drawing happen here.
 *
 * It stays listen-only. The handler is connected to the stage's `captured-event`
 * and always returns `Clutter.EVENT_PROPAGATE`, so every press continues to the
 * window that was going to get it. Nothing is consumed, blocked or rewritten,
 * whether the overlay is running or paused.
 *
 * Two things are deliberately never shown. A password field takes a keyboard
 * grab, and while any grab other than the shell's own is in place the overlay
 * records nothing — that is the Wayland equivalent of Edith refusing to display
 * secure keyboard input. Modifiers pressed on their own resolve to no labels
 * and are dropped by the queue rather than flashing a bare "Ctrl".
 *
 * The row is drawn in `Main.layoutManager.uiGroup` rather than the window group,
 * so it sits above every window — the point of the feature — and is marked
 * non-reactive so it never takes a click.
 */

import Clutter from 'gi://Clutter';
import GLib from 'gi://GLib';
import St from 'gi://St';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';

import {
    KeystrokeQueue,
    MAXIMUM_VISIBLE,
    clampDuration,
    labelsFor,
    parsePosition,
} from './keystrokeLabels.js';

/** How far the row sits from the edge of the work area. */
const EDGE_MARGIN = 64;
/** How long a cap fades in and out. Short enough not to lag the typing. */
const FADE_MS = 140;

export class KeystrokeHighlight {
    /** @param {import('./settings.js').SettingsWatcher} settings */
    constructor(settings) {
        this._settings = settings;
        this._unsubscribe = null;
        this._running = false;
        this._container = null;
        this._eventId = 0;
        this._sweepId = 0;
        this._monitorsId = 0;
        this._queue = new KeystrokeQueue(MAXIMUM_VISIBLE);
        this._actors = new Map();
        this._config = {
            duration: clampDuration(undefined),
            position: parsePosition(undefined),
        };
    }

    enable() {
        this._unsubscribe = this._settings?.subscribe(values => this._apply(values)) ?? null;
    }

    disable() {
        this._unsubscribe?.();
        this._unsubscribe = null;
        this._stop();
        this._settings = null;
    }

    /** Whether the overlay is drawing right now, for the action bridge. */
    get running() {
        return this._running;
    }

    _apply(values) {
        this._config = {
            // The raw stored values, not coerced: the clamp decides what counts
            // as a number, so a string falls back rather than becoming NaN.
            duration: clampDuration(values.keystrokeHighlightDuration),
            position: parsePosition(values.keystrokeHighlightPosition),
        };
        // Active alone must not draw: otherwise the Extensions switch would not
        // actually stop the overlay. This mirrors `is_running` in the core.
        const wanted =
            values.keystrokeHighlightEnabled === true && values.keystrokeHighlightActive === true;
        if (wanted !== this._running) {
            if (wanted)
                this._start();
            else
                this._stop();
            return;
        }
        if (this._running)
            this._position();
    }

    _start() {
        if (this._running)
            return;
        this._running = true;
        this._build();

        this._eventId = global.stage.connect(
            'captured-event', (_actor, event) => this._onEvent(event));
        this._monitorsId = Main.layoutManager.connect('monitors-changed', () => this._position());

        // Caps expire on a clock rather than one timer each, so a fast typist
        // does not leave dozens of sources behind.
        this._sweepId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 100, () => {
            this._sweep();
            return GLib.SOURCE_CONTINUE;
        });
    }

    _stop() {
        if (this._eventId) {
            global.stage.disconnect(this._eventId);
            this._eventId = 0;
        }
        if (this._monitorsId) {
            Main.layoutManager.disconnect(this._monitorsId);
            this._monitorsId = 0;
        }
        if (this._sweepId) {
            GLib.source_remove(this._sweepId);
            this._sweepId = 0;
        }
        // Pausing hides what is on screen without disabling the extension.
        this._queue.clear();
        for (const actor of this._actors.values())
            actor.destroy();
        this._actors.clear();
        this._container?.destroy();
        this._container = null;
        this._running = false;
    }

    _build() {
        this._container = new St.BoxLayout({
            style_class: 'veronica-keystroke-row',
            reactive: false,
            can_focus: false,
            track_hover: false,
        });
        Main.layoutManager.uiGroup.add_child(this._container);
        this._position();
    }

    /**
     * Centre the row horizontally on the primary monitor, at the configured
     * edge of its *work area* so it never lands under the top bar or a dock.
     */
    _position() {
        if (!this._container)
            return;
        const monitor = Main.layoutManager.primaryMonitor;
        if (!monitor)
            return;
        const work = Main.layoutManager.getWorkAreaForMonitor(monitor.index);
        const [, height] = this._container.get_preferred_height(-1);
        const [, width] = this._container.get_preferred_width(-1);
        this._container.set_position(
            Math.round(work.x + (work.width - width) / 2),
            this._config.position === 'top'
                ? work.y + EDGE_MARGIN
                : work.y + work.height - height - EDGE_MARGIN
        );
    }

    _onEvent(event) {
        // Listen-only, always: the press continues to whatever was going to
        // receive it, and this returns before anything else can change that.
        try {
            if (event.type() === Clutter.EventType.KEY_PRESS)
                this._record(event);
        } catch (error) {
            console.debug(`veronica: keystroke highlight failed to record a press: ${error}`);
        }
        return Clutter.EVENT_PROPAGATE;
    }

    _record(event) {
        // Nothing is recorded while the shell holds a modal grab. That covers
        // the lock screen, the polkit and network authentication prompts, and
        // Veronica's own keyboard-cleaning mode — every place on this desktop
        // where a password is typed into the shell itself. It is the Wayland
        // equivalent of Edith refusing to display secure keyboard input.
        if (Main.modalCount > 0)
            return;

        const keyval = event.get_key_symbol();
        const unicode = event.get_key_unicode?.() ?? Clutter.keyval_to_unicode(keyval);
        const state = event.get_state();

        const labels = labelsFor(keyval, unicode, state);
        const entry = this._queue.append(labels, GLib.get_monotonic_time() / 1000, this._config.duration);
        if (!entry)
            return;

        this._addCap(entry);
        // Appending may have pushed the oldest cap out of the queue.
        this._reconcile();
        this._position();
    }

    _addCap(entry) {
        const cap = new St.BoxLayout({
            style_class: 'veronica-keystroke-cap',
            reactive: false,
            opacity: 0,
        });
        for (const label of entry.keys) {
            cap.add_child(new St.Label({
                style_class: 'veronica-keystroke-key',
                text: label,
                y_align: Clutter.ActorAlign.CENTER,
            }));
        }
        this._container.add_child(cap);
        this._actors.set(entry.id, cap);
        cap.ease({ opacity: 255, duration: FADE_MS, mode: Clutter.AnimationMode.EASE_OUT_QUAD });
    }

    /** Remove actors whose entry is no longer in the queue. */
    _reconcile() {
        const live = new Set(this._queue.entries.map(entry => entry.id));
        for (const [id, actor] of this._actors) {
            if (!live.has(id))
                this._fadeOut(id, actor);
        }
    }

    _sweep() {
        if (!this._running)
            return;
        const expired = this._queue.removeExpired(GLib.get_monotonic_time() / 1000);
        for (const entry of expired) {
            const actor = this._actors.get(entry.id);
            if (actor)
                this._fadeOut(entry.id, actor);
        }
        if (expired.length > 0)
            this._position();
    }

    _fadeOut(id, actor) {
        // Dropped from the map first, so a second sweep does not fade it twice.
        this._actors.delete(id);
        actor.remove_all_transitions();
        actor.ease({
            opacity: 0,
            duration: FADE_MS,
            mode: Clutter.AnimationMode.EASE_OUT_QUAD,
            onComplete: () => actor.destroy(),
        });
    }
}
