use anyhow::{Context, Result};
use serde::Serialize;

use crate::aggregate::{Dashboard, DayPoint, HeatmapCell};

pub const FOOTER_URL: &str = "github.com/namannn04/veronica";

pub const WIDTH: u32 = 1200;
pub const HEIGHT: u32 = 630;
pub const SCALE: f32 = 2.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Card {
    Highlights,
    Activity,
    Daily,
    Busiest,
}

impl Card {
    pub const ALL: [Self; 4] = [Self::Highlights, Self::Activity, Self::Daily, Self::Busiest];

    pub fn key(self) -> &'static str {
        match self {
            Self::Highlights => "highlights",
            Self::Activity => "activity",
            Self::Daily => "daily",
            Self::Busiest => "busiest",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Highlights => "Agent usage",
            Self::Activity => "Activity",
            Self::Daily => "Daily rhythm",
            Self::Busiest => "Busiest day",
        }
    }

    pub fn parse(raw: &str) -> Option<Vec<Self>> {
        if raw.eq_ignore_ascii_case("all") {
            return Some(Self::ALL.to_vec());
        }
        Self::ALL
            .into_iter()
            .find(|card| card.key().eq_ignore_ascii_case(raw))
            .map(|card| vec![card])
    }
}

const INK: &str = "#f2f2f4";
const MUTED: &str = "#8b8b96";
const PLANE: &str = "#141418";
const RAISED: &str = "#1c1c22";
const HAIRLINE: &str = "#2a2a32";
const ACCENT: &str = "#7bd6a8";

const LEVELS: [&str; 5] = ["#20202a", "#1f4f3a", "#2c7a55", "#45a874", ACCENT];

pub fn tokens(value: u64) -> String {
    const UNITS: [(u64, &str); 4] = [
        (1_000_000_000_000, "T"),
        (1_000_000_000, "B"),
        (1_000_000, "M"),
        (1_000, "K"),
    ];
    let decimals = |scaled: f64| -> usize {
        if scaled >= 100.0 {
            0
        } else if scaled >= 10.0 {
            1
        } else {
            2
        }
    };
    let round = |scaled: f64, places: usize| {
        let factor = 10f64.powi(places as i32);
        (scaled * factor).round() / factor
    };
    for (index, (scale, suffix)) in UNITS.into_iter().enumerate() {
        if value >= scale {
            let scaled = value as f64 / scale as f64;
            let rounded = round(scaled, decimals(scaled));
            if rounded >= 1000.0 && index > 0 {
                return format!("1.00{}", UNITS[index - 1].1);
            }
            return format!("{rounded:.*}{suffix}", decimals(rounded));
        }
    }
    value.to_string()
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn text(x: f64, y: f64, size: f64, weight: &str, fill: &str, body: &str) -> String {
    format!(
        r#"<text x="{x}" y="{y}" font-family="Noto Sans, DejaVu Sans, Ubuntu, sans-serif" font-size="{size}" font-weight="{weight}" fill="{fill}">{}</text>"#,
        escape(body)
    )
}

fn chrome(card: Card, subtitle: &str, body: &str) -> String {
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" height="{HEIGHT}" viewBox="0 0 {WIDTH} {HEIGHT}">
<rect width="{WIDTH}" height="{HEIGHT}" fill="{PLANE}"/>
<rect x="40" y="40" width="{inner_width}" height="{inner_height}" rx="24" fill="{RAISED}" stroke="{HAIRLINE}"/>
<circle cx="88" cy="96" r="18" fill="{ACCENT}"/>
{mark}
{title}
{subtitle}
{body}
<line x1="72" y1="{footer_line}" x2="{footer_end}" y2="{footer_line}" stroke="{HAIRLINE}"/>
{footer}
</svg>"#,
        inner_width = WIDTH - 80,
        inner_height = HEIGHT - 80,
        mark = text(81.0, 103.0, 20.0, "700", PLANE, "V"),
        title = text(120.0, 104.0, 26.0, "650", INK, card.title()),
        subtitle = text(120.0, 128.0, 15.0, "400", MUTED, subtitle),
        footer_line = HEIGHT - 100,
        footer_end = WIDTH - 72,
        footer = text(72.0, f64::from(HEIGHT - 68), 14.0, "400", MUTED, FOOTER_URL),
    )
}

const CONTENT_LEFT: f64 = 72.0;
const CONTENT_TOP: f64 = 170.0;
const CONTENT_BOTTOM: f64 = 470.0;
const CONTENT_WIDTH: f64 = WIDTH as f64 - CONTENT_LEFT * 2.0;

