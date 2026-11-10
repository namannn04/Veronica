//! Local, bounded image conversion. SVG exports wrap raster pixels; no tracing.
use anyhow::{bail, ensure, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use image::{DynamicImage, ImageFormat, ImageReader};
use std::io::Cursor;

pub const MAX_BYTES: usize = 20 * 1024 * 1024;
const MAX_PIXELS: u64 = 16_000_000;

pub struct Source {
    pub pixels: DynamicImage,
    pub format: &'static str,
    pub svg: Option<Vec<u8>>,
}

pub fn decode(bytes: &[u8]) -> Result<Source> {
    ensure!(bytes.len() <= MAX_BYTES, "Image must be at most 20 MiB");
    if let Ok(format) = image::guess_format(bytes) {
        ensure!(
            matches!(format, ImageFormat::Png | ImageFormat::Jpeg),
            "Choose PNG, JPG/JPEG or SVG"
        );
        let reader = ImageReader::with_format(Cursor::new(bytes), format);
        let (width, height) = reader.into_dimensions()?;
        check_size(width, height)?;
        let pixels = image::load_from_memory_with_format(bytes, format)?;
        return Ok(Source {
            pixels,
            svg: None,
            format: if format == ImageFormat::Png {
                "PNG"
            } else {
                "JPG"
            },
        });
    }
    // No filesystem or network image resolution for untrusted SVG input.
    let mut options = resvg::usvg::Options::default();
    options.image_href_resolver.resolve_string = Box::new(|_, _| None);
    options.fontdb_mut().load_system_fonts();
    let tree = resvg::usvg::Tree::from_data(bytes, &options)
        .context("Choose a valid PNG, JPG/JPEG or SVG image")?;
    let size = tree.size().to_int_size();
    check_size(size.width(), size.height())?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size.width(), size.height())
        .context("Cannot allocate image")?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::default(),
        &mut pixmap.as_mut(),
    );
    // tiny-skia encodes straight-alpha PNG, avoiding premultiplied color fringes.
    let png = pixmap.encode_png()?;
    Ok(Source {
        pixels: image::load_from_memory(&png)?,
        format: "SVG",
        svg: Some(
            tree.to_string(&resvg::usvg::WriteOptions::default())
                .into_bytes(),
        ),
    })
}

fn check_size(width: u32, height: u32) -> Result<()> {
    ensure!(
        width > 0 && height > 0 && u64::from(width) * u64::from(height) <= MAX_PIXELS,
        "Image must contain at most 16 million pixels"
    );
    Ok(())
}

pub fn encode(source: &Source, format: &str) -> Result<Vec<u8>> {
    let mut bytes = Cursor::new(Vec::new());
    match format {
        "png" => source.pixels.write_to(&mut bytes, ImageFormat::Png)?,
        "jpg" => {
            let mut rgb = source.pixels.to_rgb8();
            for (out, pixel) in rgb.pixels_mut().zip(source.pixels.to_rgba8().pixels()) {
                let alpha = u32::from(pixel[3]);
                for c in 0..3 {
                    out[c] =
                        ((u32::from(pixel[c]) * alpha + 255 * (255 - alpha) + 127) / 255) as u8;
                }
            }
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 92)
                .encode_image(&DynamicImage::ImageRgb8(rgb))?;
        }
        "svg" => {
            if let Some(svg) = &source.svg {
                return Ok(svg.clone());
            }
            let png = STANDARD.encode(encode(source, "png")?);
            return Ok(format!(r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{}" height="{}" viewBox="0 0 {} {}"><image width="100%" height="100%" xlink:href="data:image/png;base64,{}"/></svg>"#, source.pixels.width(), source.pixels.height(), source.pixels.width(), source.pixels.height(), png).into_bytes());
        }
        _ => bail!("Unsupported output format"),
    }
    Ok(bytes.into_inner())
}

fn qr_code(link: &str) -> Result<qrcode::QrCode> {
    let link = link.trim();
    ensure!(!link.is_empty(), "Enter a link first");
    ensure!(
        !link.chars().any(char::is_control),
        "Link contains control characters"
    );
    qrcode::QrCode::new(link.as_bytes()).context("Link is too long for a QR code")
}
pub fn qr(link: &str) -> Result<Vec<u8>> {
    Ok(qr_code(link)?
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(320, 320)
        .quiet_zone(true)
        .build()
        .into_bytes())
}
/// QR pixels do not require SVG parsing or scanning all installed system fonts.
pub fn qr_source(link: &str) -> Result<Source> {
    let code = qr_code(link)?;
    let width = code.width();
    let scale = 320_usize.div_ceil(width + 8);
    let side = ((width + 8) * scale) as u32;
    let mut pixels = image::GrayImage::from_pixel(side, side, image::Luma([255]));
    for y in 0..width {
        for x in 0..width {
            if code[(x, y)] == qrcode::Color::Dark {
                for dy in 0..scale {
                    for dx in 0..scale {
                        pixels.put_pixel(
                            ((x + 4) * scale + dx) as u32,
                            ((y + 4) * scale + dy) as u32,
                            image::Luma([0]),
                        );
                    }
                }
            }
        }
    }
    Ok(Source {
        pixels: DynamicImage::ImageLuma8(pixels),
        format: "PNG",
        svg: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn direct_qr_pixels_match_svg_rendering_without_font_discovery() {
        let direct = qr_source("https://example.com/notch").unwrap();
        let svg = decode(&qr("https://example.com/notch").unwrap()).unwrap();
        assert_eq!(direct.pixels.to_luma8(), svg.pixels.to_luma8());
    }
    #[test]
    fn formats_and_alpha_roundtrip() {
        let source = Source {
            pixels: DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
                2,
                3,
                image::Rgba([255, 0, 0, 0]),
            )),
            format: "PNG",
            svg: None,
        };
        for (format, detected) in [("png", "PNG"), ("jpg", "JPG"), ("svg", "SVG")] {
            let decoded = decode(&encode(&source, format).unwrap()).unwrap();
            assert_eq!(decoded.format, detected);
            assert_eq!((decoded.pixels.width(), decoded.pixels.height()), (2, 3));
            if format == "jpg" {
                assert!(decoded.pixels.to_rgb8().get_pixel(0, 0)[1] > 245);
            }
        }
    }
    #[test]
    fn rejects_invalid_and_oversized_images() {
        assert!(decode(b"bad image").is_err());
        assert!(decode(
            br#"<svg xmlns="http://www.w3.org/2000/svg" width="100000" height="100000"/>"#
        )
        .is_err());
        assert!(decode(&vec![0; MAX_BYTES + 1]).is_err());
    }
    #[test]
    fn qr_has_quiet_zone_and_renders() {
        assert!(qr("  ").is_err());
        assert!(qr(&"x".repeat(10000)).is_err());
        let svg = qr("https://example.com/a?q=1&b=2").unwrap();
        let source = decode(&svg).unwrap();
        let pixels = source.pixels.to_rgb8();
        assert_eq!(pixels.get_pixel(0, 0).0, [255, 255, 255]);
        assert!(pixels.pixels().any(|pixel| pixel.0 == [0, 0, 0]));
    }
}
