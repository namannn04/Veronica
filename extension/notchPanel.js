/* Edith's notch content, adapted to live inside GNOME's native clock popup.
 *
 * The shell keeps owning notifications. This widget owns the 580px-style tab
 * strip and cards Edith places below its hardware notch: Home, Notifications,
 * Files, Clipboard and Camera. Every visible control has a Linux-native action.
 */

import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Pango from 'gi://Pango';
import St from 'gi://St';
import * as Calendar from 'resource:///org/gnome/shell/ui/calendar.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';

import { CameraPreview } from './cameraPreview.js';
import { entryText, recentEntries } from './clipboard.js';
import { launchApp, runJson } from './lib.js';
import { NowPlayingCard } from './nowPlaying.js';
import { PowerInhibitors } from './inhibitors.js';
import { UsageCard } from './usageCard.js';

const TABS = [
    ['home', 'user-home-symbolic', 'Home'],
    ['notifications', 'preferences-system-notifications-symbolic', 'Notifications'],
    ['files', 'folder-symbolic', 'Files'],
    ['clipboard', 'edit-paste-symbolic', 'Clipboard'],
    ['camera', 'camera-photo-symbolic', 'Camera'],
];

const THEMES = new Set(['light', 'dark', 'midnight', 'aubergine', 'forest']);

export class NotchPanel {
    constructor(clipboardWatcher, cancellable, closeMenu = null, onThemeChanged = null) {
        this._clipboardWatcher = clipboardWatcher;
        this._cancellable = cancellable;
        this._closeMenu = closeMenu;
        this._onThemeChanged = onThemeChanged;
        this._activeTab = 'home';
        this._tabButtons = new Map();
        this._panels = new Map();
        this._power = new PowerInhibitors(key => this._powerInhibitorFailed(key));

        this.actor = new St.BoxLayout({
            orientation: Clutter.Orientation.VERTICAL,
            style_class: 'veronica-shelf',
            x_expand: true,
        });
        this.actor.add_child(this._header());

        this._stack = new St.Widget({
            style_class: 'veronica-shelf-stack',
            layout_manager: new Clutter.BinLayout(),
            x_expand: true,
        });
        this.actor.add_child(this._stack);

        this._addPanel('home', this._homePanel());
        this._addPanel('notifications', this._notificationsPanel());
        this._addPanel('files', this._filesPanel());
        this._addPanel('clipboard', this._clipboardPanel());
        this._addPanel('camera', this._cameraPanel());
        this._selectTab('home');
        this._watchSettings();
    }

    get isLive() {
        return this.actor !== null;
    }

    showTab(id) {
        if (this._panels.has(id))
            this._selectTab(id);
    }

    _header() {
        const header = new St.BoxLayout({ style_class: 'veronica-shelf-header' });
        for (const [id, icon, title] of TABS) {
            const button = new St.Button({
                style_class: 'veronica-shelf-tab',
                can_focus: true,
                accessible_name: title,
            });
            const content = new St.BoxLayout({ style_class: 'veronica-shelf-tab-content' });
            content.add_child(new St.Icon({ icon_name: icon, style_class: 'veronica-shelf-tab-icon' }));
            const label = new St.Label({
                text: title,
                style_class: 'veronica-shelf-tab-label',
                y_align: Clutter.ActorAlign.CENTER,
            });
            content.add_child(label);
            button.set_child(content);
            button.connect('clicked', () => this._selectTab(id));
            button._veronicaLabel = label;
            this._tabButtons.set(id, button);
            header.add_child(button);
        }

        const spacer = new St.Widget({ x_expand: true });
        header.add_child(spacer);
        const settings = new St.Button({
            style_class: 'veronica-shelf-settings',
            can_focus: true,
            accessible_name: 'Open Veronica settings',
            child: new St.Icon({ icon_name: 'emblem-system-symbolic' }),
        });
        settings.connect('clicked', () => {
            this._closeMenu?.();
            launchApp();
        });
        header.add_child(settings);
        return header;
    }