fn tile(x: f64, y: f64, value: &str, label: &str) -> String {
    format!(
        "{}\n{}",
        text(x, y, 54.0, "700", INK, value),
        text(x, y + 28.0, 14.0, "500", MUTED, label)
    )
}

pub fn render_svg(card: Card, dashboard: &Dashboard, range_label: &str) -> String {
    match card {
        Card::Highlights => highlights(dashboard, range_label),
        Card::Activity => activity(dashboard, range_label),
        Card::Daily => daily(dashboard, range_label),
        Card::Busiest => busiest(dashboard, range_label),
    }
}

fn highlights(dashboard: &Dashboard, range_label: &str) -> String {
    let totals = &dashboard.totals;
    let column = CONTENT_WIDTH / 4.0;
    let mut body = String::new();

    let headline = [
        (tokens(totals.tokens), "TOKENS"),
        (dashboard.active_days.to_string(), "ACTIVE DAYS"),
        (dashboard.session_count.to_string(), "SESSIONS"),
        (dashboard.by_model.len().to_string(), "MODELS"),
    ];
    for (index, (value, label)) in headline.iter().enumerate() {
        body.push_str(&tile(
            CONTENT_LEFT + index as f64 * column,
            CONTENT_TOP + 90.0,
            value,
            label,
        ));
        body.push('\n');
    }

    body.push_str(&format!(
        r#"<line x1="{CONTENT_LEFT}" y1="{rule}" x2="{right}" y2="{rule}" stroke="{HAIRLINE}"/>"#,
        rule = CONTENT_TOP + 150.0,
        right = f64::from(WIDTH) - CONTENT_LEFT,
    ));
    let detail = [
        ("IN", totals.input_tokens),
        ("OUT", totals.output_tokens),
        ("CACHE WRITE", totals.cache_creation_tokens),
        ("CACHE READ", totals.cache_read_tokens),
    ];
    for (index, (label, value)) in detail.iter().enumerate() {
        let x = CONTENT_LEFT + index as f64 * column;
        body.push_str(&text(x, CONTENT_BOTTOM - 40.0, 13.0, "500", MUTED, label));
        body.push('\n');
        body.push_str(&text(x, CONTENT_BOTTOM, 28.0, "650", INK, &tokens(*value)));
        body.push('\n');
    }
    chrome(Card::Highlights, range_label, &body)
}

fn activity(dashboard: &Dashboard, range_label: &str) -> String {
    const ROWS: usize = 7;
    const GAP: f64 = 4.0;
    const MIN_CELL: f64 = 11.0;
    const MAX_CELL: f64 = 24.0;
    const MIN_COLUMNS: usize = 26;

    let grid_height = 200.0;
    let mut body = String::new();

    let max_columns = ((CONTENT_WIDTH + GAP) / (MIN_CELL + GAP)).floor() as usize;
    let capacity = max_columns * ROWS;
    let cells: &[HeatmapCell] = if dashboard.heatmap.len() > capacity {
        &dashboard.heatmap[dashboard.heatmap.len() - capacity..]
    } else {
        &dashboard.heatmap
    };

    let columns = cells.len().div_ceil(ROWS).max(MIN_COLUMNS);
    let padding = columns * ROWS - cells.len();
    let cell = (((CONTENT_WIDTH + GAP) / columns as f64) - GAP).clamp(MIN_CELL, MAX_CELL);
    let grid_width = columns as f64 * (cell + GAP) - GAP;
    let left = CONTENT_LEFT + (CONTENT_WIDTH - grid_width).max(0.0) / 2.0;
    let top = CONTENT_TOP + (grid_height - (ROWS as f64 * (cell + GAP) - GAP)) / 2.0;

    let radius = (cell / 4.0).min(4.0);
    for slot in 0..columns * ROWS {
        let x = left + (slot / ROWS) as f64 * (cell + GAP);
        let y = top + (slot % ROWS) as f64 * (cell + GAP);
        let fill = match slot.checked_sub(padding).and_then(|index| cells.get(index)) {
            Some(entry) => LEVELS[(entry.level as usize).min(LEVELS.len() - 1)],
            None => LEVELS[0],
        };
        body.push_str(&format!(
            r#"<rect x="{x:.1}" y="{y:.1}" width="{cell:.1}" height="{cell:.1}" rx="{radius:.1}" fill="{fill}"/>"#
        ));
        body.push('\n');
    }

    let active = cells.iter().filter(|entry| entry.tokens > 0).count();
    let total: u64 = cells.iter().map(|entry| entry.tokens).sum();
    body.push_str(&tile(
        CONTENT_LEFT,
        CONTENT_BOTTOM,
        &active.to_string(),
        "DAYS WITH ACTIVITY",
    ));
    body.push('\n');
    body.push_str(&tile(
        CONTENT_LEFT + CONTENT_WIDTH / 4.0,
        CONTENT_BOTTOM,
        &tokens(total),
        "TOKENS IN THIS WINDOW",
    ));
    body.push('\n');

    let legend_cell = 13.0;
    let legend_left = f64::from(WIDTH) - CONTENT_LEFT - 190.0;
    body.push_str(&text(
        legend_left,
        CONTENT_BOTTOM,
        13.0,
        "400",
        MUTED,
        "Less",
    ));
    for (index, colour) in LEVELS.iter().enumerate() {
        let x = legend_left + 42.0 + index as f64 * (legend_cell + 4.0);
        body.push_str(&format!(
            r#"<rect x="{x:.1}" y="{y:.1}" width="{legend_cell}" height="{legend_cell}" rx="3" fill="{colour}"/>"#,
            y = CONTENT_BOTTOM - 11.0
        ));
    }
    body.push('\n');
    body.push_str(&text(
        legend_left + 42.0 + LEVELS.len() as f64 * (legend_cell + 4.0) + 6.0,
        CONTENT_BOTTOM,
        13.0,
        "400",
        MUTED,
        "More",
    ));

    chrome(Card::Activity, range_label, &body)
}

