//! Dashboard composition, six tabs, and source keyboard state.
use super::data::{self, StatsSummary};
use super::format::{format_cost, format_model_name, format_number};
use super::layout::{self, Border, BoxStyle, Paint};
use super::table::{self, Cell, Table};
use crate::modes::interactive::theme::ThemeColor;
use pi_js_compat::js_json::JsString;

pub const RANGES: [&str; 4] = ["1d", "7d", "30d", "All"];
pub const RANGE_LABELS: [&str; 4] = ["Today", "Last 7 days", "Last 30 days", "All time"];
const TABS: [&str; 6] = ["Overview", "Languages", "Models", "Projects", "Skills", "Usage"];

pub struct View<'a> {
    pub summary: &'a StatsSummary,
    pub tab: usize,
    pub range: usize,
    pub focus: usize,
    pub scroll: usize,
    pub marquee_tick: usize,
    pub marquee: &'a std::cell::Cell<bool>,
    pub rows: usize,
    pub all_empty: bool,
    pub update: Option<&'a JsString>,
    pub paint: &'a dyn Paint,
}
impl View<'_> {
    pub fn render(&self, width: usize) -> Vec<JsString> {
        let paint = self.paint;
        let inner = width.saturating_sub(4).max(1);
        let mut children = vec![tab_bar(self.tab, inner, paint)];
        children.extend(layout::text(
            paint.fg(ThemeColor::BorderMuted, "─".repeat(inner.max(60)).into()),
            inner,
        ));
        if self.all_empty || self.summary.session_count == 0.0 {
            children.push(" ".repeat(inner).into());
            let message = if self.all_empty {
                "No sessions found in ~/.pi/agent/sessions"
            } else {
                "No data for this time range"
            };
            children.extend(pi_tui::tui::Component::render(
                &mut pi_tui::components::text::Text::new(paint.fg(ThemeColor::Muted, message.into()), 1, 0),
                inner,
            ));
            children.push(" ".repeat(inner).into());
        } else {
            let height = self.content_height();
            children.extend(if self.tab == 0 {
                overview(self.summary, &RANGES[self.range].into(), height, inner, paint)
            } else {
                self.render_table_tab(height, inner)
            });
        }
        children.extend(layout::text(
            paint.fg(ThemeColor::BorderMuted, "─".repeat(inner.max(60)).into()),
            inner,
        ));
        children.extend(layout::text(
            paint.fg(ThemeColor::Dim, "Esc/q close  ←→ tabs  r range  ↑↓ scroll".into()),
            inner,
        ));
        let style = BoxStyle {
            titles: vec![
                (
                    paint.bold("Pi Atlas".into()) + paint.fg(ThemeColor::Dim, " · v0.2.1".into()),
                    false,
                ),
                (
                    paint.fg(ThemeColor::Accent, RANGE_LABELS[self.range].into())
                        + " "
                        + paint.fg(ThemeColor::Muted, "[r]".into()),
                    true,
                ),
            ],
            footers: self
                .update
                .map(|update| vec![(paint.fg(ThemeColor::Muted, paint.italic(update.clone())), true)])
                .unwrap_or_default(),
            color: Some(ThemeColor::Text),
            padding: 1,
            top: 1,
            ..BoxStyle::default()
        };
        layout::border_box(&style, children, width, paint)
    }

    pub fn content_height(&self) -> usize {
        ((self.rows as f64 * 0.8).floor() as usize).saturating_sub(5).max(5)
    }

    fn render_table_tab(&self, height: usize, width: usize) -> Vec<JsString> {
        let paint = self.paint;
        let description = &TABLES[self.tab - 1];
        let title = description.title;
        let rows = rows_for_tab(self.summary, self.tab, paint);
        let mut result = if self.tab == 5 {
            token_cards(self.summary, width, paint)
        } else {
            Vec::new()
        };
        let titles = if rows.is_empty() {
            vec![(
                if self.tab <= 2 {
                    title.into()
                } else {
                    paint.bold(title.into())
                },
                false,
            )]
        } else {
            vec![
                (paint.bold(title.into()), false),
                (paint.fg(ThemeColor::Muted, description.subtitle.into()), true),
            ]
        };
        let style = BoxStyle {
            color: Some(ThemeColor::Border),
            titles,
            ..BoxStyle::default()
        };
        let children = if rows.is_empty() {
            pi_tui::tui::Component::render(
                &mut pi_tui::components::text::Text::new(paint.fg(ThemeColor::Muted, description.empty.into()), 1, 1),
                width.saturating_sub(2).max(1),
            )
        } else {
            let table = Table {
                columns: table::columns(description.columns),
                rows,
                sort_column: description.sort_column,
                height: if self.tab == 5 {
                    height.saturating_sub(4).max(3)
                } else {
                    height
                },
                focus: self.focus,
                scroll: self.scroll,
                marquee_tick: self.marquee_tick,
            };
            let width = width.saturating_sub(2).max(1);
            self.marquee.set(table.focused_overflows(width));
            table.render(width, paint)
        };
        result.extend(layout::border_box(&style, children, width, paint));
        result
    }
}