    _addPanel(id, actor) {
        actor.visible = false;
        this._panels.set(id, actor);
        this._stack.add_child(actor);
    }

    _selectTab(id) {
        if (this._activeTab === 'camera' && id !== 'camera')
            this._stopCameraPreview();
        this._activeTab = id;
        for (const [tabId, button] of this._tabButtons) {
            const active = tabId === id;
            button.set_style_class_name(
                active ? 'veronica-shelf-tab active' : 'veronica-shelf-tab'
            );
            button._veronicaLabel.visible = active;
        }
        for (const [panelId, panel] of this._panels)
            panel.visible = panelId === id;
        if (id === 'clipboard')
            this._refreshClipboard().catch(() => {});
        if (id === 'files')
            this._refreshFiles().catch(() => {});
    }

    _homePanel() {
        const panel = new St.BoxLayout({
            orientation: Clutter.Orientation.VERTICAL,
            style_class: 'veronica-shelf-panel veronica-shelf-home',
            x_expand: true,
        });

        const glance = new St.BoxLayout({ style_class: 'veronica-glance-row', x_expand: true });
        const musicWrap = new St.Widget({
            style_class: 'veronica-home-card-wrap',
            layout_manager: new Clutter.BinLayout(),
            x_expand: true,
        });
        this._nowPlaying = new NowPlayingCard();
        musicWrap.add_child(this._nowPlaying.actor);
        this._emptyMusic = new St.BoxLayout({
            orientation: Clutter.Orientation.VERTICAL,
            style_class: 'veronica-card veronica-empty-music',
        });
        this._emptyMusic.add_child(new St.Widget({ y_expand: true }));
        this._emptyMusic.add_child(new St.Icon({
            icon_name: 'emblem-music-symbolic',
            style_class: 'veronica-empty-music-icon',
            x_align: Clutter.ActorAlign.CENTER,
        }));
        this._emptyMusic.add_child(new St.Label({
            text: 'Nothing playing',
            style_class: 'veronica-empty-music-label',
            x_align: Clutter.ActorAlign.CENTER,
        }));
        this._emptyMusic.add_child(new St.Widget({ y_expand: true }));
        musicWrap.add_child(this._emptyMusic);
        glance.add_child(musicWrap);

        this._usage = new UsageCard();
        glance.add_child(this._usage.actor);
        panel.add_child(glance);

        const actions = new St.BoxLayout({ style_class: 'veronica-actions-row' });
        actions.add_child(this._actionTile(
            'input-keyboard-symbolic', 'Clean keys', () => this._startCleanKeys()
        ));
        this._keepAwake = this._toggleTile(
            'weather-clear-night-symbolic', 'Keep awake', 'preventSleep',
            active => this._power.set('preventSleep', active)
        );
        this._lidAwake = this._toggleTile(
            'computer-laptop-symbolic', 'Lid awake', 'lidAwakeEnabled',
            active => this._power.set('lidAwakeEnabled', active)
        );
        actions.add_child(this._keepAwake);
        actions.add_child(this._lidAwake);
        this._presenter = this._toggleTile(
            'avatar-default-symbolic', 'Presenter', 'presenterMode',
            active => this._applyPresenter(active)
        );
        actions.add_child(this._presenter);
        actions.add_child(this._actionTile(
            'color-select-symbolic', 'Pick color', () => this._pickColor()
        ));
        panel.add_child(actions);
        return panel;
    }

    _notificationsPanel() {
        const panel = new St.BoxLayout({
            orientation: Clutter.Orientation.VERTICAL,
            style_class: 'veronica-shelf-panel veronica-notifications-panel',
            x_expand: true,
        });
        this._messageList = new Calendar.CalendarMessageList();
        this._messageList.add_style_class_name('veronica-shelf-messages');
        panel.add_child(this._messageList);
        return panel;
    }

