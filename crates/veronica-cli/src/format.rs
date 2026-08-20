//! Output formatting.
//!
//! Every read command supports `--json`, and Edith's contract is that stdout
//! carries exactly one JSON document while logs go to stderr. Honouring that
//! keeps `vr` drivable by scripts and agents.

use anyhow::Result;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Output {
    Text,
    Json,
}

impl Output {
    pub fn new(json: bool) -> Self {
        if json {
            Output::Json
        } else {
            Output::Text
        }
    }

    /// Emit one JSON document, or fall back to the supplied text renderer.
    pub fn emit<T: Serialize>(self, value: &T, text: impl FnOnce() -> String) -> Result<()> {
        match self {
            Output::Json => {
                println!("{}", serde_json::to_string_pretty(value)?);
            }
            Output::Text => {
                let rendered = text();
                if !rendered.is_empty() {
                    println!("{rendered}");
                }
            }
        }
        Ok(())
    }
}

/// Money, always two decimals with a leading dollar.
pub fn money(value: f64) -> String {
    format!("${value:.2}")
}

/// Large token counts, abbreviated the way the dashboard shows them.
pub fn tokens(value: u64) -> String {
    const UNITS: [(u64, &str); 4] = [
        (1_000_000_000_000, "T"),
        (1_000_000_000, "B"),
        (1_000_000, "M"),
        (1_000, "K"),
    ];
    for (scale, suffix) in UNITS {
        if value >= scale {
            let scaled = value as f64 / scale as f64;
            // Keep three significant figures so 1.05M and 10.5M both read well.
            return if scaled >= 100.0 {
                format!("{scaled:.0}{suffix}")
            } else if scaled >= 10.0 {
                format!("{scaled:.1}{suffix}")
            } else {
                format!("{scaled:.2}{suffix}")
            };
        }
    }
    value.to_string()
}

/// A duration as a compact countdown, e.g. "2h 14m" or "5d 3h".
///
pub fn countdown(seconds: i64) -> String {
    if seconds <= 0 {
        return "now".to_string();
    }
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m")
    } else {
        format!("{seconds}s")
    }
}

/// Render a simple aligned table. Columns are sized to their widest cell so
/// text output stays readable without a formatting dependency.
pub fn table(headers: &[&str], rows: &[Vec<String>]) -> String {
    if rows.is_empty() {
        return String::new();
    }
    let mut widths: Vec<usize> = headers.iter().map(|h| h.chars().count()).collect();
    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            if index < widths.len() {
                widths[index] = widths[index].max(cell.chars().count());
            }
        }
    }

    let mut out = String::new();
    let line = |cells: &[String], widths: &[usize]| -> String {
        cells
            .iter()
            .enumerate()
            .map(|(i, cell)| {
                let width = widths.get(i).copied().unwrap_or(0);
                // The last column needs no trailing padding.
                if i + 1 == cells.len() {
                    cell.clone()
                } else {
                    format!("{cell:<width$}")
                }
            })
            .collect::<Vec<_>>()
            .join("  ")
    };

    let head: Vec<String> = headers.iter().map(|h| h.to_uppercase()).collect();
    out.push_str(&line(&head, &widths));
    for row in rows {
        out.push('\n');
        out.push_str(&line(row, &widths));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_always_has_two_decimals() {
        assert_eq!(money(179.7819), "$179.78");
        assert_eq!(money(0.0), "$0.00");
        assert_eq!(money(5.0), "$5.00");
    }

    #[test]
    fn tokens_abbreviate_with_three_significant_figures() {
        assert_eq!(tokens(999), "999");
        assert_eq!(tokens(1_000), "1.00K");
        assert_eq!(tokens(1_050), "1.05K");
        assert_eq!(tokens(15_000), "15.0K");
        assert_eq!(tokens(502_000_260), "502M");
        assert_eq!(tokens(2_500_000_000), "2.50B");
    }

    #[test]
    fn countdown_picks_the_two_largest_useful_units() {
        assert_eq!(countdown(0), "now");
        assert_eq!(countdown(-5), "now");
        assert_eq!(countdown(45), "45s");
        assert_eq!(countdown(90), "1m");
        assert_eq!(countdown(8_040), "2h 14m");
        assert_eq!(countdown(444_000), "5d 3h");
    }

    #[test]
    fn table_aligns_columns_and_has_no_trailing_padding() {
        let rendered = table(
            &["name", "cost"],
            &[
                vec!["cli".into(), "$11.00".into()],
                vec!["commandcode".into(), "$1.00".into()],
            ],
        );
        let lines: Vec<&str> = rendered.lines().collect();
        assert_eq!(lines[0], "NAME         COST");
        assert_eq!(lines[1], "cli          $11.00");
        assert_eq!(lines[2], "commandcode  $1.00");
        for line in lines {
            assert_eq!(line, line.trim_end(), "no trailing whitespace");
        }
    }

    #[test]
    fn an_empty_table_renders_nothing_rather_than_a_bare_header() {
        assert_eq!(table(&["name"], &[]), "");
    }
}
