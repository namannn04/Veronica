import Clutter from 'gi://Clutter';
import St from 'gi://St';
import { installBannerPresentation } from './bannerPresentation.js';

export function attachNotchBanners(tray, button, theme) {
    return installBannerPresentation(tray, {
        theme,
        clockSize: () => ({ width: button.width || 148, height: button.height || 30 }),
        enabled: () => St.Settings.get().enable_animations,
        mode: Clutter.AnimationMode.EASE_OUT_CUBIC,
        createMaterial(bin, banner) {
            // St.Bin on older shells only supports its one native child.
            if (bin instanceof St.Bin) return null;
            const actor = new St.Widget({ reactive: false, x_align: Clutter.ActorAlign.START, y_align: Clutter.ActorAlign.START });
            actor.set_pivot_point(0.5, 0);
            actor.add_constraint(new Clutter.BindConstraint({ source: banner, coordinate: Clutter.BindCoordinate.WIDTH }));
            actor.add_constraint(new Clutter.BindConstraint({ source: banner, coordinate: Clutter.BindCoordinate.HEIGHT }));
            bin.insert_child_below(actor, banner);
            return actor;
        },
    });
}
