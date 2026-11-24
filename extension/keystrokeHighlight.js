import Clutter from 'gi://Clutter';
import GLib from 'gi://GLib';
import St from 'gi://St';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';

import { clampDuration, KeystrokeQueue, labelsFor, MAXIMUM_VISIBLE, parsePosition } from './keystrokeLabels.js';

const EDGE_MARGIN = 64;
const FADE_MS = 140;

export class KeystrokeHighlight {
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

    get running() {
        return this._running;
    }

    _apply(values) {
        this._config = {
            duration: clampDuration(values.keystrokeHighlightDuration),
            position: parsePosition(values.keystrokeHighlightPosition),
        };
        const wanted = values.keystrokeHighlightEnabled === true && values.keystrokeHighlightActive === true;
        if (wanted !== this._running) {
            if (wanted) {
                this._start();
            } else {
                this._stop();
            }
            return;
        }
        if (this._running) {
            this._position();
        }
    }

    _start() {
        if (this._running) {
            return;
        }
        this._running = true;
        this._build();

        this._eventId = global.stage.connect('captured-event', (_actor, event) => this._onEvent(event));
        this._monitorsId = Main.layoutManager.connect('monitors-changed', () => this._position());

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
        this._queue.clear();
        for (const actor of this._actors.values()) {
            actor.destroy();
        }
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

    _position() {
        if (!this._container) {
            return;
        }
        const monitor = Main.layoutManager.primaryMonitor;
        if (!monitor) {
            return;
        }
        const work = Main.layoutManager.getWorkAreaForMonitor(monitor.index);
        const [, height] = this._container.get_preferred_height(-1);
        const [, width] = this._container.get_preferred_width(-1);
        this._container.set_position(
            Math.round(work.x + (work.width - width) / 2),
            this._config.position === 'top' ? work.y + EDGE_MARGIN : work.y + work.height - height - EDGE_MARGIN
        );
    }

    _onEvent(event) {
        try {
            if (event.type() === Clutter.EventType.KEY_PRESS) {
                this._record(event);
            }
        } catch (error) {
            console.debug(`veronica: keystroke highlight failed to record a press: ${error}`);
        }
        return Clutter.EVENT_PROPAGATE;
    }

    _record(event) {
        if (Main.modalCount > 0) {
            return;
        }

        const keyval = event.get_key_symbol();
        const unicode = event.get_key_unicode?.() ?? Clutter.keyval_to_unicode(keyval);
        const state = event.get_state();

        const labels = labelsFor(keyval, unicode, state);
        const entry = this._queue.append(labels, GLib.get_monotonic_time() / 1000, this._config.duration);
        if (!entry) {
            return;
        }

        this._addCap(entry);
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
            cap.add_child(
                new St.Label({
                    style_class: 'veronica-keystroke-key',
                    text: label,
                    y_align: Clutter.ActorAlign.CENTER,
                })
            );
        }
        this._container.add_child(cap);
        this._actors.set(entry.id, cap);
        cap.ease({ opacity: 255, duration: FADE_MS, mode: Clutter.AnimationMode.EASE_OUT_QUAD });
    }

    _reconcile() {
        const live = new Set(this._queue.entries.map(entry => entry.id));
        for (const [id, actor] of this._actors) {
            if (!live.has(id)) {
                this._fadeOut(id, actor);
            }
        }
    }

    _sweep() {
        if (!this._running) {
            return;
        }
        const expired = this._queue.removeExpired(GLib.get_monotonic_time() / 1000);
        for (const entry of expired) {
            const actor = this._actors.get(entry.id);
            if (actor) {
                this._fadeOut(entry.id, actor);
            }
        }
        if (expired.length > 0) {
            this._position();
        }
    }

    _fadeOut(id, actor) {
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