    _filesPanel() {
        const panel = new St.BoxLayout({
            orientation: Clutter.Orientation.VERTICAL,
            style_class: 'veronica-shelf-panel veronica-files-panel',
            x_expand: true,
        });
        const toolbar = new St.BoxLayout({ style_class: 'veronica-files-toolbar' });
        toolbar.add_child(new St.Label({
            text: 'File Shelf',
            style_class: 'veronica-files-title',
            y_align: Clutter.ActorAlign.CENTER,
        }));
        toolbar.add_child(new St.Widget({ x_expand: true }));
        toolbar.add_child(this._smallButton('list-add-symbolic', 'Add files', () => this._chooseFiles()));
        toolbar.add_child(this._smallButton('edit-clear-all-symbolic', 'Clear shelf', () => {
            this._saveFiles([]).catch(() => {});
        }));
        panel.add_child(toolbar);

        const scroll = new St.ScrollView({
            style_class: 'veronica-files-scroll',
            overlay_scrollbars: true,
            x_expand: true,
            y_expand: true,
        });
        this._fileRows = new St.BoxLayout({
            orientation: Clutter.Orientation.VERTICAL,
            style_class: 'veronica-file-rows',
            x_expand: true,
        });
        scroll.set_child(this._fileRows);
        panel.add_child(scroll);
        return panel;
    }

    _cameraPanel() {
        const panel = new St.BoxLayout({
            orientation: Clutter.Orientation.VERTICAL,
            style_class: 'veronica-shelf-panel veronica-camera-panel',
            x_expand: true,
        });

        const frame = new St.Widget({
            style_class: 'veronica-camera-preview-frame',
            layout_manager: new Clutter.BinLayout(),
            x_align: Clutter.ActorAlign.CENTER,
            clip_to_allocation: true,
        });
        this._cameraSurface = new St.Widget({
            style_class: 'veronica-camera-preview-surface',
            x_expand: true,
            y_expand: true,
        });
        frame.add_child(this._cameraSurface);

        this._cameraPlaceholder = new St.BoxLayout({
            orientation: Clutter.Orientation.VERTICAL,
            style_class: 'veronica-camera-placeholder',
            x_align: Clutter.ActorAlign.CENTER,
            y_align: Clutter.ActorAlign.CENTER,
        });
        this._cameraPlaceholder.add_child(new St.Icon({
            icon_name: 'camera-photo-symbolic',
            style_class: 'veronica-camera-icon',
            x_align: Clutter.ActorAlign.CENTER,
        }));
        this._cameraTitle = new St.Label({
            text: 'Camera preview',
            style_class: 'veronica-camera-title',
            x_align: Clutter.ActorAlign.CENTER,
        });
        this._cameraPlaceholder.add_child(this._cameraTitle);
        this._cameraDetail = new St.Label({
            text: 'Your camera stays on only while this preview is open.',
            style_class: 'veronica-camera-detail',
            x_align: Clutter.ActorAlign.CENTER,
        });
        this._cameraPlaceholder.add_child(this._cameraDetail);
        frame.add_child(this._cameraPlaceholder);
        panel.add_child(frame);

        this._cameraButton = new St.Button({
            style_class: 'veronica-camera-preview-button',
            label: 'Start Preview',
            can_focus: true,
            x_align: Clutter.ActorAlign.CENTER,
        });
        this._cameraButton.connect('clicked', () => this._toggleCameraPreview());
        panel.add_child(this._cameraButton);

        this._cameraPreview = new CameraPreview(
            this._cameraSurface,
            () => {
                if (this._cameraPlaceholder)
                    this._cameraPlaceholder.visible = false;
            },
            error => this._cameraPreviewFailed(error)
        );
        return panel;
    }

    _clipboardPanel() {
        const panel = new St.BoxLayout({
            orientation: Clutter.Orientation.VERTICAL,
            style_class: 'veronica-shelf-panel veronica-clipboard-panel',
            x_expand: true,
        });
        const scroll = new St.ScrollView({
            style_class: 'veronica-clipboard-scroll',
            overlay_scrollbars: true,
            x_expand: true,
            y_expand: true,
        });
        this._clipboardRows = new St.BoxLayout({
            orientation: Clutter.Orientation.VERTICAL,
            style_class: 'veronica-clipboard-rows',
            x_expand: true,
        });
        scroll.set_child(this._clipboardRows);
        panel.add_child(scroll);
        return panel;
    }

