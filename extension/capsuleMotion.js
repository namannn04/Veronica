/* Native actor transitions only: no animation timer or per-frame JS. */
export function installCapsule(button, { active, enabled, mode }) {
    const label = button._veronicaLabel;
    let natural = null;
    label.visible = true;
    label.clip_to_allocation = true;
    const sync = () => {
        if (natural === null) natural = Math.min(96, label.get_preferred_width(-1)[1]);
        const expanded = active() || button.hover || button.has_key_focus();
        label.remove_all_transitions();
        const values = { width: expanded ? natural : 0, opacity: expanded ? 255 : 0 };
        if (enabled() && button.mapped) label.ease({ ...values, duration: 180, mode });
        else { label.width = values.width; label.opacity = values.opacity; }
    };
    button.connect('notify::hover', sync);
    button.connect('key-focus-in', sync);
    button.connect('key-focus-out', sync);
    button.connect('destroy', () => label.remove_all_transitions());
    sync();
    return sync;
}
