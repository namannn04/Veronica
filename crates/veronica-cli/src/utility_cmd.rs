//! Bounded, short-lived image work for the shell; no resident converter service.
use crate::format::Output;
use anyhow::{ensure, Result};
use std::{
    io::Read,
    path::{Path, PathBuf},
};
use veronica_core::AppDirectories;
use veronica_system::image_tools::{self, Source};

#[derive(clap::Subcommand)]
pub enum UtilityCommand {
    Image {
        path: PathBuf,
        #[arg(value_parser=["png", "jpg", "svg"])]
        format: Option<String>,
    },
    Qr {
        text: String,
    },
}
fn read(path: &Path) -> Result<Vec<u8>> {
    let file = std::fs::File::open(path)?;
    ensure!(file.metadata()?.is_file(), "Choose a regular image file");
    let mut bytes = Vec::new();
    file.take(image_tools::MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= image_tools::MAX_BYTES,
        "Image must be at most 20 MiB"
    );
    Ok(bytes)
}
pub fn run(dirs: &AppDirectories, command: &UtilityCommand, output: Output) -> Result<()> {
    let (source, format) = match command {
        UtilityCommand::Image { path, format } => {
            (image_tools::decode(&read(path)?)?, format.as_deref())
        }
        UtilityCommand::Qr { text } => (image_tools::decode(&image_tools::qr(text)?)?, Some("png")),
    };
    let root =
        dirs.cache
            .join("notch-tools")
            .join(if matches!(command, UtilityCommand::Qr { .. }) {
                "qr"
            } else {
                "image"
            });
    std::fs::create_dir_all(&root)?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700))?;
    // Bound retained results; only our UUID-named files in this private cache.
    let mut files: Vec<_> = std::fs::read_dir(&root)?
        .flatten()
        .filter(|e| {
            e.file_type().is_ok_and(|t| t.is_file())
                && e.file_name()
                    .to_str()
                    .and_then(|n| n.get(..36))
                    .is_some_and(|id| uuid::Uuid::parse_str(id).is_ok())
        })
        .collect();
    files.sort_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());
    let excess = files.len().saturating_sub(16);
    for entry in files.into_iter().take(excess) {
        let _ = std::fs::remove_file(entry.path());
    }
    let encoded = format
        .map(|format| image_tools::encode(&source, format))
        .transpose()?;
    let decoded = if format == Some("jpg") {
        Some(image_tools::decode(encoded.as_deref().unwrap_or_default())?)
    } else {
        None
    };
    let displayed = decoded.as_ref().unwrap_or(&source);
    let id = uuid::Uuid::new_v4();
    let preview = root.join(format!("{id}-preview.png"));
    let thumb = Source {
        pixels: displayed.pixels.thumbnail(240, 160),
        format: "PNG",
        svg: None,
    };
    std::fs::write(&preview, image_tools::encode(&thumb, "png")?)?;
    let (artifact, copy_path) = if let Some(format) = format {
        let bytes = encoded.as_deref().unwrap_or_default();
        let artifact = root.join(format!("{id}.{format}"));
        std::fs::write(&artifact, bytes)?;
        let copy_path = if format == "png" {
            artifact.clone()
        } else {
            let copy = root.join(format!("{id}-copy.png"));
            std::fs::write(&copy, image_tools::encode(displayed, "png")?)?;
            copy
        };
        (Some(artifact), Some(copy_path))
    } else {
        (None, None)
    };
    let value = serde_json::json!({"format":source.format,"width":source.pixels.width(),"height":source.pixels.height(),"preview":preview,"artifact":artifact,"copyPath":copy_path});
    output.emit(&value, || value.to_string())
}
