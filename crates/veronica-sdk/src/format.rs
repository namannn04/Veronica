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

#[cfg(test)]
mod tests {
    use super::table;

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