fn tab_bar(active: usize, width: usize, paint: &dyn Paint) -> JsString {
    let line = JsString::join(
        TABS.iter().enumerate().map(|(index, label)| {
            let label = format!(" {label} ").into();
            if index == active {
                paint.bg(paint.fg(ThemeColor::Accent, label))
            } else {
                paint.fg(ThemeColor::Muted, label)
            }
        }),
        " ",
    );
    let vis_len = pi_js_compat::js_regex::JsRegex::new(r"\x1b\[[0-9;]*m|<[^>]+>")
        .expect("constant tab length regex")
        .replace_all(line.clone(), "")
        .len();
    if vis_len > width { line.slice(0, width) } else { line }
}

fn kpi_cards(summary: &StatsSummary, width: usize, paint: &dyn Paint) -> Vec<JsString> {
    let widths: Vec<_> = [17, 17, 17, 17, 17, 15].map(|pct| width * pct / 100).into();
    let values = [
        ("Total cost", format_cost(summary.total_cost), ThemeColor::Success),
        ("Tokens", format_number(summary.total_tokens, false), ThemeColor::Error),
        (
            "Messages",
            format_number(summary.total_messages, false),
            ThemeColor::BorderAccent,
        ),
        (
            "Sessions",
            format_number(summary.session_count, false),
            ThemeColor::Accent,
        ),
        (
            "Active days",
            format_number(summary.days_active, false),
            ThemeColor::Warning,
        ),
        ("Avg/Day", format_cost(summary.avg_cost_per_day), ThemeColor::Border),
    ];
    layout::grid(
        values
            .into_iter()
            .zip(&widths)
            .map(|((label, value, color), width)| layout::stat_card(label, value, color, *width, paint))
            .collect(),
        &widths,
    )
}

fn overview(summary: &StatsSummary, range: &JsString, height: usize, width: usize, paint: &dyn Paint) -> Vec<JsString> {
    let kpi_style = BoxStyle {
        padding: 1,
        ..BoxStyle::default()
    };
    let mut result = layout::border_box(
        &kpi_style,
        kpi_cards(summary, width.saturating_sub(4).max(1), paint),
        width,
        paint,
    );
    let chart_height = height
        .saturating_sub(kpi_cards(summary, 80, paint).len() + top_cards(summary, 80, paint).len())
        .min(18);
    let chart_style = BoxStyle {
        color: Some(ThemeColor::Border),
        padding: 1,
        top: 1,
        titles: vec![(paint.bold("Cost overtime".into()), false)],
        ..BoxStyle::default()
    };
    result.extend(layout::border_box(
        &chart_style,
        super::chart::render(summary, range, chart_height, width.saturating_sub(4).max(1), paint),
        width,
        paint,
    ));
    result.extend(top_cards(summary, width, paint));
    result
}

