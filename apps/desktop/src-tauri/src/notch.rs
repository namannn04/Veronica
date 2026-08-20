//! The notch overlay.
//!
//! Edith turns the MacBook's physical notch into a hover shelf. Ubuntu has no
//! notch, so Veronica hangs an island from the top bar instead: a pill tucked
//! directly beneath the bar that grows downward on hover.
//!
//! Two platform facts shape the implementation.
//!
//! First, positioning. The overlay must sit at the top centre of the primary
//! display and stay above other windows. A Wayland client may do neither, which
//! is why the process asks GTK for the X11 backend before startup (see `main`);
//! under XWayland both hints work. GNOME's own top bar lives in the shell's
//! layer and cannot be covered by an application window, so the island is
//! anchored just below it and reads as attached to it.
//!
//! Second, the clickable area. GTK enforces a minimum height on a window that
//! holds a webview - about 200px, far taller than the 34px pill - so shrinking
//! the window to fit the collapsed island is impossible. Instead the window
//! keeps one size and an X11 input shape exposes only the part the island
//! actually occupies. Without that shape the transparent remainder of an
//! always-on-top window swallows every click meant for the desktop beneath it.

use anyhow::{Context, Result};
use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, WebviewUrl, WebviewWindowBuilder};

pub const WINDOW_LABEL: &str = "notch";

/// Collapsed pill: wide enough for a clock, the day's spend and two indicators.
pub const COLLAPSED: LogicalSize<f64> = LogicalSize {
    width: 300.0,
    height: 34.0,
};

/// Expanded island. Two columns, like the shell's own clock dropdown: media and
/// notifications on the left, the month and the day's agenda on the right. This
/// is also the window's fixed size, because the interactive region is what
/// changes rather than the window.
pub const EXPANDED: LogicalSize<f64> = LogicalSize {
    width: 740.0,
    height: 560.0,
};

/// Height of GNOME's top bar at scale 1. The island hangs directly beneath it.
pub const DEFAULT_TOP_BAR_HEIGHT: f64 = 34.0;

/// The island's rectangle inside the window, in logical pixels.
///
/// The window is always `EXPANDED`; the collapsed island is a narrower pill
/// centred on the same axis, so its rectangle is inset from the left.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IslandRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

pub fn island_rect(expanded: bool) -> IslandRect {
    if expanded {
        return IslandRect {
            x: 0.0,
            y: 0.0,
            width: EXPANDED.width,
            height: EXPANDED.height,
        };
    }
    IslandRect {
        // Centred within the window so the pill and the expanded body share a
        // centre line and expanding does not appear to jump sideways.
        x: (EXPANDED.width - COLLAPSED.width) / 2.0,
        y: 0.0,
        width: COLLAPSED.width,
        height: COLLAPSED.height,
    }
}

/// Horizontally centred on the primary monitor, at `top_offset` from the top.
pub fn centred_position(
    monitor_logical_width: f64,
    window_width: f64,
    top_offset: f64,
) -> LogicalPosition<f64> {
    // A window wider than the monitor would otherwise be pushed off the left
    // edge, so clamp rather than centring negatively.
    let x = ((monitor_logical_width - window_width) / 2.0).max(0.0);
    LogicalPosition {
        x,
        y: top_offset.max(0.0),
    }
}

/// Create the overlay window, hidden until the notch extension is enabled.
pub fn create(app: &AppHandle) -> Result<()> {
    if app.get_webview_window(WINDOW_LABEL).is_some() {
        return Ok(());
    }

    let window = WebviewWindowBuilder::new(app, WINDOW_LABEL, WebviewUrl::App("index.html".into()))
        .title("Veronica Notch")
        .inner_size(EXPANDED.width, EXPANDED.height)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .shadow(false)
        .focused(false)
        .visible(false)
        .build()
        .context("cannot create the notch overlay window")?;

    reposition(&window)?;
    Ok(())
}

/// Move the overlay to the top centre of the primary monitor.
fn reposition(window: &tauri::WebviewWindow) -> Result<()> {
    let Some(monitor) = window.primary_monitor()? else {
        // Without monitor information the compositor's placement is the best
        // available answer.
        return Ok(());
    };
    let scale = monitor.scale_factor();
    let logical_width = monitor.size().width as f64 / scale;
    let position = centred_position(logical_width, EXPANDED.width, DEFAULT_TOP_BAR_HEIGHT);
    window.set_position(tauri::Position::Logical(position))?;
    Ok(())
}

