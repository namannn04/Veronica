/* Paint-only bridge. GNOME still owns notification queue, actions and policy. */
export function installBannerPresentation(tray, { createMaterial, clockSize, enabled, mode, theme = 'dark' }) {
    const bin = tray?._bannerBin;
    if (!bin?.connect || typeof bin.ease !== 'function') return null;
    const originalEase = bin.ease;
    const descriptor = Object.getOwnPropertyDescriptor(bin, 'ease');
    let current = null, material = null, started = false, alive = true;
    let appearance = theme;
    const clear = () => {
        current?.remove_style_class_name('veronica-notch-native-banner');
        current?.remove_style_class_name('veronica-banner-transparent');
        material?.remove_all_transitions(); material?.destroy();
        current = material = null; started = false;
    };
    const attach = child => {
        if (!alive || child === material || !child?.has_style_class_name?.('notification-banner')) return;
        clear(); current = child;
        child.add_style_class_name('veronica-notch-native-banner');
        try {
            material = createMaterial(bin, child);
            material?.set_style_class_name(`veronica-banner-material veronica-theme-${appearance}`);
            if (material) child.add_style_class_name('veronica-banner-transparent');
        } catch { material = null; } // Styled native banner remains the fallback.
    };
    let added = 0, removed = 0;
    try {
        added = bin.connect('child-added', (_actor, child) => attach(child));
        removed = bin.connect('child-removed', (_actor, child) => { if (child === current) clear(); });
    } catch {
        if (added) bin.disconnect(added);
        return null;
    }
    const compact = () => {
        const clock = clockSize();
        return { x: Math.max(0.12, Math.min(1, clock.width / (current?.width || 520))), y: Math.max(0.12, Math.min(1, clock.height / (current?.height || 120))) };
    };
    const wrapper = function (params) {
        if (!alive || !current) return originalEase.call(this, params);
        const values = { ...params };
        const opening = params.opacity === 255 || params.y === 0;
        const closing = params.opacity === 0 || (typeof params.y === 'number' && params.y < 0);
        const motion = enabled() && params.duration !== 0;
        const duration = motion ? (closing ? 180 : 280) : 0;
        // Keep text/action widgets at native scale; only the separate material morphs.
        this.scale_x = this.scale_y = 1;
        delete values.scale_x; delete values.scale_y;
        this.y = 0;
        if ('y' in values) { values.y = 0; values.opacity = closing ? 0 : 255; }
        values.duration = duration; values.mode = mode;
        if (material && (opening || closing)) {
            material.remove_all_transitions();
            const size = compact();
            if (!started && opening) {
                material.scale_x = motion ? size.x : 1;
                material.scale_y = motion ? size.y : 1;
                started = true;
            }
            const target = { scale_x: closing ? size.x : 1, scale_y: closing ? size.y : 1 };
            if (motion) material.ease({ ...target, duration, mode });
            else { material.scale_x = target.scale_x; material.scale_y = target.scale_y; }
        }
        // Preserve onComplete/onStopped verbatim: dropping them stalls the shell queue.
        return originalEase.call(this, values);
    };
    bin.ease = wrapper;
    for (const child of bin.get_children?.() ?? []) attach(child);
    return {
        setTheme(next) { appearance = next; material?.set_style_class_name(`veronica-banner-material veronica-theme-${appearance}`); },
        destroy() {
            alive = false;
            if (bin.ease === wrapper) {
                if (descriptor) Object.defineProperty(bin, 'ease', descriptor);
                else delete bin.ease;
            }
            bin.disconnect(added); bin.disconnect(removed); clear();
            bin.scale_x = bin.scale_y = 1;
        },
    };
}
