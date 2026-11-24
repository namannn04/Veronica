use std::borrow::Cow;
use std::io::Write;

const EXPECTED: &str = "__TAURI_INVOKE_KEY__ expected ";
const RECEIVED: &str = " but received ";
const SHOWN_CHARACTERS: usize = 48;

pub(crate) fn init() {
    let filter = std::env::var("VERONICA_LOG").unwrap_or_else(|_| "warn,veronica=info".to_string());
    tracing_subscriber::fmt()
        .with_writer(|| Redacted(std::io::stderr()))
        .with_env_filter(filter)
        .init();
}

pub(crate) fn redact(line: &str) -> Cow<'_, str> {
    let Some(start) = line.find(EXPECTED) else {
        return Cow::Borrowed(line);
    };
    let rest = &line[start + EXPECTED.len()..];
    let received: String = rest
        .find(RECEIVED)
        .map_or("", |at| &rest[at + RECEIVED.len()..])
        .chars()
        .filter(|character| !character.is_control())
        .take(SHOWN_CHARACTERS)
        .collect();
    Cow::Owned(format!(
        "{}refused an invoke with the wrong key, received {received}\n",
        &line[..start]
    ))
}

struct Redacted<W>(W);

impl<W: Write> Write for Redacted<W> {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        let text = String::from_utf8_lossy(buffer);
        self.0.write_all(redact(&text).as_bytes())?;
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_expected_invoke_key_never_reaches_the_log() {
        let line = "2026-11-24T12:00:00Z ERROR tauri::webview: __TAURI_INVOKE_KEY__ expected s3cretKey but received guess-1\n";
        assert_eq!(
            redact(line),
            "2026-11-24T12:00:00Z ERROR tauri::webview: refused an invoke with the wrong key, received guess-1\n"
        );
        let mut log = Vec::new();
        Redacted(&mut log).write_all(line.as_bytes()).unwrap();
        let written = String::from_utf8(log).unwrap();
        assert!(!written.contains("s3cretKey") && !written.contains("__TAURI_INVOKE_KEY__"));
        assert!(written.ends_with("received guess-1\n"));
    }

    #[test]
    fn hostile_received_values_are_trimmed_and_other_lines_pass_through() {
        let forged = format!(
            "__TAURI_INVOKE_KEY__ expected key but received a\u{1b}[2J\nb{}",
            "x".repeat(200)
        );
        let shown = redact(&forged);
        assert!(shown.starts_with("refused an invoke with the wrong key, received a[2Jbxxx"));
        assert_eq!(shown.lines().count(), 1);
        assert!(shown.len() < 120);
        assert!(!redact("__TAURI_INVOKE_KEY__ expected key").contains("key\n"));
        let plain = "INFO veronica: started\n";
        assert!(matches!(redact(plain), Cow::Borrowed(text) if text == plain));
    }
}