    _statusPanel(iconName, title, detail) {
        const panel = new St.BoxLayout({
            orientation: Clutter.Orientation.VERTICAL,
            style_class: 'veronica-shelf-panel veronica-status-panel',
            x_expand: true,
        });
        panel.add_child(new St.Icon({
            icon_name: iconName,
            style_class: 'veronica-status-panel-icon',
            x_align: Clutter.ActorAlign.CENTER,
        }));
        panel.add_child(new St.Label({
            text: title,
            style_class: 'veronica-status-panel-title',
            x_align: Clutter.ActorAlign.CENTER,
        }));
        const detailLabel = new St.Label({
            text: detail,
            style_class: 'veronica-status-panel-detail',
            x_align: Clutter.ActorAlign.CENTER,
        });
        detailLabel.clutter_text.line_wrap = true;
        detailLabel.clutter_text.line_wrap_mode = Pango.WrapMode.WORD_CHAR;
        detailLabel.clutter_text.justification = Pango.Alignment.CENTER;
        panel.add_child(detailLabel);
        return panel;
    }

    _actionTile(iconName, label, onClicked) {
        const button = new St.Button({
            style_class: 'veronica-action-tile',
            can_focus: true,
            x_expand: true,
        });
        const content = new St.BoxLayout({ style_class: 'veronica-action-content' });
        content.add_child(new St.Icon({ icon_name: iconName, style_class: 'veronica-action-icon' }));
        content.add_child(new St.Label({
            text: label,
            style_class: 'veronica-action-label',
            y_align: Clutter.ActorAlign.CENTER,
        }));
        button.set_child(content);
        button.connect('clicked', onClicked);
        return button;
    }

    _smallButton(iconName, accessibleName, onClicked) {
        const button = new St.Button({
            style_class: 'veronica-small-button',
            can_focus: true,
            accessible_name: accessibleName,
            child: new St.Icon({ icon_name: iconName }),
        });
        button.connect('clicked', onClicked);
        return button;
    }

    _toggleTile(iconName, label, key, onChanged = null) {
        const button = this._actionTile(iconName, label, () => {
            const next = !button._veronicaActive;
            this._setToggle(button, next);
            if (onChanged?.(next) === false) {
                this._setToggle(button, !next);
                Main.notify('Veronica', `${label} could not acquire a system inhibitor.`);
                return;
            }
            runJson(['config', 'set', key, String(next)], this._cancellable)
                .then(result => {
                    if (!result) {
                        this._setToggle(button, !next);
                        onChanged?.(!next);
                    }
                })
                .catch(() => {
                    this._setToggle(button, !next);
                    onChanged?.(!next);
                });
        });
        button._veronicaKey = key;
        button._veronicaActive = false;
        return button;
    }

    _powerInhibitorFailed(key) {
        const button = key === 'preventSleep' ? this._keepAwake : this._lidAwake;
        if (button)
            this._setToggle(button, false);
        runJson(['config', 'set', key, 'false'], this._cancellable).catch(() => {});
        Main.notify(
            key === 'preventSleep' ? 'Keep Awake stopped' : 'Lid Awake stopped',
            'systemd-logind did not keep the inhibitor active.'
        );
    }

    _setToggle(button, active) {
        button._veronicaActive = active;
        button.set_style_class_name(
            active ? 'veronica-action-tile active' : 'veronica-action-tile'
        );
    }

    async _readFiles() {
        const value = await runJson(['config', 'get', 'notchShelfFiles'], this._cancellable);
        return Array.isArray(value)
            ? value.filter(path => typeof path === 'string' && GLib.file_test(path, GLib.FileTest.EXISTS))
            : [];
    }

