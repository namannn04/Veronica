import Clutter from 'gi://Clutter';
import St from 'gi://St';

import { heading, meterRow, runJson, textRow, vbox } from './lib.js';
import { actionArgs, notchActivities, parseSurfaces, surfaceView } from './surfaceModel.js';

let current = [];
const listeners = new Set();

function announce() {
    for (const listener of listeners) {
        listener(current);
    }
}

export function setSurfaces(raw) {
    current = parseSurfaces(raw);
    announce();
}

export async function pullSurfaces(cancellable = null) {
    const listed = await runJson(['surfaces'], cancellable);
    current = parseSurfaces(listed);
    announce();
}

export function subscribeSurfaces(listener) {
    listeners.add(listener);
    listener(current);
    return () => listeners.delete(listener);
}

function note(text) {
    return new St.Label({ text, style_class: 'veronica-row-label veronica-dim', x_expand: true });
}

function rowActor(row) {
    if (row.kind === 'meter') {
        return meterRow(row.label, row.fraction, row.value, '');
    }
    if (row.kind === 'note') {
        return note(row.value);
    }
    return textRow(row.label, row.value);
}

export class SurfaceStrip {
    constructor(cancellable, presenter = () => null) {
        this._cancellable = cancellable;
        this._presenter = presenter;
        this.actor = vbox({ style_class: 'veronica-surface-strip', x_expand: true, visible: false });
        this._unsubscribe = subscribeSurfaces(surfaces => this._render(surfaces));
    }

    refresh() {
        this._render(current);
    }

    _render(surfaces) {
        if (!this.actor) {
            return;
        }
        this.actor.destroy_all_children();
        const activities = notchActivities(surfaces);
        this.actor.visible = activities.length > 0;
        for (const surface of activities) {
            this.actor.add_child(this._card(surfaceView(surface, this._presenter())));
        }
    }

    _card(view) {
        const card = vbox({
            style_class: view.hidden
                ? 'veronica-card veronica-surface presenter-private'
                : 'veronica-card veronica-surface',
            x_expand: true,
        });
        if (view.heading) {
            card.add_child(heading(view.heading));
        }
        for (const row of view.rows) {
            card.add_child(rowActor(row));
        }
        if (view.actions.length > 0) {
            const actions = new St.BoxLayout({ style_class: 'veronica-surface-actions', x_expand: true });
            for (const action of view.actions) {
                const button = new St.Button({
                    label: action.label,
                    style_class: 'veronica-action-tile veronica-surface-action',
                    can_focus: true,
                    track_hover: true,
                    y_align: Clutter.ActorAlign.CENTER,
                });
                button.connect('clicked', () => {
                    runJson(actionArgs(action), this._cancellable).catch(() => {});
                });
                actions.add_child(button);
            }
            card.add_child(actions);
        }
        return card;
    }

    destroy() {
        this._unsubscribe?.();
        this._unsubscribe = null;
        this.actor?.destroy();
        this.actor = null;
    }
}
