pub fn money(value: f64) -> String {
    format!("${value:.2}")
}

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

#[cfg(test)]
mod tests {
    use super::{countdown, money, table};

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

    #[test]
    fn money_always_has_two_decimals() {
        assert_eq!(money(179.7819), "$179.78");
        assert_eq!(money(0.0), "$0.00");
        assert_eq!(money(5.0), "$5.00");
    }
}