    async _saveFiles(paths) {
        await runJson(
            ['config', 'set', 'notchShelfFiles', JSON.stringify([...new Set(paths)])],
            this._cancellable
        );
        await this._refreshFiles();
    }

    async _chooseFiles() {
        this._closeMenu?.();
        try {
            const process = Gio.Subprocess.new([
                'zenity', '--file-selection', '--multiple', '--separator=\n',
                '--title=Park files in Veronica',
            ], Gio.SubprocessFlags.STDOUT_PIPE | Gio.SubprocessFlags.STDERR_PIPE);
            const [stdout] = await process.communicate_utf8_async(null, this._cancellable);
            if (!process.get_successful() || !stdout?.trim())
                return;
            const selected = stdout.split('\n').map(path => path.trim()).filter(Boolean);
            await this._saveFiles([...(await this._readFiles()), ...selected]);
        } catch (error) {
            console.debug(`veronica: file chooser failed: ${error}`);
        }
    }

    async _refreshFiles() {
        if (!this._fileRows)
            return;
        const paths = await this._readFiles();
        if (!this._fileRows)
            return;
        this._fileRows.destroy_all_children();
        if (paths.length === 0) {
            this._fileRows.add_child(new St.Label({
                text: 'Add files to keep them one click away',
                style_class: 'veronica-empty veronica-files-empty',
                x_align: Clutter.ActorAlign.CENTER,
            }));
            return;
        }
        for (const path of paths)
            this._fileRows.add_child(this._fileRow(path, paths));
    }

    _fileRow(path, allPaths) {
        const row = new St.BoxLayout({ style_class: 'veronica-file-row', x_expand: true });
        row.add_child(new St.Icon({ icon_name: 'text-x-generic-symbolic', style_class: 'veronica-file-icon' }));
        const open = new St.Button({
            style_class: 'veronica-file-open',
            label: GLib.path_get_basename(path),
            can_focus: true,
            x_expand: true,
            x_align: Clutter.ActorAlign.START,
        });
        open.connect('clicked', () => {
            try {
                Gio.AppInfo.launch_default_for_uri(Gio.File.new_for_path(path).get_uri(), null);
                this._closeMenu?.();
            } catch (error) {
                console.debug(`veronica: cannot open ${path}: ${error}`);
            }
        });
        row.add_child(open);
        row.add_child(this._smallButton('user-trash-symbolic', `Remove ${path}`, () => {
            this._saveFiles(allPaths.filter(item => item !== path)).catch(() => {});
        }));
        return row;
    }

    _toggleCameraPreview() {
        if (this._cameraPreview?.isRunning) {
            this._stopCameraPreview();
            return;
        }
        this._cameraPlaceholder.visible = true;
        this._cameraTitle.text = 'Starting camera…';
        this._cameraDetail.text = 'The preview will appear here.';
        this._cameraButton.label = 'Stop Preview';
        try {
            this._cameraPreview.start();
        } catch (error) {
            this._cameraPreviewFailed(`${error}`);
        }
    }

    _stopCameraPreview() {
        this._cameraPreview?.stop();
        if (!this._cameraPlaceholder)
            return;
        this._cameraPlaceholder.visible = true;
        this._cameraTitle.text = 'Camera preview';
        this._cameraDetail.text = 'Your camera stays on only while this preview is open.';
        this._cameraButton.label = 'Start Preview';
    }

    _cameraPreviewFailed(error) {
        if (!this._cameraPlaceholder)
            return;
        this._cameraPlaceholder.visible = true;
        this._cameraTitle.text = 'Camera unavailable';
        this._cameraDetail.text = `${error}`.replace(/^.*?:\s*/, '').slice(0, 92);
        this._cameraButton.label = 'Try Again';
    }

    onMenuClosed() {
        this._stopCameraPreview();
    }

    startCleanKeys() {
        this._startCleanKeys();
    }

    pickColor() {
        this._pickColor();
    }