fn daily(dashboard: &Dashboard, range_label: &str) -> String {
    let days: Vec<&DayPoint> = dashboard.days.iter().rev().take(60).rev().collect();
    let peak = days.iter().map(|day| day.tokens).max().unwrap_or(0).max(1);

    let mut body = String::new();
    let plot_bottom = CONTENT_BOTTOM - 40.0;
    let plot_height = 210.0;
    let width = CONTENT_WIDTH / days.len().max(1) as f64;
    let bar = (width - 4.0).max(2.0);

    for (index, day) in days.iter().enumerate() {
        let height = (day.tokens as f64 / peak as f64 * plot_height).max(2.0);
        let x = CONTENT_LEFT + index as f64 * width;
        let y = plot_bottom - height;
        body.push_str(&format!(
            r#"<rect x="{x:.1}" y="{y:.1}" width="{bar:.1}" height="{height:.1}" rx="2" fill="{ACCENT}"/>"#
        ));
        body.push('\n');
    }

    body.push_str(&format!(
        r#"<line x1="{CONTENT_LEFT}" y1="{plot_bottom}" x2="{right}" y2="{plot_bottom}" stroke="{HAIRLINE}"/>"#,
        right = f64::from(WIDTH) - CONTENT_LEFT
    ));
    body.push('\n');
    body.push_str(&text(
        CONTENT_LEFT,
        CONTENT_BOTTOM,
        13.0,
        "400",
        MUTED,
        days.first().map_or("", |day| day.period.as_str()),
    ));
    body.push('\n');
    body.push_str(&text(
        f64::from(WIDTH) - CONTENT_LEFT - 88.0,
        CONTENT_BOTTOM,
        13.0,
        "400",
        MUTED,
        days.last().map_or("", |day| day.period.as_str()),
    ));
    body.push_str(&format!(
        "\n{}",
        text(
            CONTENT_LEFT,
            CONTENT_TOP,
            14.0,
            "500",
            MUTED,
            &format!("PEAK {} TOKENS IN A DAY", tokens(peak))
        )
    ));

    chrome(Card::Daily, range_label, &body)
}

