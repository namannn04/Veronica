use base64::Engine as _;

pub(crate) const MAX_ART_BYTES: usize = 2 * 1024 * 1024;

pub(crate) fn to_data_url(url: &str) -> Option<String> {
    if url.starts_with("data:") {
        return Some(url.to_string());
    }
    let path = local_path(url)?;
    let bytes = std::fs::read(&path).ok()?;
    if bytes.is_empty() || bytes.len() > MAX_ART_BYTES {
        return None;
    }
    let mime = sniff_image(&bytes)?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Some(format!("data:{mime};base64,{encoded}"))
}

pub(crate) fn local_path(url: &str) -> Option<String> {
    let rest = url.strip_prefix("file://")?;
    let path = if let Some(stripped) = rest.strip_prefix("localhost/") {
        format!("/{stripped}")
    } else if rest.starts_with('/') {
        rest.to_string()
    } else {
        return None;
    };
    Some(percent_decode(&path))
}

pub(crate) fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).ok();
            if let Some(value) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(value);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub(crate) fn sniff_image(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some("image/png");
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("image/jpeg");
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("image/gif");
    }
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    if bytes.starts_with(b"<svg") || bytes.starts_with(b"<?xml") {
        return Some("image/svg+xml");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_a_local_path_from_a_file_url() {
        assert_eq!(
            local_path("file:///tmp/.com.google.Chrome.37mMBI").as_deref(),
            Some("/tmp/.com.google.Chrome.37mMBI")
        );
        assert_eq!(
            local_path("file://localhost/tmp/art.png").as_deref(),
            Some("/tmp/art.png")
        );
    }

    #[test]
    fn refuses_a_remote_host_and_other_schemes() {
        assert_eq!(local_path("file://someserver/share/art.png"), None);
        assert_eq!(local_path("https://example.com/art.png"), None);
        assert_eq!(local_path("/tmp/art.png"), None);
    }

    #[test]
    fn decodes_escapes_so_paths_with_spaces_resolve() {
        assert_eq!(percent_decode("/tmp/my%20art.png"), "/tmp/my art.png");
        assert_eq!(percent_decode("/tmp/a%2Bb.png"), "/tmp/a+b.png");
        assert_eq!(percent_decode("/tmp/100%.png"), "/tmp/100%.png");
        assert_eq!(percent_decode("/tmp/%ZZ.png"), "/tmp/%ZZ.png");
    }

    #[test]
    fn sniffs_the_formats_players_actually_write() {
        assert_eq!(
            sniff_image(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0]),
            Some("image/png")
        );
        assert_eq!(sniff_image(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("image/jpeg"));
        assert_eq!(sniff_image(b"GIF89a..."), Some("image/gif"));
        let mut webp = b"RIFF".to_vec();
        webp.extend_from_slice(&[0, 0, 0, 0]);
        webp.extend_from_slice(b"WEBP");
        assert_eq!(sniff_image(&webp), Some("image/webp"));
    }

    #[test]
    fn refuses_content_that_is_not_an_image() {
        assert_eq!(sniff_image(b""), None);
        assert_eq!(sniff_image(b"not an image at all"), None);
        assert_eq!(sniff_image(b"RIFF"), None);
    }

    #[test]
    fn a_data_url_passes_straight_through() {
        let url = "data:image/png;base64,AAAA";
        assert_eq!(to_data_url(url).as_deref(), Some(url));
    }

    #[test]
    fn a_missing_file_yields_no_art_rather_than_an_error() {
        assert_eq!(to_data_url("file:///nonexistent/art.png"), None);
    }

    #[test]
    fn inlines_a_real_png_from_disk() {
        let dir = std::env::temp_dir().join(format!("veronica-art-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("art");
        let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        bytes.extend_from_slice(&[1, 2, 3, 4]);
        std::fs::write(&path, &bytes).unwrap();

        let url = format!("file://{}", path.display());
        let data = to_data_url(&url).expect("art should inline");
        assert!(data.starts_with("data:image/png;base64,"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_oversized_file_is_skipped() {
        let dir = std::env::temp_dir().join(format!("veronica-big-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("big");
        let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        bytes.resize(MAX_ART_BYTES + 1, 0);
        std::fs::write(&path, &bytes).unwrap();
        assert_eq!(to_data_url(&format!("file://{}", path.display())), None);
        std::fs::remove_dir_all(&dir).ok();
    }
}
