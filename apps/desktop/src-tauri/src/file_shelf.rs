//! Native GTK file shelf: real URI drags work with browsers and file managers.
use crate::state::AppState;
use gtk::{gio, glib, prelude::*};
use std::{cell::RefCell, path::PathBuf};
use tauri::Manager;
use veronica_core::file_shelf::FileShelf;

thread_local! { static WINDOW: RefCell<Option<gtk::Window>> = const { RefCell::new(None) }; }

enum Change {
    Imported(Result<usize, String>),
    Removed(Result<(), String>),
}

pub fn show(app: &tauri::AppHandle) -> anyhow::Result<()> {
    if WINDOW.with(|slot| {
        if let Some(window) = slot.borrow().as_ref() {
            window.present();
            true
        } else {
            false
        }
    }) {
        return Ok(());
    }
    let state = app.state::<AppState>();
    let root = state.directories.data.join("file-shelf");
    let shelf = FileShelf::new(root);
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title("Veronica File Shelf");
    window.set_default_size(580, 320);
    window.set_decorated(false);
    window.set_keep_above(true);
    window.set_skip_taskbar_hint(true);
    window.set_widget_name("veronica-file-shelf");
    if let Some(screen) = gtk::prelude::WidgetExt::screen(&window) {
        window.set_visual(screen.rgba_visual().as_ref());
    }
    let css = gtk::CssProvider::new();
    css.load_from_data(b"#veronica-file-shelf { background: #101012; color: #f5f5f7; border: 1px solid #343438; border-radius: 24px; } #veronica-file-shelf button { border-radius: 10px; } #veronica-file-shelf label { color: #f5f5f7; }")?;
    if let Some(screen) = gtk::prelude::WidgetExt::screen(&window) {
        gtk::StyleContext::add_provider_for_screen(
            &screen,
            &css,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        window
            .connect_destroy(move |_| gtk::StyleContext::remove_provider_for_screen(&screen, &css));
    }
    let body = gtk::Box::new(gtk::Orientation::Vertical, 12);
    body.set_border_width(16);
    window.add(&body);
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let handle = gtk::EventBox::new();
    let title = gtk::Label::new(Some("File Shelf · Drag to move"));
    title.set_xalign(0.0);
    handle.add(&title);
    header.pack_start(&handle, true, true, 0);
    let close = gtk::Button::with_label("×");
    header.pack_end(&close, false, false, 0);
    body.pack_start(&header, false, false, 0);
    let weak = window.downgrade();
    handle.connect_button_press_event(move |_, event| {
        if event.button() == 1 {
            if let Some(window) = weak.upgrade() {
                let (x, y) = event.root();
                window.begin_move_drag(1, x as i32, y as i32, event.time());
            }
        }
        glib::Propagation::Proceed
    });
    let weak = window.downgrade();
    close.connect_clicked(move |_| {
        if let Some(window) = weak.upgrade() {
            window.close();
        }
    });
    window.connect_key_press_event(|window, event| {
        if event.keyval() == gtk::gdk::keys::constants::Escape {
            window.close();
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    let hint = gtk::Label::new(Some(
        "Drop files or folders here to keep a copy. Drag a saved file out to use it.",
    ));
    hint.set_line_wrap(true);
    hint.set_xalign(0.0);
    body.pack_start(&hint, false, false, 0);
    let status = gtk::Label::new(None);
    status.set_line_wrap(true);
    status.set_xalign(0.0);
    let scroll = gtk::ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    let rows = gtk::Box::new(gtk::Orientation::Vertical, 6);
    scroll.add(&rows);
    body.pack_start(&scroll, true, true, 0);
    let add = gtk::Button::with_label("Add files…");
    body.pack_start(&add, false, false, 0);
    body.pack_start(&status, false, false, 0);
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel::<Change>();
    render(&rows, &shelf, &sender)?;
    let targets = [gtk::TargetEntry::new(
        "text/uri-list",
        gtk::TargetFlags::empty(),
        0,
    )];
    body.drag_dest_set(
        gtk::DestDefaults::MOTION | gtk::DestDefaults::HIGHLIGHT,
        &targets,
        gtk::gdk::DragAction::COPY,
    );
    body.connect_drag_drop(|widget, context, _, _, time| {
        widget.drag_get_data(context, &gtk::gdk::Atom::intern("text/uri-list"), time);
        true
    });
    let store = shelf.clone();
    let tx = sender.clone();
    let pending = status.clone();
    body.connect_drag_data_received(move |_, context, _, _, data, _, time| {
        let paths: Vec<_> = data
            .uris()
            .iter()
            .filter_map(|uri| gio::File::for_uri(uri.as_str()).path())
            .collect();
        if paths.is_empty() {
            context.drag_finish(false, false, time);
            pending.set_text("Drop local files or folders.");
            return;
        }
        pending.set_text("Copying into shelf…");
        import_files(
            store.clone(),
            paths,
            tx.clone(),
            Some((context.clone(), time)),
        );
    });
    let weak = window.downgrade();
    let store = shelf.clone();
    let tx = sender.clone();
    let pending = status.clone();
    add.connect_clicked(move |_| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let dialog = gtk::FileChooserDialog::new(
            Some("Add files to shelf"),
            Some(&window),
            gtk::FileChooserAction::Open,
        );
        dialog.add_buttons(&[
            ("Cancel", gtk::ResponseType::Cancel),
            ("Add copies", gtk::ResponseType::Accept),
        ]);
        dialog.set_select_multiple(true);
        if dialog.run() == gtk::ResponseType::Accept {
            pending.set_text("Copying into shelf…");
            import_files(store.clone(), dialog.filenames(), tx.clone(), None);
        }
        dialog.close();
    });
    let legacy: Vec<PathBuf> = state
        .settings_snapshot()
        .get("notchShelfFiles")
        .and_then(|value| value.as_array())
        .map(|paths| {
            paths
                .iter()
                .filter_map(|path| path.as_str().map(PathBuf::from))
                .collect()
        })
        .unwrap_or_default();
    if !legacy.is_empty() {
        let store = shelf.clone();
        let tx = sender.clone();
        std::thread::spawn(move || {
            let result = store
                .import_legacy(&legacy)
                .map_err(|error| error.to_string());
            let _ = tx.send(Change::Imported(result));
        });
    }
    let weak = window.downgrade();
    let source = glib::MainContext::default().spawn_local(async move {
        while let Some(change) = receiver.recv().await {
            if weak.upgrade().is_none() {
                break;
            }
            let message = match change {
                Change::Imported(result) => match result {
                    Ok(count) => format!("{count} copies saved. Dragging out keeps them here."),
                    Err(error) => error,
                },
                Change::Removed(result) => match result {
                    Ok(()) => "Removed from shelf.".into(),
                    Err(error) => error,
                },
            };
            status.set_text(&message);
            if let Err(error) = render(&rows, &shelf, &sender) {
                status.set_text(&error.to_string());
            }
        }
    });
    window.connect_destroy(move |_| {
        source.abort();
        WINDOW.with(|slot| {
            slot.borrow_mut().take();
        });
    });
    WINDOW.with(|slot| *slot.borrow_mut() = Some(window.clone()));
    window.show_all();
    if let Some(monitor) =
        gtk::gdk::Display::default().and_then(|display| display.primary_monitor())
    {
        let area = monitor.workarea();
        window.move_(area.x() + (area.width() - 580) / 2, area.y() + 8);
    }
    window.present();
    Ok(())
}

fn import_files(
    shelf: FileShelf,
    paths: Vec<PathBuf>,
    sender: tokio::sync::mpsc::UnboundedSender<Change>,
    drag: Option<(gtk::gdk::DragContext, u32)>,
) {
    // GTK objects cannot cross threads. Complete native DND on the main
    // context through a one-shot channel; file copying stays off the UI thread.
    let (tx, rx) = tokio::sync::oneshot::channel::<Result<usize, String>>();
    glib::MainContext::default().spawn_local(async move {
        if let Ok(result) = rx.await {
            if let Some((context, time)) = &drag {
                context.drag_finish(result.is_ok(), false, *time);
            }
            let _ = sender.send(Change::Imported(result));
        }
    });
    std::thread::spawn(move || {
        let mut count = 0;
        let mut errors = Vec::new();
        for path in paths {
            match shelf.import(&path) {
                Ok(_) => count += 1,
                Err(e) => errors.push(format!("{}: {e}", path.display())),
            }
        }
        let result = if errors.is_empty() {
            Ok(count)
        } else {
            Err(format!("{count} saved. {}", errors.join("; ")))
        };
        let _ = tx.send(result);
    });
}

fn render(
    rows: &gtk::Box,
    shelf: &FileShelf,
    sender: &tokio::sync::mpsc::UnboundedSender<Change>,
) -> anyhow::Result<()> {
    for child in rows.children() {
        rows.remove(&child);
    }
    let targets = [gtk::TargetEntry::new(
        "text/uri-list",
        gtk::TargetFlags::empty(),
        0,
    )];
    let entries = shelf.entries()?;
    if entries.is_empty() {
        rows.add(&gtk::Label::new(Some(
            "Your shelf is empty · Drop a file to start",
        )));
    }
    for entry in entries {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let file = gtk::Button::with_label(&entry.name);
        file.set_hexpand(true);
        file.set_tooltip_text(Some("Drag to another app · the shelf keeps its copy"));
        file.drag_source_set(
            gtk::gdk::ModifierType::BUTTON1_MASK,
            &targets,
            gtk::gdk::DragAction::COPY,
        );
        let uri = gio::File::for_path(&entry.path).uri();
        file.connect_drag_data_get(move |_, _, data, _, _| {
            data.set_uris(&[uri.as_str()]);
        });
        let open_uri = gio::File::for_path(&entry.path).uri();
        file.connect_clicked(move |_| {
            let _ = gio::AppInfo::launch_default_for_uri(&open_uri, None::<&gio::AppLaunchContext>);
        });
        row.pack_start(&file, true, true, 0);
        let remove = gtk::Button::with_label("Remove");
        let store = shelf.clone();
        let id = entry.id;
        let tx = sender.clone();
        remove.connect_clicked(move |_| {
            let store = store.clone();
            let id = id.clone();
            let tx = tx.clone();
            std::thread::spawn(move || {
                let result = store.remove(&id).map_err(|e| e.to_string());
                let _ = tx.send(Change::Removed(result));
            });
        });
        row.pack_end(&remove, false, false, 0);
        rows.pack_start(&row, false, false, 0);
    }
    rows.show_all();
    Ok(())
}