fn busiest(dashboard: &Dashboard, range_label: &str) -> String {
    let best = dashboard
        .days
        .iter()
        .max_by_key(|day| day.tokens)
        .cloned()
        .unwrap_or_default();

    let mut active: Vec<u64> = dashboard
        .days
        .iter()
        .filter(|day| day.tokens > 0)
        .map(|day| day.tokens)
        .collect();
    active.sort_unstable_by(|left, right| right.cmp(left));
    let rank = active
        .iter()
        .position(|value| *value == best.tokens)
        .map_or(1, |index| index + 1);
    let share = if dashboard.totals.tokens > 0 {
        best.tokens as f64 / dashboard.totals.tokens as f64 * 100.0
    } else {
        0.0
    };

    let body = format!(
        "{}\n{}\n{}\n{}\n{}",
        text(
            CONTENT_LEFT,
            CONTENT_TOP + 20.0,
            18.0,
            "500",
            MUTED,
            &best.period
        ),
        text(
            CONTENT_LEFT,
            CONTENT_TOP + 110.0,
            82.0,
            "700",
            INK,
            &tokens(best.tokens)
        ),
        text(
            CONTENT_LEFT,
            CONTENT_TOP + 142.0,
            15.0,
            "500",
            MUTED,
            "TOKENS IN ONE DAY"
        ),
        tile(
            CONTENT_LEFT,
            CONTENT_BOTTOM,
            &format!("#{rank}"),
            &format!("OF {} ACTIVE DAYS", active.len())
        ),
        tile(
            CONTENT_LEFT + CONTENT_WIDTH / 4.0,
            CONTENT_BOTTOM,
            &format!("{share:.0}%"),
            "OF EVERY TOKEN IN RANGE"
        ),
    );
    chrome(Card::Busiest, range_label, &body)
}

pub fn render_png(svg: &str) -> Result<Vec<u8>> {
    let mut fonts = resvg::usvg::fontdb::Database::new();
    fonts.load_system_fonts();
    if fonts.is_empty() {
        anyhow::bail!(
            "no fonts are installed, so a card would render as empty boxes. \
             Install fonts-noto-core, or export the SVG with --svg."
        );
    }

    let options = resvg::usvg::Options {
        fontdb: std::sync::Arc::new(fonts),
        ..resvg::usvg::Options::default()
    };
    let tree =
        resvg::usvg::Tree::from_str(svg, &options).context("the card produced invalid SVG")?;

    let width = (WIDTH as f32 * SCALE) as u32;
    let height = (HEIGHT as f32 * SCALE) as u32;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)
        .context("cannot allocate the card's pixels")?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(SCALE, SCALE),
        &mut pixmap.as_mut(),
    );
    pixmap.encode_png().context("cannot encode the card as PNG")
}