fn top_cards(summary: &StatsSummary, width: usize, paint: &dyn Paint) -> Vec<JsString> {
    let widths = [width * 33 / 100, width * 33 / 100, width * 34 / 100];
    let language = summary.languages.first();
    let model = summary.models.first();
    let project = summary.projects.first();
    let style = |title: &str, footer: Option<JsString>| BoxStyle {
        border: Border::Heavy,
        padding: 1,
        titles: vec![(paint.bold(title.into()), false)],
        footers: footer
            .map(|footer| vec![(paint.fg(ThemeColor::Muted, footer), true)])
            .unwrap_or_default(),
        ..BoxStyle::default()
    };
    let mut language_style = style(
        "Top Language",
        language.map(|language| format_number(language.lines, false) + " ln"),
    );
    language_style.palette = language.map(|language| (language.language.clone(), false));
    language_style.color = language.is_none().then_some(ThemeColor::BorderMuted);
    let mut model_style = style("Top model", model.map(|model| format_cost(model.cost)));
    model_style.palette = model.map(|model| (model.provider.clone(), true));
    model_style.color = model.is_none().then_some(ThemeColor::BorderMuted);
    let project_style = BoxStyle {
        color: Some(ThemeColor::Text),
        ..style("Top project", project.map(|project| format_cost(project.cost)))
    };
    let text = |value: Option<JsString>, width: usize| {
        layout::text(
            paint.fg(
                if value.is_some() {
                    ThemeColor::Text
                } else {
                    ThemeColor::Dim
                },
                value.unwrap_or_else(|| "No data".into()),
            ),
            width.saturating_sub(4).max(1),
        )
    };
    layout::grid(
        vec![
            layout::border_box(
                &language_style,
                text(language.map(|item| item.language.clone()), widths[0]),
                widths[0],
                paint,
            ),
            layout::border_box(
                &model_style,
                text(model.map(|item| format_model_name(&item.model)), widths[1]),
                widths[1],
                paint,
            ),
            layout::border_box(
                &project_style,
                text(project.map(|item| item.project.clone()), widths[2]),
                widths[2],
                paint,
            ),
        ],
        &widths,
    )
}

fn token_cards(summary: &StatsSummary, width: usize, paint: &dyn Paint) -> Vec<JsString> {
    let inner = width.saturating_sub(4).max(1);
    let widths = [inner / 4; 4];
    let values = [
        ("Input", summary.total_input_tokens),
        ("Output", summary.total_output_tokens),
        ("Cache Read", summary.total_cache_read_tokens),
        ("Cache Write", summary.total_cache_write_tokens),
    ];
    let cards = layout::grid(
        values
            .into_iter()
            .enumerate()
            .map(|(index, (label, value))| {
                layout::stat_card(
                    label,
                    format_number(value, false),
                    ThemeColor::Accent,
                    widths[index],
                    paint,
                )
            })
            .collect(),
        &widths,
    );
    let style = BoxStyle {
        border: Border::Single,
        color: Some(ThemeColor::Border),
        titles: vec![
            (paint.bold("Tokens".into()), false),
            (
                paint.fg(ThemeColor::Muted, format_number(summary.total_tokens, false)),
                true,
            ),
        ],
        padding: 1,
        ..BoxStyle::default()
    };
    layout::border_box(&style, cards, width, paint)
}