    _startCleanKeys() {
        this._closeMenu?.();
        GLib.idle_add(GLib.PRIORITY_DEFAULT_IDLE, () => {
            if (!this.actor || this._cleanOverlay)
                return GLib.SOURCE_REMOVE;

            const overlay = new St.Widget({
                style_class: 'veronica-clean-overlay',
                reactive: true,
                can_focus: true,
                layout_manager: new Clutter.BinLayout(),
            });
            overlay.set_position(0, 0);
            overlay.set_size(global.stage.width, global.stage.height);
            overlay.connect('key-press-event', () => Clutter.EVENT_STOP);
            overlay.connect('key-release-event', () => Clutter.EVENT_STOP);

            const card = new St.BoxLayout({
                orientation: Clutter.Orientation.VERTICAL,
                style_class: 'veronica-clean-card',
                x_align: Clutter.ActorAlign.CENTER,
                y_align: Clutter.ActorAlign.CENTER,
            });
            card.add_child(new St.Icon({ icon_name: 'input-keyboard-symbolic' }));
            card.add_child(new St.Label({ text: 'Keyboard cleaning mode' }));
            card.add_child(new St.Label({
                text: 'Keys are blocked. Use the pointer to finish.',
                style_class: 'veronica-clean-detail',
            }));
            const done = new St.Button({
                style_class: 'veronica-clean-done',
                label: 'Done',
                can_focus: true,
            });
            done.connect('clicked', () => this._stopCleanKeys());
            card.add_child(done);
            overlay.add_child(card);

            this._cleanOverlay = overlay;
            Main.layoutManager.addTopChrome(overlay);
            this._cleanGrab = Main.pushModal(overlay);
            return GLib.SOURCE_REMOVE;
        });
    }

    _stopCleanKeys() {
        if (this._cleanGrab) {
            Main.popModal(this._cleanGrab);
            this._cleanGrab = null;
        }
        if (this._cleanOverlay) {
            Main.layoutManager.removeChrome(this._cleanOverlay);
            this._cleanOverlay.destroy();
            this._cleanOverlay = null;
        }
    }

    _applyPresenter(active) {
        this._usage?.setPrivate(active);
        this._nowPlaying?.setPrivate(active);
    }

    async _pickColor() {
        this._closeMenu?.();
        // Delegated to `vr color pick` rather than calling PickColor here, so
        // the sample lands in the swatch history and honours the configured
        // format and colour profile no matter where the pick was started from.
        // `vr` opens the same shell eyedropper, then copies and records.
        const picked = await runJson(['color', 'pick'], this._cancellable);
        if (!picked?.hex) {
            // Cancelling is the common case and is not worth a banner.
            console.debug('veronica: no colour was picked');
            return;
        }
        Main.notify('Color copied', picked.value ?? picked.hex);
    }

    async _refreshClipboard() {
        if (!this._clipboardRows)
            return;
        const rows = await recentEntries(30, this._cancellable);
        if (!this._clipboardRows)
            return;
        this._clipboardRows.destroy_all_children();
        if (rows.length === 0) {
            this._clipboardRows.add_child(new St.Label({
                text: 'Nothing copied yet',
                style_class: 'veronica-empty veronica-clipboard-empty',
                x_align: Clutter.ActorAlign.CENTER,
            }));
            return;
        }
        for (const row of rows)
            this._clipboardRows.add_child(this._clipboardRow(row));
    }

    _clipboardRow(row) {
        const wrap = new St.BoxLayout({ style_class: 'veronica-clipboard-row', x_expand: true });
        const copy = new St.Button({
            style_class: 'veronica-clipboard-copy',
            label: row.preview,
            can_focus: true,
            x_expand: true,
            x_align: Clutter.ActorAlign.START,
        });
        copy.connect('clicked', () => {
            entryText(row.id, this._cancellable)
                .then(text => text && this._clipboardWatcher?.write(text))
                .catch(() => {});
        });
        wrap.add_child(copy);
        const remove = new St.Button({
            style_class: 'veronica-clipboard-delete',
            can_focus: true,
            accessible_name: 'Delete clipboard entry',
            child: new St.Icon({ icon_name: 'user-trash-symbolic' }),
        });
        remove.connect('clicked', () => {
            runJson(['clipboard', 'remove', String(row.id)], this._cancellable)
                .then(() => this._refreshClipboard())
                .catch(() => {});
        });
        wrap.add_child(remove);
        return wrap;
    }

