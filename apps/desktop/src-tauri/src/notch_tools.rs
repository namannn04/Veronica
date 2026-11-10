//! Native notch utility surface: GTK owns clipboard image and URI-drop protocols.
use crate::image_tools::{self, Source};
use gtk::{gdk_pixbuf::PixbufLoader, gio, glib, prelude::*};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

thread_local! { static WINDOW: RefCell<Option<gtk::Window>> = const { RefCell::new(None) }; }
struct ResultImage {
    bytes: Vec<u8>,
    format: String,
    preview: Vec<u8>,
}
type Shared = Rc<RefCell<Option<ResultImage>>>;
enum Job {
    Load(Vec<u8>),
    Path(std::path::PathBuf),
    Convert(String),
    Qr(String),
}

pub fn show() -> anyhow::Result<()> {
    if WINDOW.with(|slot| {
        if let Some(w) = slot.borrow().as_ref() {
            w.present();
            true
        } else {
            false
        }
    }) {
        return Ok(());
    }
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title("Veronica · Image & QR Tools");
    window.set_default_size(480, 540);
    window.set_keep_above(true);
    window.set_widget_name("veronica-utilities");
    window.set_default_size(500, 440);
    let notebook = gtk::Notebook::new();
    notebook.set_show_border(false);
    notebook.append_page(
        &panel(&window, "image")?,
        Some(&gtk::Label::new(Some("Image converter"))),
    );
    notebook.append_page(
        &panel(&window, "qr")?,
        Some(&gtk::Label::new(Some("QR code"))),
    );
    window.add(&notebook);
    let css = gtk::CssProvider::new();
    css.load_from_data(b"#veronica-utilities { background: @theme_bg_color; color: @theme_fg_color; border-radius: 22px; } #veronica-utilities notebook header { background: transparent; padding: 10px; border: none; } #veronica-utilities notebook tab { border-radius: 14px; padding: 10px 22px; margin: 0 4px; border: none; } #veronica-utilities notebook tab:checked { background: alpha(@theme_fg_color,0.10); box-shadow: none; } #veronica-utilities button { border-radius: 12px; padding: 8px 16px; } #veronica-utilities entry { border-radius: 12px; padding: 8px; }")?;
    if let Some(screen) = gtk::prelude::WidgetExt::screen(&window) {
        gtk::StyleContext::add_provider_for_screen(
            &screen,
            &css,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        window
            .connect_destroy(move |_| gtk::StyleContext::remove_provider_for_screen(&screen, &css));
    }
    window.connect_destroy(|_| {
        WINDOW.with(|slot| {
            slot.borrow_mut().take();
        });
    });
    WINDOW.with(|slot| *slot.borrow_mut() = Some(window.clone()));
    window.show_all();
    window.present();
    Ok(())
}
fn panel(window: &gtk::Window, kind: &str) -> anyhow::Result<gtk::Box> {
    let body = gtk::Box::new(gtk::Orientation::Vertical, 12);
    body.set_border_width(20);
    let hint = gtk::Label::new(Some("Drop an image, paste with Ctrl+V, or choose a file."));
    hint.set_line_wrap(true);
    body.pack_start(&hint, false, false, 0);
    let import = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let choose = gtk::Button::with_label("Choose image…");
    let paste = gtk::Button::with_label("Paste image");
    import.add(&choose);
    import.add(&paste);
    body.add(&import);
    let formats = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    body.add(&formats);
    formats.set_sensitive(false);
    let note = gtk::Label::new(Some(
        "SVG preserves the image in an SVG container. JPG uses a white background.",
    ));
    note.set_line_wrap(true);
    body.add(&note);
    let preview = gtk::Image::new();
    body.pack_start(&preview, true, true, 0);
    let output: Shared = Rc::new(RefCell::new(None));
    let status = gtk::Label::new(Some("PNG · JPG / JPEG · SVG — processed locally"));
    status.set_line_wrap(true);
    body.add(&status);
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let download = gtk::Button::with_label("Download");
    let copy = gtk::Button::with_label("Copy image");
    actions.add(&download);
    actions.add(&copy);
    body.add(&actions);
    actions.set_no_show_all(true);
    actions.hide();
    let qr_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let link = gtk::Entry::new();
    link.set_placeholder_text(Some("Paste a link for a QR code"));
    let generate = gtk::Button::with_label("Generate QR");
    qr_row.pack_start(&link, true, true, 0);
    qr_row.add(&generate);
    body.add(&qr_row);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(u64, Job)>();
    let (result_tx, mut results) = tokio::sync::mpsc::unbounded_channel();
    std::thread::spawn(move || {
        let mut source: Option<Source> = None;
        while let Some((mut request, mut job)) = rx.blocking_recv() {
            // A burst of clicks retains at most the newest pending operation.
            while let Ok((next_request, next_job)) = rx.try_recv() {
                request = next_request;
                job = next_job;
            }
            let result: anyhow::Result<(Option<ResultImage>, String, Vec<u8>)> = (|| {
                match job {
                    Job::Load(bytes) => {
                        source = None;
                        source = Some(image_tools::decode(&bytes)?);
                    }
                    Job::Path(path) => {
                        source = None;
                        anyhow::ensure!(
                            std::fs::metadata(&path)?.is_file(),
                            "Choose a regular image file"
                        );
                        anyhow::ensure!(
                            std::fs::metadata(&path)?.len() <= image_tools::MAX_BYTES as u64,
                            "Image must be at most 20 MiB"
                        );
                        use std::io::Read;
                        let file = std::fs::File::open(path)?;
                        anyhow::ensure!(file.metadata()?.is_file(), "Choose a regular image file");
                        let mut bytes = Vec::new();
                        file.take(image_tools::MAX_BYTES as u64 + 1)
                            .read_to_end(&mut bytes)?;
                        source = Some(image_tools::decode(&bytes)?);
                    }
                    Job::Convert(format) => {
                        let source = source
                            .as_ref()
                            .ok_or_else(|| anyhow::anyhow!("Choose an image first"))?;
                        let bytes = image_tools::encode(source, &format)?;
                        let preview = if format == "jpg" {
                            image_tools::encode(&image_tools::decode(&bytes)?, "png")?
                        } else {
                            image_tools::encode(source, "png")?
                        };
                        return Ok((
                            Some(ResultImage {
                                bytes,
                                format,
                                preview: preview.clone(),
                            }),
                            "Converted — ready to download or copy".into(),
                            preview,
                        ));
                    }
                    Job::Qr(link) => {
                        let png = image_tools::encode(&image_tools::qr_source(&link)?, "png")?;
                        return Ok((
                            Some(ResultImage {
                                bytes: png.clone(),
                                format: "png".into(),
                                preview: png.clone(),
                            }),
                            "QR ready — links directly to your original URL".into(),
                            png,
                        ));
                    }
                }
                let source = source.as_ref().unwrap();
                let preview = image_tools::encode(source, "png")?;
                Ok((
                    None,
                    format!(
                        "Detected {} · {} × {}",
                        source.format,
                        source.pixels.width(),
                        source.pixels.height()
                    ),
                    preview,
                ))
            })();
            if result_tx
                .send((request, result.map_err(|error| error.to_string())))
                .is_err()
            {
                break;
            }
        }
    });
    let latest = Rc::new(Cell::new(0_u64));
    let pending_request = latest.clone();
    if kind == "image" {
        qr_row.set_no_show_all(true);
        qr_row.hide();
    } else {
        for widget in [
            hint.upcast::<gtk::Widget>(),
            import.upcast(),
            formats.clone().upcast(),
            note.upcast(),
        ] {
            widget.set_no_show_all(true);
            widget.hide();
        }
        status.set_text("Paste a link below to create a QR code. Everything stays local.");
    }
    let pending_formats = formats.clone();
    let pending_actions = actions.clone();
    let pending_status = status.clone();
    let submit = Rc::new(move |job| {
        if matches!(&job, Job::Load(_) | Job::Path(_)) {
            pending_formats.set_sensitive(false);
        }
        pending_actions.hide();
        pending_status.set_text("Processing locally…");
        let request = pending_request.get() + 1;
        pending_request.set(request);
        let _ = tx.send((request, job));
    });
    for (name, format) in [("PNG", "png"), ("JPG", "jpg"), ("SVG", "svg")] {
        let button = gtk::Button::with_label(name);
        let submit = submit.clone();
        button.connect_clicked(move |_| {
            submit(Job::Convert(format.into()));
        });
        formats.add(&button);
    }
    let weak = window.downgrade();
    let sender = submit.clone();
    choose.connect_clicked(move |_| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let dialog = gtk::FileChooserDialog::new(
            Some("Choose PNG, JPG/JPEG or SVG"),
            Some(&window),
            gtk::FileChooserAction::Open,
        );
        dialog.add_buttons(&[
            ("Cancel", gtk::ResponseType::Cancel),
            ("Open", gtk::ResponseType::Accept),
        ]);
        let filter = gtk::FileFilter::new();
        filter.set_name(Some("Images"));
        for mime in ["image/png", "image/jpeg", "image/svg+xml"] {
            filter.add_mime_type(mime);
        }
        dialog.add_filter(filter);
        if dialog.run() == gtk::ResponseType::Accept {
            if let Some(path) = dialog.filename() {
                sender(Job::Path(path));
            }
        }
        dialog.close();
    });
    let sender = submit.clone();
    let message = status.clone();
    let paste_image = Rc::new(move || {
        let sender = sender.clone();
        let message = message.clone();
        gtk::Clipboard::get(&gtk::gdk::SELECTION_CLIPBOARD).request_image(move |_, image| {
            if let Some(image) = image {
                match image.save_to_bufferv("png", &[]) {
                    Ok(bytes) => {
                        sender(Job::Load(bytes));
                    }
                    Err(e) => message.set_text(&e.to_string()),
                }
            } else {
                message.set_text("No image on the clipboard. Copy an image first.");
            }
        });
    });
    let paste_action = paste_image.clone();
    paste.connect_clicked(move |_| paste_action());
    let paste_entry = link.clone();
    let page = body.downgrade();
    let image_page = kind == "image";
    window.connect_key_press_event(move |_, event| {
        if !image_page || !page.upgrade().is_some_and(|p| p.is_mapped()) {
            return glib::Propagation::Proceed;
        }
        if event.state().contains(gtk::gdk::ModifierType::CONTROL_MASK)
            && event.keyval() == gtk::gdk::keys::constants::v
        {
            // Let the URL entry handle text paste normally.
            if !paste_entry.has_focus() {
                paste_image();
                return glib::Propagation::Stop;
            }
        }
        glib::Propagation::Proceed
    });
    let targets = [gtk::TargetEntry::new(
        "text/uri-list",
        gtk::TargetFlags::empty(),
        0,
    )];
    body.drag_dest_set(gtk::DestDefaults::ALL, &targets, gtk::gdk::DragAction::COPY);
    let sender = submit.clone();
    let message = status.clone();
    body.connect_drag_data_received(move |_, context, _, _, data, _, time| {
        let path = data
            .uris()
            .first()
            .and_then(|uri| gio::File::for_uri(uri.as_str()).path());
        let ok = if let Some(path) = path {
            {
                sender(Job::Path(path));
                true
            }
        } else {
            message.set_text("Drop a local image file.");
            false
        };
        context.drag_finish(ok, false, time);
    });
    let entry = link;
    let sender = submit.clone();
    let entry_copy = entry.clone();
    generate.connect_clicked(move |_| {
        sender(Job::Qr(entry_copy.text().to_string()));
    });
    entry.connect_activate(move |entry| {
        submit(Job::Qr(entry.text().to_string()));
    });
    let weak = window.downgrade();
    let saved = output.clone();
    let message = status.clone();
    download.connect_clicked(move |_| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let saved = saved.borrow();
        let Some(image) = saved.as_ref() else {
            return;
        };
        let dialog = gtk::FileChooserDialog::new(
            Some("Save image"),
            Some(&window),
            gtk::FileChooserAction::Save,
        );
        dialog.add_buttons(&[
            ("Cancel", gtk::ResponseType::Cancel),
            ("Save", gtk::ResponseType::Accept),
        ]);
        dialog.set_current_name(&format!("veronica.{}", image.format));
        dialog.set_do_overwrite_confirmation(true);
        if dialog.run() == gtk::ResponseType::Accept {
            if let Some(path) = dialog.filename() {
                match std::fs::write(path, &image.bytes) {
                    Ok(()) => message.set_text("Image saved"),
                    Err(e) => message.set_text(&e.to_string()),
                }
            }
        }
        dialog.close();
    });
    let saved = output.clone();
    let message = status.clone();
    copy.connect_clicked(move |_| {
        if let Some(image) = saved.borrow().as_ref() {
            match pixbuf(&image.preview) {
                Ok(pixels) => {
                    gtk::Clipboard::get(&gtk::gdk::SELECTION_CLIPBOARD).set_image(&pixels);
                    message.set_text("Image copied — paste into another app");
                }
                Err(e) => message.set_text(&e.to_string()),
            }
        }
    });
    let weak = window.downgrade();
    let task = glib::MainContext::default().spawn_local(async move {
        while let Some((request, result)) = results.recv().await {
            if request != latest.get() {
                continue;
            }
            if weak.upgrade().is_none() {
                break;
            }
            output.borrow_mut().take();
            actions.hide();
            preview.clear();
            match result {
                Ok((image, text, bytes)) => {
                    formats.set_sensitive(true);
                    status.set_text(&text);
                    if let Ok(pixels) = pixbuf(&bytes) {
                        if let Some(small) = pixels.scale_simple(
                            (f64::from(pixels.width())
                                * (320.0 / f64::from(pixels.width()))
                                    .min(220.0 / f64::from(pixels.height()))
                                    .min(1.0))
                            .round()
                            .max(1.0) as i32,
                            (f64::from(pixels.height())
                                * (320.0 / f64::from(pixels.width()))
                                    .min(220.0 / f64::from(pixels.height()))
                                    .min(1.0))
                            .round()
                            .max(1.0) as i32,
                            gtk::gdk_pixbuf::InterpType::Nearest,
                        ) {
                            preview.set_from_pixbuf(Some(&small));
                        }
                    }
                    if image.is_some() {
                        actions.set_no_show_all(false);
                        actions.show_all();
                        actions.set_no_show_all(true);
                    }
                    *output.borrow_mut() = image;
                }
                Err(error) => status.set_text(&error),
            }
        }
    });
    window.connect_destroy(move |_| {
        task.abort();
    });
    Ok(body)
}
fn pixbuf(bytes: &[u8]) -> anyhow::Result<gtk::gdk_pixbuf::Pixbuf> {
    let loader = PixbufLoader::new();
    loader.write(bytes)?;
    loader.close()?;
    loader
        .pixbuf()
        .ok_or_else(|| anyhow::anyhow!("Cannot display image"))
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Run explicitly in a GNOME/X11 session; CI exercises the pure engine tests.
    #[test]
    #[ignore = "requires a desktop display and image clipboard"]
    fn native_image_and_qr_clipboard() {
        gtk::init().expect("GTK display");
        show().unwrap();
        let window = WINDOW.with(|slot| slot.borrow().as_ref().unwrap().clone());
        let notebook = window.child().unwrap().downcast::<gtk::Notebook>().unwrap();
        let body = notebook
            .nth_page(Some(0))
            .unwrap()
            .downcast::<gtk::Box>()
            .unwrap();
        let children = body.children();
        let actions = children[6].clone().downcast::<gtk::Box>().unwrap();
        assert!(!actions.is_visible());
        let status = children[5].clone().downcast::<gtk::Label>().unwrap();
        let input =
            gtk::gdk_pixbuf::Pixbuf::new(gtk::gdk_pixbuf::Colorspace::Rgb, true, 8, 2, 3).unwrap();
        input.fill(0xff000000);
        gtk::Clipboard::get(&gtk::gdk::SELECTION_CLIPBOARD).set_image(&input);
        let import = children[1].clone().downcast::<gtk::Box>().unwrap();
        import.children()[1]
            .clone()
            .downcast::<gtk::Button>()
            .unwrap()
            .emit_clicked();
        pump_until(|| status.text().starts_with("Detected PNG"));
        let formats = children[2].clone().downcast::<gtk::Box>().unwrap();
        formats.children()[1]
            .clone()
            .downcast::<gtk::Button>()
            .unwrap()
            .emit_clicked();
        pump_until(|| actions.is_visible());
        actions.children()[1]
            .clone()
            .downcast::<gtk::Button>()
            .unwrap()
            .emit_clicked();
        let copied_jpg = Rc::new(Cell::new(false));
        let received_jpg = copied_jpg.clone();
        gtk::Clipboard::get(&gtk::gdk::SELECTION_CLIPBOARD).request_image(move |_, image| {
            let image = image.expect("converted JPEG clipboard pixels");
            assert_eq!((image.width(), image.height()), (2, 3));
            assert!(image.read_pixel_bytes()[..3]
                .iter()
                .all(|channel| *channel > 245));
            received_jpg.set(true);
        });
        pump_until(|| copied_jpg.get());
        notebook.set_current_page(Some(1));
        let body = notebook
            .nth_page(Some(1))
            .unwrap()
            .downcast::<gtk::Box>()
            .unwrap();
        let children = body.children();
        let actions = children[6].clone().downcast::<gtk::Box>().unwrap();
        let qr_row = children[7].clone().downcast::<gtk::Box>().unwrap();
        let entry = qr_row.children()[0]
            .clone()
            .downcast::<gtk::Entry>()
            .unwrap();
        entry.set_text("https://example.com/veronica?test=1&source=notch");
        entry.emit_activate();
        pump_until(|| actions.is_visible());
        let copy = actions.children()[1]
            .clone()
            .downcast::<gtk::Button>()
            .unwrap();
        copy.emit_clicked();
        let copied = Rc::new(Cell::new(false));
        let received = copied.clone();
        gtk::Clipboard::get(&gtk::gdk::SELECTION_CLIPBOARD).request_image(move |_, image| {
            let image = image.expect("actual image clipboard payload");
            assert!(image.width() >= 320 && image.height() >= 320);
            received.set(true);
        });
        pump_until(|| copied.get());
        window.close();
    }
    fn pump_until(done: impl Fn() -> bool) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        while !done() {
            assert!(
                std::time::Instant::now() < deadline,
                "native operation timed out"
            );
            while glib::MainContext::default().pending() {
                glib::MainContext::default().iteration(false);
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}
