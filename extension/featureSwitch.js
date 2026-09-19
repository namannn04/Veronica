/* Live on/off lifecycle for shell-owned extension features.
 *
 * Free of gi:// imports so the transition rules can be tested under Node. The
 * GNOME entry point supplies the real start and stop functions; this module
 * only makes them idempotent and applies the catalogue's default when a key is
 * absent from a first-launch settings file.
 */

export function booleanSetting(values, key, fallback) {
    const value = values?.[key];
    return typeof value === 'boolean' ? value : fallback;
}

export class FeatureSwitch {
    constructor(key, fallback, start, stop) {
        this._key = key;
        this._fallback = fallback;
        this._start = start;
        this._stop = stop;
        this._wanted = false;
    }

    update(values) {
        const wanted = booleanSetting(values, this._key, this._fallback);
        if (wanted === this._wanted)
            return;
        this._wanted = wanted;
        if (wanted)
            this._start();
        else
            this._stop();
    }

    disable() {
        if (this._wanted)
            this._stop();
        this._wanted = false;
    }
}