/// Restrict pointer input to the island's rectangle.
///
/// Everything outside it passes through to whatever is underneath, so the
/// transparent part of the window is not a dead zone over the desktop.
#[cfg(target_os = "linux")]
fn apply_input_shape(window: &tauri::WebviewWindow, expanded: bool) -> Result<()> {
    use gtk::prelude::*;

    let rect = island_rect(expanded);
    let scale = window.scale_factor().unwrap_or(1.0);
    let to_device = |value: f64| (value * scale).round() as i32;

    let region = cairo::Region::create_rectangle(&cairo::RectangleInt::new(
        to_device(rect.x),
        to_device(rect.y),
        to_device(rect.width),
        to_device(rect.height),
    ));

    let gtk_window = window
        .gtk_window()
        .context("the overlay has no GTK window")?;
    gtk_window.input_shape_combine_region(Some(&region));
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn apply_input_shape(_window: &tauri::WebviewWindow, _expanded: bool) -> Result<()> {
    Ok(())
}

/// Grow or shrink the island.
///
/// The window does not change size; only the interactive region does, and the
/// interface animates the island's own width and height inside it.
///
/// `pinned` means the island was opened by a click rather than a hover, so it
/// should stay open. It also takes keyboard focus, which is what lets Escape
/// close it: an unfocused window never sees the key.
pub fn set_expanded(app: &AppHandle, expanded: bool, pinned: bool) -> Result<()> {
    let Some(window) = app.get_webview_window(WINDOW_LABEL) else {
        return Ok(());
    };
    apply_input_shape(&window, expanded)?;

    if pinned {
        // Raise first: a click on the pill does not necessarily bring an
        // always-on-top window forward on every window manager.
        let _ = window.set_always_on_top(true);
        let _ = window.set_focus();
    }
    Ok(())
}

pub fn set_visible(app: &AppHandle, visible: bool) -> Result<()> {
    let Some(window) = app.get_webview_window(WINDOW_LABEL) else {
        return Ok(());
    };
    if visible {
        window.show()?;
        // Re-assert stacking: another window may have been raised above the
        // overlay while it was hidden.
        window.set_always_on_top(true)?;
        reposition(&window)?;
        // The shape only applies to a realised window, so it is set after show.
        apply_input_shape(&window, false)?;
    } else {
        window.hide()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centres_the_island_horizontally_below_the_top_bar() {
        let position = centred_position(1920.0, EXPANDED.width, DEFAULT_TOP_BAR_HEIGHT);
        assert_eq!(position.x, (1920.0 - EXPANDED.width) / 2.0);
        assert_eq!(position.y, 34.0);
    }

    #[test]
    fn the_expanded_island_fits_a_common_display() {
        // A 1366x768 laptop is the smallest screen worth supporting; the island
        // must not exceed it or the shape would extend past the display.
        assert!(EXPANDED.width <= 1366.0);
        assert!(EXPANDED.height <= 768.0 - DEFAULT_TOP_BAR_HEIGHT);
    }

    #[test]
    fn a_window_wider_than_the_display_is_clamped_on_screen() {
        assert_eq!(centred_position(400.0, 520.0, 34.0).x, 0.0);
    }

    #[test]
    fn a_negative_offset_is_clamped_to_the_top_edge() {
        assert_eq!(centred_position(1920.0, 300.0, -20.0).y, 0.0);
    }

    #[test]
    fn the_collapsed_pill_is_centred_inside_the_fixed_window() {
        let rect = island_rect(false);
        assert_eq!(rect.width, COLLAPSED.width);
        assert_eq!(rect.height, COLLAPSED.height);
        // Derived, not a literal, so changing the island size cannot silently
        // leave this test asserting the old geometry.
        assert_eq!(rect.x, (EXPANDED.width - COLLAPSED.width) / 2.0);
        assert_eq!(rect.y, 0.0);
    }

    #[test]
    fn both_states_share_a_centre_line_so_expanding_does_not_jump() {
        let collapsed = island_rect(false);
        let expanded = island_rect(true);
        let centre = |r: IslandRect| r.x + r.width / 2.0;
        assert!((centre(collapsed) - centre(expanded)).abs() < 0.001);
    }

    #[test]
    fn the_expanded_island_fills_the_whole_window() {
        let rect = island_rect(true);
        assert_eq!(rect.x, 0.0);
        assert_eq!(rect.y, 0.0);
        assert_eq!(rect.width, EXPANDED.width);
        assert_eq!(rect.height, EXPANDED.height);
    }

    #[test]
    fn the_interactive_region_never_covers_the_whole_window_when_collapsed() {
        // The point of the shape is that the transparent remainder stays
        // clickable, so a collapsed island must be strictly shorter.
        let rect = island_rect(false);
        assert!(rect.height < EXPANDED.height);
        assert!(rect.width < EXPANDED.width);
    }
}
