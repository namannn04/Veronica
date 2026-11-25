use std::fmt::Write as _;

use anyhow::Result;
use veronica_core::AppDirectories;
use veronica_host::surfaces::{self, template_name, Placement, Surface};
use veronica_host::Layout;

use crate::format::Output;

fn describe(surfaces: &[Surface]) -> String {
    let mut text = String::new();
    for surface in surfaces {
        let place = match surface.placement {
            Placement::Home => "home",
            Placement::Notch => "notch",
        };
        let _ = writeln!(
            text,
            "{place:<6} {:<16} {:<20} {}",
            surface.extension,
            surface.id,
            template_name(surface.template)
        );
    }
    if text.is_empty() {
        "No extension is showing a card or notch activity.".to_string()
    } else {
        text.trim_end().to_string()
    }
}

pub(crate) fn run(directories: &AppDirectories, output: Output) -> Result<()> {
    let surfaces = surfaces::load(&Layout::new(directories).surfaces_file());
    output.emit(&surfaces, || describe(&surfaces))
}
