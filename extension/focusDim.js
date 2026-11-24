import Clutter from 'gi://Clutter';
import Meta from 'gi://Meta';
import St from 'gi://St';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';

import {
    clampAnimationSeconds,
    clampIntensity,
    overlayPlacement,
    parseDisplayMode,
} from './focusDimMath.js';

export class FocusDim {
    constructor(settings) {
        this._settings = settings;
        this._unsubscribe = null;
        this._overlays = [];
        this._active = false;
        this._focusId = 0;
        this._monitorsId = 0;
        this._restackId = 0;
        this._workspaceId = 0;
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
            intensity: clampIntensity(values.focusDimIntensity),
            animation: clampAnimationSeconds(values.focusDimAnimationDuration),
            mode: parseDisplayMode(values.focusDimOtherDisplaysMode),
        };
        const wanted = values.focusDimEnabled === true;
        if (wanted !== this._active) {
            if (wanted) {
                this._start();
            }
            else {
                this._stop();
            }
            return;
        }
        if (this._active) {
            this._update();
        }
    }

    _start() {
        if (this._active) {
            return;
        }
        this._active = true;
        this._build();

        this._focusId = global.display.connect(
            'notify::focus-window', () => this._update());
        this._restackId = global.display.connect('restacked', () => this._update());
        this._monitorsId = Main.layoutManager.connect(
            'monitors-changed', () => { this._build(); this._update(); });
        this._workspaceId = global.workspace_manager.connect(
            'active-workspace-changed', () => this._update());

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
        if (this._workspaceId) {
            global.workspace_manager.disconnect(this._workspaceId);
            this._workspaceId = 0;
        }
        this._destroyOverlays();
        this._active = false;
    }

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
        if (!this._active || this._overlays.length === 0) {
            return;
        }

        const focus = global.display.focus_window;
        const focusMonitor = focus ? focus.get_monitor() : -1;
        const target = Math.round(this._config.intensity * 255);
        const duration = Math.round(this._config.animation * 1000);

        for (const overlay of this._overlays) {
            const index = overlay._veronicaMonitorIndex;
            const bright = this._brightWindowFor(index, focus, focusMonitor);
            const placement = overlayPlacement(this._config.mode, Boolean(bright), Boolean(focus));
            const wanted = placement === 'off' ? 0 : target;

            if (placement === 'below') {
                const actor = bright.get_compositor_private();
                if (actor && actor.get_parent() === global.window_group) {
                    global.window_group.set_child_below_sibling(overlay, actor);
                }
            } else if (placement === 'top') {
                global.window_group.set_child_above_sibling(overlay, null);
            }

            if (overlay.opacity === wanted) {
                continue;
            }
            overlay.remove_all_transitions();
            overlay.ease({
                opacity: wanted,
                duration,
                mode: Clutter.AnimationMode.EASE_OUT_QUAD,
            });
        }
    }

    _brightWindowFor(monitorIndex, focus, focusMonitor) {
        if (this._config.mode === 'dimUnfocused') {
            return monitorIndex === focusMonitor ? focus : null;
        }

        if (monitorIndex === focusMonitor && focus) {
            return focus;
        }

        const stack = global.display.sort_windows_by_stacking(
            global.get_window_actors().map(actor => actor.meta_window)
        );
        for (let i = stack.length - 1; i >= 0; i--) {
            const window = stack[i];
            if (this._isDimmable(window) && window.get_monitor() === monitorIndex) {
                return window;
            }
        }
        return null;
    }

    _isDimmable(window) {
        if (!window || window.minimized) {
            return false;
        }
        const workspace = global.workspace_manager.get_active_workspace();
        if (!window.is_on_all_workspaces() && !window.located_on_workspace(workspace)) {
            return false;
        }
        const type = window.get_window_type();
        return type === Meta.WindowType.NORMAL || type === Meta.WindowType.DIALOG;
    }
}
