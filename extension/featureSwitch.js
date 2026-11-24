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
        if (wanted === this._wanted) {
            return;
        }
        this._wanted = wanted;
        if (wanted) {
            this._start();
        } else {
            this._stop();
        }
    }

    disable() {
        if (this._wanted) {
            this._stop();
        }
        this._wanted = false;
    }
}