struct TableDescription {
    title: &'static str,
    subtitle: &'static str,
    empty: &'static str,
    columns: &'static [(&'static str, Option<usize>)],
    sort_column: usize,
}
const TABLES: [TableDescription; 5] = [
    TableDescription {
        title: "Languages",
        subtitle: "by lines written",
        empty: "No language data for this time range",
        columns: &[
            ("Name", None),
            ("Share %", Some(20)),
            ("Edits", Some(10)),
            ("Lines", Some(12)),
        ],
        sort_column: 3,
    },
    TableDescription {
        title: "Models",
        subtitle: "by cost",
        empty: "No model data for this time range",
        columns: &[
            ("Name", None),
            ("Provider", Some(20)),
            ("Cost %", Some(14)),
            ("Calls", Some(10)),
            ("Cost", Some(10)),
        ],
        sort_column: 4,
    },
    TableDescription {
        title: "Projects",
        subtitle: "by cost",
        empty: "No projects data for this time range",
        columns: &[
            ("Name", None),
            ("Share %", Some(14)),
            ("Sessions", Some(10)),
            ("Cost", Some(10)),
        ],
        sort_column: 3,
    },
    TableDescription {
        title: "Skills",
        subtitle: "by cost",
        empty: "No skill usage data for this time range",
        columns: &[
            ("Name", None),
            ("Calls", Some(7)),
            ("Sess.", Some(7)),
            ("Tokens", Some(10)),
            ("↑In", Some(6)),
            ("↓Out", Some(6)),
            ("Cost", Some(12)),
        ],
        sort_column: 6,
    },
    TableDescription {
        title: "Tools",
        subtitle: "by calls",
        empty: "No tools data for this time range",
        columns: &[("Command", None), ("Share %", Some(20)), ("Calls", Some(12))],
        sort_column: 2,
    },
];
fn muted(paint: &dyn Paint, value: JsString) -> Cell {
    Cell::Text(paint.fg(ThemeColor::Muted, value))
}
fn cost_cell(paint: &dyn Paint, value: f64) -> Cell {
    Cell::Text(if value > 0.0 {
        paint.bold(format_cost(value))
    } else {
        paint.fg(ThemeColor::Dim, "Free".into())
    })
}
fn rows_for_tab(summary: &StatsSummary, tab: usize, paint: &dyn Paint) -> Vec<Vec<Cell>> {
    match tab {
        1 => language_rows(summary, paint),
        2 => model_rows(summary, paint),
        3 => project_rows(summary, paint),
        4 => skill_rows(summary, paint),
        _ => tool_rows(summary, paint),
    }
}
fn language_rows(summary: &StatsSummary, paint: &dyn Paint) -> Vec<Vec<Cell>> {
    let max = summary.languages.iter().fold(0.0_f64, |max, item| max.max(item.lines));
    summary
        .languages
        .iter()
        .map(|item| {
            vec![
                Cell::Marquee(item.language.clone()),
                Cell::Bar(percent(item.lines, max), Some((item.language.clone(), false))),
                muted(paint, format_number(item.edits, false)),
                Cell::Text(paint.bold(format_number(item.lines, false))),
            ]
        })
        .collect()
}
fn model_rows(summary: &StatsSummary, paint: &dyn Paint) -> Vec<Vec<Cell>> {
    let max = summary.models.iter().fold(0.0_f64, |max, item| max.max(item.cost));
    summary
        .models
        .iter()
        .map(|item| {
            vec![
                Cell::Marquee(format_model_name(&item.model)),
                muted(paint, item.provider.clone()),
                Cell::Bar(percent(item.cost, max), Some((item.provider.clone(), true))),
                muted(paint, format_number(item.calls, false)),
                cost_cell(paint, item.cost),
            ]
        })
        .collect()
}
fn project_rows(summary: &StatsSummary, paint: &dyn Paint) -> Vec<Vec<Cell>> {
    let max = summary.projects.iter().fold(0.0_f64, |max, item| max.max(item.cost));
    summary
        .projects
        .iter()
        .map(|item| {
            vec![
                Cell::Marquee(item.project.clone()),
                Cell::Bar(percent(item.cost, max), None),
                muted(paint, format_number(item.sessions, false)),
                cost_cell(paint, item.cost),
            ]
        })
        .collect()
}
fn skill_rows(summary: &StatsSummary, paint: &dyn Paint) -> Vec<Vec<Cell>> {
    summary
        .skills
        .iter()
        .map(|item| {
            let price = data::number(&item.usage["cost"], "total");
            let price = if price > 0.0 {
                paint.bold(format_cost(price))
            } else {
                paint.fg(ThemeColor::Dim, format_cost(0.0))
            };
            vec![
                Cell::Text(item.name.clone()),
                muted(paint, format_number(item.calls, false)),
                muted(paint, format_number(item.sessions, false)),
                Cell::Text(format_number(data::number(&item.usage, "totalTokens"), false)),
                muted(paint, format_number(data::number(&item.usage, "input"), true)),
                muted(paint, format_number(data::number(&item.usage, "output"), true)),
                Cell::Text(price),
            ]
        })
        .collect()
}
fn tool_rows(summary: &StatsSummary, paint: &dyn Paint) -> Vec<Vec<Cell>> {
    let max = summary.tools.iter().fold(0.0_f64, |max, item| max.max(item.count));
    summary
        .tools
        .iter()
        .map(|item| {
            vec![
                Cell::Marquee(strip_ansi(&item.name).slice(0, 120)),
                Cell::Bar(percent(item.count, max), None),
                Cell::Text(paint.bold(format_number(item.count, false))),
            ]
        })
        .collect()
}
fn percent(value: f64, max: f64) -> f64 {
    if max > 0.0 { value / max * 100.0 } else { 0.0 }
}
fn strip_ansi(value: &JsString) -> JsString {
    let text = pi_js_compat::js_regex::JsRegex::new(r"(?:\u001B\][\s\S]*?(?:\u0007|\u001B\\|\u009C))|[\u001B\u009B][[\]()#;?]*(?:\d{1,4}(?:[;:]\d{0,4})*)?[\dA-PR-TZcf-nq-uy=><~]").expect("constant ANSI regex").replace_all(value.clone(), "");
    JsString::from_utf16(
        text.units()
            .iter()
            .copied()
            .filter(|unit| !matches!(unit, 0..=8 | 10..=31 | 127 | 0x200b..=0x200f | 0x2028..=0x2029 | 0xfeff))
            .collect(),
    )
}