    async refresh() {
        await Promise.all([
            this._refreshPreferences(),
            this._usage?.refresh(this._cancellable),
            this._nowPlaying?.refresh(this._cancellable),
            this._refreshClipboard(),
            this._refreshFiles(),
            this._refreshToggle(this._keepAwake),
            this._refreshToggle(this._lidAwake),
            this._refreshToggle(this._presenter),
        ]);
        if (!this.actor)
            return;
        this._emptyMusic.visible = !this._nowPlaying.actor.visible;
    }

    async _refreshPreferences() {
        const settings = await runJson(['config', 'list'], this._cancellable);
        if (!this.actor)
            return;
        this._usage?.setProvider(settings?.limitsProvider);
        this._applyTheme(settings?.appearance);
    }

    _applyTheme(value) {
        let theme = THEMES.has(value) ? value : 'system';
        if (theme === 'system') {
            try {
                const desktop = new Gio.Settings({ schema_id: 'org.gnome.desktop.interface' });
                theme = desktop.get_string('color-scheme').includes('dark') ? 'dark' : 'light';
            } catch (_error) {
                theme = 'dark';
            }
        }
        this.actor.set_style_class_name(`veronica-shelf veronica-theme-${theme}`);
        this._onThemeChanged?.(theme);
    }

    _watchSettings() {
        try {
            const directory = GLib.build_filenamev([GLib.get_user_config_dir(), 'veronica']);
            GLib.mkdir_with_parents(directory, 0o700);
            this._settingsMonitor = Gio.File.new_for_path(directory)
                .monitor_directory(Gio.FileMonitorFlags.NONE, this._cancellable);
            this._settingsMonitor.connect('changed', (_monitor, file) => {
                if (file?.get_basename() !== 'settings.json')
                    return;
                if (this._settingsDebounceId)
                    GLib.Source.remove(this._settingsDebounceId);
                this._settingsDebounceId = GLib.timeout_add(
                    GLib.PRIORITY_DEFAULT_IDLE,
                    120,
                    () => {
                        this._settingsDebounceId = 0;
                        this._refreshPreferences().catch(() => {});
                        return GLib.SOURCE_REMOVE;
                    }
                );
            });
        } catch (error) {
            console.debug(`veronica: cannot monitor shared settings: ${error}`);
        }
    }

    async _refreshToggle(button) {
        if (!button)
            return;
        const value = await runJson(['config', 'get', button._veronicaKey], this._cancellable);
        if (this.actor) {
            this._setToggle(button, value === true);
            if (button._veronicaKey === 'preventSleep' ||
                button._veronicaKey === 'lidAwakeEnabled')
                this._power.set(button._veronicaKey, value === true);
            if (button._veronicaKey === 'presenterMode')
                this._applyPresenter(value === true);
        }
    }

    destroy() {
        if (this._settingsDebounceId) {
            GLib.Source.remove(this._settingsDebounceId);
            this._settingsDebounceId = 0;
        }
        this._settingsMonitor?.cancel();
        this._settingsMonitor = null;
        this._stopCleanKeys();
        this._power?.destroy();
        this._power = null;
        this._cameraPreview?.destroy();
        this._cameraPreview = null;
        this._usage?.destroy();
        this._nowPlaying?.destroy();
        this._clipboardRows = null;
        this._fileRows = null;
        this._messageList = null;
        this._cameraSurface = null;
        this._cameraPlaceholder = null;
        this._cameraTitle = null;
        this._cameraDetail = null;
        this._cameraButton = null;
        this._onThemeChanged = null;
        this.actor?.destroy();
        this.actor = null;
    }
}
