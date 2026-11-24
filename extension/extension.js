import Gio from 'gi://Gio';

import { Extension } from 'resource:///org/gnome/shell/extensions/extension.js';
import { ActionBridge } from './actionBridge.js';
import { AttentionTracker } from './attention.js';
import { ClipboardWatcher } from './clipboard.js';
import { booleanSetting, FeatureSwitch } from './featureSwitch.js';
import { FocusDim } from './focusDim.js';
import { KeyboardCleaner } from './keyboardCleaner.js';
import { KeystrokeHighlight } from './keystrokeHighlight.js';
import { findCli } from './lib.js';
import { PanelReplacement } from './panelReplacement.js';
import { SettingsWatcher } from './settings.js';
import { StandaloneStats } from './systemStats.js';

export default class VeronicaExtension extends Extension {
    enable() {
        this._cancellable = new Gio.Cancellable();

        this._settings = new SettingsWatcher();
        this._settings.enable();

        this._focusDim = new FocusDim(this._settings);
        this._focusDim.enable();

        this._attention = new AttentionTracker(this._settings);
        this._attention.enable();

        this._keystrokes = new KeystrokeHighlight(this._settings);
        this._keystrokes.enable();

        this._clipboard = new ClipboardWatcher();
        this._keyboardCleaner = new KeyboardCleaner();
        this._panelReplacement = new PanelReplacement();
        this._clipboardSwitch = new FeatureSwitch(
            'clipboardEnabled',
            true,
            () => {
                if (this._clipboard?.enable()) {
                    console.debug('veronica: watching the clipboard');
                }
            },
            () => this._clipboard?.disable()
        );
        this._notchSwitch = new FeatureSwitch(
            'notchShelfEnabled',
            true,
            () =>
                this._panelReplacement?.enable(
                    this._clipboard,
                    this._cancellable,
                    this._settings,
                    this._keyboardCleaner
                ),
            () => this._panelReplacement?.disable()
        );
        this._standaloneStats = new StandaloneStats(this._settings);
        this._lifecycleUnsubscribe = this._settings.subscribe(values => {
            this._clipboardSwitch?.update(values);
            this._notchSwitch?.update(values);
            if (booleanSetting(values, 'notchShelfEnabled', true)) {
                this._standaloneStats?.hide();
            } else {
                this._standaloneStats?.show();
            }
        });
        this._actionBridge = new ActionBridge({
            cleanKeys: () => this._keyboardCleaner?.start(),
            pickColor: () => {
                if (!this._panelReplacement?.pickColor()) {
                    throw new Error('the color picker extension is disabled');
                }
            },
            showClipboard: () => {
                if (!this._panelReplacement?.showClipboard()) {
                    throw new Error('the clipboard shelf action is disabled');
                }
            },
            writeClipboard: text => this._clipboard?.write(text, false),
        });
        this._actionBridge.enable();
        console.debug(`veronica: compact Edith notch enabled; vr at "${findCli() || 'not found'}"`);
    }

    disable() {
        this._attention?.disable();
        this._attention = null;
        this._keystrokes?.disable();
        this._keystrokes = null;
        this._actionBridge?.disable();
        this._actionBridge = null;

        this._lifecycleUnsubscribe?.();
        this._lifecycleUnsubscribe = null;
        this._notchSwitch?.disable();
        this._notchSwitch = null;
        this._standaloneStats?.hide();
        this._standaloneStats = null;
        this._clipboardSwitch?.disable();
        this._clipboardSwitch = null;

        this._keyboardCleaner?.stop();
        this._keyboardCleaner = null;

        this._cancellable?.cancel();
        this._cancellable = null;

        this._panelReplacement = null;

        this._clipboard = null;

        this._focusDim?.disable();
        this._focusDim = null;

        this._settings?.disable();
        this._settings = null;
    }
}
