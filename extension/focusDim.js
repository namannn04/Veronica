/* Focus Dim: darken everything behind the window you are working in.
 *
 * Edith stacks its own translucent windows underneath the focused one. A Wayland
 * client cannot do that — it cannot position itself, raise itself, or learn what
 * else is on screen — so on Ubuntu the compositor draws the overlay instead.
 * That is also why this works on Wayland, where the X11 equivalent would not.
 *
 * How it works: one dim actor per monitor is added to `global.window_group`,
 * which holds every window actor. Raising the focused window's actor above the
 * dim leaves that window bright and everything below it darkened. The overlay is
 * inside the window group rather than over the whole stage, so the panel, the
 * overview and Veronica's own notch stay untouched.
 *
 * The ranges are clamped here as well as in `veronica-core`. The core stops a
 * bad value being *stored*; this stops a hand-edited file from blacking out the
 * screen, which is not a state a user could recover from by using the desktop.
 */

import Clutter from 'gi://Clutter';
import Meta from 'gi://Meta';
import St from 'gi://St';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';

import { clampAnimationSeconds, clampIntensity, parseDisplayMode } from './focusDimMath.js';

export class FocusDim {
    /** @param {import('./settings.js').SettingsWatcher} settings */
    constructor(settings) {
        this._settings = settings;
        this._unsubscribe = null;
        this._overlays = [];
        this._active = false;
        this._focusId = 0;
        this._monitorsId = 0;
        this._restackId = 0;
        // Seeded with the defaults, via the same clamps, so there is no moment
        // where the config is undefined.
        this._config = {
            intensity: clampIntensity(undefined),
            animation: clampAnimationSeconds(undefined),
            mode: parseDisplayMode(undefined),
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

    _apply(values) {
        this._config = {
            // The raw stored values, not coerced: the clamps decide what counts
            // as a number, so a string like "dark" falls back rather than
            // becoming NaN and then a bound.
            intensity: clampIntensity(values.focusDimIntensity),
            animation: clampAnimationSeconds(values.focusDimAnimationDuration),
            mode: parseDisplayMode(values.focusDimOtherDisplaysMode),
        };
        const wanted = values.focusDimEnabled === true;
        if (wanted !== this._active) {
            if (wanted)
                this._start();
            else
                this._stop();
            return;
        }
        // Already in the right state; a changed intensity still has to land.
        if (this._active)
            this._update();
    }

    _start() {
        if (this._active)
            return;
        this._active = true;
        this._build();

        this._focusId = global.display.connect(
            'notify::focus-window', () => this._update());
        // A window opening, closing or being raised changes what is in front of
        // the dim even when focus itself did not move.
        this._restackId = global.display.connect('restacked', () => this._update());
        this._monitorsId = Main.layoutManager.connect(
            'monitors-changed', () => { this._build(); this._update(); });

        this._update();
    }

    _stop() {
        if (this._focusId) {
            global.display.disconnect(this._focusId);
            this._focusId = 0;
        }
        if (this._restackId) {
            global.display.disconnect(this._restackId);
            this._restackId = 0;
        }
        if (this._monitorsId) {
            Main.layoutManager.disconnect(this._monitorsId);
            this._monitorsId = 0;
        }
        this._destroyOverlays();
        this._active = false;
    }

    /** One overlay per monitor, sized to that monitor's work area. */
    _build() {
        this._destroyOverlays();
        for (const monitor of Main.layoutManager.monitors) {
            const overlay = new St.Widget({
                style_class: 'veronica-focus-dim',
                reactive: false,
                can_focus: false,
                opacity: 0,
                x: monitor.x,
                y: monitor.y,
                width: monitor.width,
                height: monitor.height,
            });
            overlay._veronicaMonitorIndex = monitor.index;
            global.window_group.add_child(overlay);
            this._overlays.push(overlay);
        }
    }

    _destroyOverlays() {
        for (const overlay of this._overlays) {
            overlay.remove_all_transitions();
            overlay.destroy();
        }
        this._overlays = [];
    }

    _update() {
        if (!this._active || this._overlays.length === 0)
            return;

        const focus = global.display.focus_window;
        const focusMonitor = focus ? focus.get_monitor() : -1;
        const target = Math.round(this._config.intensity * 255);
        const duration = Math.round(this._config.animation * 1000);

        for (const overlay of this._overlays) {
            const index = overlay._veronicaMonitorIndex;
            const bright = this._brightWindowFor(index, focus, focusMonitor);

            // Nothing to keep bright on this monitor: no window means an empty
            // desktop, and dimming a bare wallpaper is pointless noise.
            const wanted = bright ? target : 0;

            if (bright) {
                // Below the window that stays bright, above everything else.
                const actor = bright.get_compositor_private();
                if (actor && actor.get_parent() === global.window_group) {
                    global.window_group.set_child_below_sibling(overlay, actor);
                }
            }

            if (overlay.opacity === wanted)
                continue;
            overlay.remove_all_transitions();
            overlay.ease({
                opacity: wanted,
                duration,
                mode: Clutter.AnimationMode.EASE_OUT_QUAD,
            });
        }
    }

    /**
     * Which window stays bright on one monitor.
     *
     * `dimUnfocused` keeps only the focused window bright, so every other
     * monitor darkens entirely. `perScreenFront` keeps each monitor's own
     * frontmost normal window bright, which is what a multi-monitor setup with
     * reference material on the side wants.
     */
    _brightWindowFor(monitorIndex, focus, focusMonitor) {
        if (this._config.mode === 'dimUnfocused')
            return monitorIndex === focusMonitor ? focus : null;

        if (monitorIndex === focusMonitor && focus)
            return focus;

        // Front-to-back among windows actually on this monitor.
        const stack = global.display.sort_windows_by_stacking(
            global.get_window_actors().map(actor => actor.meta_window)
        );
        for (let i = stack.length - 1; i >= 0; i--) {
            const window = stack[i];
            if (this._isDimmable(window) && window.get_monitor() === monitorIndex)
                return window;
        }
        return null;
    }

    /**
     * Whether a window counts as content worth keeping bright.
     *
     * Docks, panels and notifications are chrome: raising one above the dim
     * would leave a bright strip and defeat the effect. Minimised windows are
     * not on screen at all.
     */
    _isDimmable(window) {
        if (!window || window.minimized)
            return false;
        const type = window.get_window_type();
        return type === Meta.WindowType.NORMAL || type === Meta.WindowType.DIALOG;
    }
}