#[cfg(test)]
fn render_dashboard(summary: &StatsSummary, tab: usize, width: usize, rows: usize) -> Vec<JsString> {
    View {
        summary,
        tab,
        range: 3,
        focus: 0,
        scroll: 0,
        marquee_tick: 0,
        marquee: &std::cell::Cell::new(false),
        rows,
        all_empty: false,
        update: Some(&"Last update : 1:30 PM".into()),
        paint: &layout::Plain,
    }
    .render(width)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pi_js_compat::js_json;
    #[test]
    fn empty_all_and_empty_range_match_the_source() -> Result<(), JsString> {
        let fixture =
            js_json::parse_lossless(include_str!("fixtures/empty.json")).map_err(|error| error.into_message())?;
        let summary: StatsSummary = js_json::from_value(&fixture["empty"]).unwrap();
        for frame in fixture["frames"].as_array().unwrap() {
            let view = View {
                summary: &summary,
                tab: 0,
                range: 0,
                focus: 0,
                scroll: 0,
                marquee_tick: 0,
                marquee: &std::cell::Cell::new(false),
                rows: 50,
                all_empty: frame["allEmpty"].as_bool().unwrap(),
                update: None,
                paint: &layout::Plain,
            };
            assert_lines(
                &view.render(100),
                &js_json::from_value::<Vec<JsString>>(&frame["lines"]).unwrap(),
            );
        }
        Ok(())
    }

    fn assert_lines(actual: &[JsString], expected: &[JsString]) {
        assert_eq!(actual.len(), expected.len(), "line count");
        for (row, (actual, expected)) in actual.iter().zip(expected).enumerate() {
            assert_eq!(
                js_json::stringify_lossless(&js_json::JsValue::String(actual.clone())),
                js_json::stringify_lossless(&js_json::JsValue::String(expected.clone())),
                "row {row}"
            );
        }
    }

    #[test]
    fn ansi_dashboard_matches_the_frozen_theme() -> Result<(), JsString> {
        use crate::modes::interactive::theme::{ColorMode, ColorValue, Theme};
        let colors: Vec<_> = [
            "accent",
            "muted",
            "dim",
            "text",
            "borderMuted",
            "border",
            "borderAccent",
            "success",
            "error",
            "warning",
        ]
        .map(|name| (JsString::from(name), ColorValue::Text("#abcdef".into())))
        .into();
        let theme = Theme::new(
            &colors,
            &[("selectedBg".into(), ColorValue::Text("#102030".into()))],
            ColorMode::Truecolor,
            None,
            None,
        )
        .map_err(|error| JsString::from(error.to_string()))?;
        let domain =
            js_json::parse_lossless(include_str!("fixtures/domain.json")).map_err(|error| error.into_message())?;
        let render =
            js_json::parse_lossless(include_str!("fixtures/themed.json")).map_err(|error| error.into_message())?;
        let summary_case = domain["summaries"]
            .as_array()
            .expect("summaries")
            .iter()
            .find(|case| case["range"].as_string().is_some_and(|range| range == "All") && case["filters"].is_null())
            .expect("all");
        let summary: StatsSummary =
            js_json::from_value(&summary_case["expected"]).map_err(|error| JsString::from(error.to_string()))?;
        for case in render["dashboard"].as_array().expect("dashboard") {
            let view = View {
                summary: &summary,
                tab: case["tab"].as_u64().expect("tab") as usize,
                range: 3,
                focus: 0,
                scroll: 0,
                marquee_tick: 0,
                marquee: &std::cell::Cell::new(false),
                rows: 50,
                all_empty: false,
                update: Some(&"Last update : 1:30 PM".into()),
                paint: &theme,
            };
            let actual = view.render(case["width"].as_u64().expect("width") as usize);
            let expected: Vec<JsString> =
                js_json::from_value(&case["lines"]).map_err(|error| JsString::from(error.to_string()))?;
            assert_lines(&actual, &expected);
        }
        let loading = super::super::loading::render(
            &super::super::cache::LoadingProgress {
                total: 12,
                done: 3,
                pct: 25.0,
                remaining_time_ms: Some(12345.0),
            },
            100,
            &theme,
        );
        assert_lines(
            &loading,
            &js_json::from_value::<Vec<JsString>>(&render["loading"])
                .map_err(|error| JsString::from(error.to_string()))?,
        );
        Ok(())
    }

    #[test]
    fn six_dashboard_tabs_match_snapshot_at_three_widths() -> Result<(), JsString> {
        let domain =
            js_json::parse_lossless(include_str!("fixtures/domain.json")).map_err(|error| error.into_message())?;
        let render =
            js_json::parse_lossless(include_str!("fixtures/render.json")).map_err(|error| error.into_message())?;
        let summary_case = domain["summaries"]
            .as_array()
            .expect("summaries")
            .iter()
            .find(|case| case["range"].as_string().is_some_and(|range| range == "All") && case["filters"].is_null())
            .expect("all");
        let summary: StatsSummary =
            js_json::from_value(&summary_case["expected"]).map_err(|error| JsString::from(error.to_string()))?;
        for case in render["dashboard"].as_array().expect("dashboard") {
            let actual = render_dashboard(
                &summary,
                case["tab"].as_u64().expect("tab") as usize,
                case["width"].as_u64().expect("width") as usize,
                50,
            );
            let expected: Vec<JsString> =
                js_json::from_value(&case["lines"]).map_err(|error| JsString::from(error.to_string()))?;
            assert_lines(&actual, &expected);
        }
        Ok(())
    }
}