pub fn file_name(card: Card, stamp: &str, extension: &str) -> String {
    format!("veronica-usage-{}-{stamp}.{extension}", card.key())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aggregate::{HeatmapCell, NamedAmount, ProjectRollup};
    use crate::models::Totals;

    fn dashboard() -> Dashboard {
        Dashboard {
            totals: Totals {
                cost: 179.78,
                tokens: 502_000_260,
                input_tokens: 12_000,
                output_tokens: 8_000,
                cache_creation_tokens: 900,
                cache_read_tokens: 480_000_000,
            },
            days: vec![
                DayPoint {
                    period: "2026-08-30".into(),
                    cost: 5.0,
                    tokens: 1_000,
                },
                DayPoint {
                    period: "2026-08-31".into(),
                    cost: 90.0,
                    tokens: 400_000,
                },
                DayPoint {
                    period: "2026-09-01".into(),
                    cost: 3.0,
                    tokens: 500,
                },
            ],
            by_model: vec![NamedAmount {
                name: "opus & sonnet".into(),
                ..Default::default()
            }],
            by_source: vec![],
            by_hour: vec![],
            heatmap: (0..40)
                .map(|index| HeatmapCell {
                    period: format!("2026-07-{:02}", index % 28 + 1),
                    cost: 1.0,
                    tokens: index as u64 * 10,
                    level: (index % 5) as u8,
                })
                .collect(),
            projects: vec![ProjectRollup {
                project_name: "secret-client-work".into(),
                repository_id: Some("github.com/acme/secret".into()),
                repository_url: Some("https://github.com/acme/secret".into()),
                path: "/home/me/clients/acme".into(),
                cost: 100.0,
                tokens: 1_000,
                chats: vec![],
            }],
            active_days: 3,
            session_count: 41,
        }
    }

    #[test]
    fn no_card_can_leak_a_project_repository_path_or_dollar_cost() {
        let dashboard = dashboard();
        for card in Card::ALL {
            let svg = render_svg(card, &dashboard, "all time");
            for secret in [
                "secret-client-work",
                "github.com/acme",
                "/home/me/clients",
                "acme",
            ] {
                assert!(!svg.contains(secret), "{} leaked {secret:?}", card.key());
            }
            assert!(!svg.contains('$'), "{} shows a dollar cost", card.key());
            assert!(
                !svg.contains("179.78"),
                "{} shows a cost figure",
                card.key()
            );
        }
    }

    #[test]
    fn every_card_carries_the_project_link_and_its_own_title() {
        let dashboard = dashboard();
        for card in Card::ALL {
            let svg = render_svg(card, &dashboard, "all time");
            assert!(svg.contains(FOOTER_URL), "{} has no footer", card.key());
            assert!(svg.contains(card.title()), "{} has no title", card.key());
            assert!(svg.contains("all time"), "{} has no range", card.key());
        }
    }

    #[test]
    fn a_model_name_with_an_ampersand_does_not_break_the_svg() {
        let svg = render_svg(Card::Highlights, &dashboard(), "a & b");
        assert!(svg.contains("a &amp; b"));
        assert!(!svg.contains("a & b"));
    }

    #[test]
    fn highlights_shows_the_totals_it_is_for() {
        let svg = render_svg(Card::Highlights, &dashboard(), "all time");
        assert!(svg.contains("502M"), "the token total");
        assert!(svg.contains(">41<"), "the session count");
        assert!(svg.contains("ACTIVE DAYS"));
    }

    #[test]
    fn the_busiest_card_ranks_the_day_against_the_others() {
        let svg = render_svg(Card::Busiest, &dashboard(), "all time");
        assert!(svg.contains("2026-08-31"), "the busiest day itself");
        assert!(svg.contains("400K"), "its token count");
        assert!(svg.contains("#1"), "and its rank");
    }

    #[test]
    fn a_dashboard_with_no_days_still_renders_rather_than_panicking() {
        let empty = Dashboard::default();
        for card in Card::ALL {
            let svg = render_svg(card, &empty, "all time");
            assert!(svg.starts_with("<svg"), "{} produced nothing", card.key());
            assert!(svg.ends_with("</svg>"));
        }
    }

    #[test]
    fn the_calendar_shows_the_most_recent_cells_when_history_overflows_the_card() {
        let mut wide = dashboard();
        wide.heatmap = (0..5_000)
            .map(|index| HeatmapCell {
                period: format!("day-{index}"),
                cost: 0.0,
                tokens: if index == 4_999 { 999 } else { 0 },
                level: if index == 4_999 { 4 } else { 0 },
            })
            .collect();
        let svg = render_svg(Card::Activity, &wide, "all time");
        assert!(svg.contains(LEVELS[4]), "the newest cell was dropped");
    }

    #[test]
    fn cards_parse_by_name_and_all_expands() {
        assert_eq!(Card::parse("activity"), Some(vec![Card::Activity]));
        assert_eq!(Card::parse("ACTIVITY"), Some(vec![Card::Activity]));
        assert_eq!(Card::parse("all"), Some(Card::ALL.to_vec()));
        assert_eq!(Card::parse("nonsense"), None);
        assert_eq!(Card::parse(""), None);
    }

    #[test]
    fn a_file_name_is_timestamped_so_a_second_export_does_not_replace_the_first() {
        let first = file_name(Card::Activity, "2026-09-01-001500", "png");
        let second = file_name(Card::Activity, "2026-09-01-001600", "png");
        assert_ne!(first, second);
        assert!(first.starts_with("veronica-usage-activity-"));
        assert!(first.ends_with(".png"));
    }

    #[test]
    fn tokens_abbreviate_with_three_significant_figures() {
        assert_eq!(tokens(999), "999");
        assert_eq!(tokens(1_050), "1.05K");
        assert_eq!(tokens(15_000), "15.0K");
        assert_eq!(tokens(502_000_260), "502M");
    }

    #[test]
    fn every_card_rasterises_to_a_real_png() {
        let dashboard = dashboard();
        for card in Card::ALL {
            let png = render_png(&render_svg(card, &dashboard, "all time"))
                .unwrap_or_else(|error| panic!("{} did not render: {error:#}", card.key()));
            assert_eq!(&png[1..4], b"PNG", "{} is not a PNG", card.key());
            assert!(
                png.len() > 2_000,
                "{} rendered suspiciously small",
                card.key()
            );
        }
    }

    #[test]
    fn a_short_history_is_padded_into_a_full_calendar() {
        let mut short = dashboard();
        short.heatmap = (0..21)
            .map(|index| HeatmapCell {
                period: format!("2026-08-{:02}", index + 1),
                cost: 1.0,
                tokens: 100,
                level: 3,
            })
            .collect();
        let svg = render_svg(Card::Activity, &short, "all time");
        let cells = svg.matches("<rect x=").count();
        assert!(cells >= 26 * 7, "only {cells} cells were drawn");
        assert!(svg.contains(">21<"), "the count came from the padding");
    }
}
