/* A percentage ring, drawn with Cairo.
 *
 * Edith's usage rings are the most recognisable piece of its notch, so this
 * reproduces them directly rather than approximating with a bar: a stroked
 * circle, the covered arc drawn over it in a status colour, the percentage in
 * the centre.
 */

import Clutter from 'gi://Clutter';
import GObject from 'gi://GObject';
import St from 'gi://St';

const TRACK_RGBA = [1, 1, 1, 0.12];
const STROKE_WIDTH = 4.5;

/** Status colours, matching the same bands used elsewhere in the extension. */
const BAND_RGB = {
    good: [0x6a / 255, 0x8d / 255, 0x73 / 255],
    warning: [0xfa / 255, 0xb2 / 255, 0x19 / 255],
    critical: [0xd0 / 255, 0x3b / 255, 0x3b / 255],
};

export const RingGauge = GObject.registerClass(
class RingGauge extends St.Widget {
    _init(size = 52) {
        super._init({
            layout_manager: new Clutter.BinLayout(),
            width: size,
            height: size,
        });
        this._size = size;
        this._percent = 0;
        this._band = 'good';

        this._area = new St.DrawingArea({ width: size, height: size });
        this._area.connect('repaint', area => this._draw(area));
        this.add_child(this._area);

        this._label = new St.Label({
            style_class: 'veronica-ring-value',
            x_align: Clutter.ActorAlign.CENTER,
            y_align: Clutter.ActorAlign.CENTER,
        });
        this.add_child(this._label);
    }

    /** `percent` is 0-100, `band` one of good/warning/critical. */
    setValue(percent, band) {
        this._percent = Math.max(0, Math.min(100, percent));
        this._band = BAND_RGB[band] ? band : 'good';
        this._label.text = `${Math.round(this._percent)}%`;
        this._area.queue_repaint();
    }

    _draw(area) {
        const [width, height] = area.get_surface_size();
        const cr = area.get_context();
        const radius = Math.min(width, height) / 2 - STROKE_WIDTH / 2 - 1;
        const cx = width / 2;
        const cy = height / 2;
        // Twelve o'clock, matching the app's own rings.
        const start = -Math.PI / 2;
        const end = start + (this._percent / 100) * 2 * Math.PI;

        cr.setLineWidth(STROKE_WIDTH);
        cr.setLineCap(0); // Cairo.LineCap.BUTT — the track has square ends.
        cr.setSourceRGBA(...TRACK_RGBA);
        cr.arc(cx, cy, radius, 0, 2 * Math.PI);
        cr.stroke();

        if (this._percent > 0) {
            const [r, g, b] = BAND_RGB[this._band];
            cr.setLineCap(1); // Cairo.LineCap.ROUND — the fill's ends are round.
            cr.setSourceRGBA(r, g, b, 1);
            cr.arc(cx, cy, radius, start, end);
            cr.stroke();
        }
        cr.$dispose();
    }
});
