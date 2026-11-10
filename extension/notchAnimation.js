/* Animate the shell-painted material independently of full-size contents.
 * Pure orchestration: GPU actor transitions, with no per-frame JS or idle loop.
 */
export function installNotchAnimation(box, { buttonSize, enabled, modes, flags, onOpened }) {
    const surface = box?._border;
    const content = box?.bin;
    if (!surface || !content)
        return null;
    const originalOpen = box.open;
    const originalClose = box.close;
    let generation = 0;
    const stop = () => {
        generation++;
        box.remove_all_transitions();
        surface.remove_all_transitions();
        content.remove_all_transitions();
        return generation;
    };
    const reset = () => {
        box.opacity = 255;
        box.scale_x = box.scale_y = 1;
        box.translation_x = box.translation_y = 0;
        content.scale_x = content.scale_y = 1;
        content.translation_y = 0;
        content.opacity = 255;
        surface.scale_x = surface.scale_y = 1;
        surface.opacity = 255;
    };
    const compact = (motion, fade) => {
        if (!motion) return { x: 1, y: 1 };
        if (!fade) return { x: 0.96, y: 0.96 };
        const button = buttonSize();
        const width = box.width || box.get_preferred_width?.(-1)?.[1] || 580;
        const height = box.height || box.get_preferred_height?.(-1)?.[1] || 290;
        return {
            x: Math.max(0.12, Math.min(1, button.width / width)),
            y: Math.max(0.08, Math.min(1, button.height / height)),
        };
    };
    surface.set_pivot_point(0.5, 0);
    reset();

    box.open = (animate, onComplete) => {
        const wasVisible = box.visible;
        const token = stop();
        const animateNow = enabled() && animate !== flags.NONE;
        const motion = animateNow && Boolean(animate & flags.SLIDE);
        const fade = animateNow && Boolean(animate & flags.FADE);
        const duration = animateNow ? 280 : 0;
        if (!wasVisible) {
            reset();
            const size = compact(motion, fade);
            surface.scale_x = size.x;
            surface.scale_y = size.y;
            surface.opacity = fade ? 0 : 255;
            content.opacity = fade ? 0 : 255;
            content.translation_y = motion ? 4 : 0;
        }
        box._muteKeys = false;
        box._muteInput = animateNow;
        box.show();
        const finish = () => {
            if (token !== generation) return;
            box._muteInput = false;
            onComplete?.();
            onOpened?.();
        };
        if (!animateNow) {
            reset(); finish(); return;
        }
        // Only the material stretches; text and video stay at native scale.
        surface.ease({
            scale_x: 1, scale_y: 1, opacity: 255,
            duration, mode: modes.EASE_OUT_CUBIC,
            onComplete: finish,
        });
        content.ease({
            opacity: 255, translation_y: 0,
            delay: fade && content.opacity === 0 ? 75 : 0,
            duration: fade ? 180 : duration,
            mode: modes.EASE_OUT_CUBIC,
        });
    };

    box.close = (animate, onComplete) => {
        const token = stop();
        if (!box.visible) { onComplete?.(); return; }
        const animateNow = enabled() && animate !== flags.NONE;
        const motion = animateNow && Boolean(animate & flags.SLIDE);
        const fade = animateNow && Boolean(animate & flags.FADE);
        const size = compact(motion, fade);
        box._muteInput = box._muteKeys = true;
        const finish = () => {
            if (token !== generation) return;
            box.hide();
            reset();
            onComplete?.();
        };
        if (!animateNow) { finish(); return; }
        content.ease({
            opacity: fade ? 0 : 255, translation_y: motion ? 2 : 0,
            duration: 80, mode: modes.EASE_OUT_CUBIC,
        });
        surface.ease({
            scale_x: size.x, scale_y: size.y, opacity: fade ? 0 : 255,
            duration: 240, mode: modes.EASE_IN_OUT_CUBIC,
            onComplete: finish,
        });
    };
    return () => {
        stop();
        box.open = originalOpen;
        box.close = originalClose;
        reset();
    };
}
