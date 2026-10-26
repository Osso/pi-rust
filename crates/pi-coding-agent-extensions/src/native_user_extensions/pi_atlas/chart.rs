use super::{data::StatsSummary, format::format_cost, layout::Paint};
use crate::modes::interactive::theme::ThemeColor;
use pi_js_compat::{js_format, js_json::JsString};
use pi_tui::utils::visible_width;

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

#[derive(Clone)]
struct Bucket {
    date: JsString,
    hour: f64,
    cost: f64,
}

pub fn render(
    summary: &StatsSummary,
    range: &JsString,
    height: usize,
    width: usize,
    paint: &dyn Paint,
) -> Vec<JsString> {
    if summary.daily_spend.is_empty() && summary.hourly_spend.is_empty() {
        return vec![paint.fg(ThemeColor::Muted, "No data".into())];
    }
    let hourly = range == "1d" && summary.hourly_spend.len() == 24;
    let source: Vec<_> = if hourly {
        summary
            .hourly_spend
            .iter()
            .map(|item| Bucket {
                date: JsString::default(),
                hour: item.hour,
                cost: item.cost,
            })
            .collect()
    } else {
        summary
            .daily_spend
            .iter()
            .map(|item| Bucket {
                date: item.date.clone(),
                hour: 0.0,
                cost: item.cost,
            })
            .collect()
    };
    render_bars(&source, range, hourly, height, width, paint)
}

fn render_bars(
    source: &[Bucket],
    range: &JsString,
    hourly: bool,
    height: usize,
    width: usize,
    paint: &dyn Paint,
) -> Vec<JsString> {
    let area = height.saturating_sub(2).max(3);
    let max_cost = source.iter().fold(0.01_f64, |cost, bucket| cost.max(bucket.cost));
    let step = if area <= 6 {
        1
    } else if area <= 14 {
        2
    } else {
        3
    };
    let label_width = (0..area)
        .step_by(step)
        .map(|row| format_cost(row as f64 / (area - 1) as f64 * max_cost).len())
        .max()
        .unwrap_or(0);
    let available = width as isize - label_width as isize - 3;
    let max_bars = (available.div_euclid(3)).max(2) as usize;
    let plot = aggregate(source, max_bars);
    let column = ((available - plot.len() as isize).div_euclid(plot.len() as isize)).max(2) as usize;
    let mut result: Vec<_> = (0..area)
        .rev()
        .map(|row| {
            let label = if row == 0 || row % step == 0 {
                format_cost(row as f64 / (area - 1) as f64 * max_cost)
            } else {
                JsString::default()
            };
            let label = JsString::from(" ".repeat(label_width.saturating_sub(label.len()))) + label + " │ ";
            let bars = JsString::join(
                plot.iter().map(|item| {
                    let height = item.cost / max_cost * area as f64;
                    let symbol = if height > row as f64 + 0.5 {
                        "█"
                    } else if height > row as f64 {
                        "▄"
                    } else {
                        " "
                    };
                    symbol.repeat(column) + " "
                }),
                "",
            );
            label + bars
        })
        .collect();
    let labels = (0..plot.len()).map(|index| {
        let label = if hourly {
            hour_label(&plot, index)
        } else {
            date_label(&plot, index, range)
        };
        let remaining = (column + 1).saturating_sub(label.len());
        js_format!(
            "{}{}{}",
            "─".repeat(remaining / 2),
            label,
            "─".repeat(remaining - remaining / 2)
        )
    });
    result.push(JsString::from(" ".repeat(label_width + 1)) + "└─" + JsString::join(labels, ""));
    let granularity = if plot.len() == source.len() {
        if hourly { "Hourly".into() } else { "Daily".into() }
    } else {
        format!(
            "~{}{} avg",
            pi_js_compat::number::to_fixed(
                if hourly {
                    24.0 / plot.len() as f64
                } else {
                    source.len() as f64 / plot.len() as f64
                },
                1
            ),
            if hourly { "h" } else { "d" }
        )
        .into()
    };
    let granularity = paint.italic(granularity);
    result.push(paint.fg(
        ThemeColor::Dim,
        JsString::from(" ".repeat(width.saturating_sub(visible_width(&granularity)))) + granularity,
    ));
    result
}

fn aggregate(source: &[Bucket], target: usize) -> Vec<Bucket> {
    if source.len() <= target {
        return source.to_vec();
    }
    let quotient = source.len() / target;
    let remainder = source.len() % target;
    let mut offset = 0;
    (0..target)
        .map(|index| {
            let count = quotient + usize::from(index < remainder);
            let mut item = source[offset].clone();
            item.cost = source[offset..offset + count]
                .iter()
                .fold(0.0, |sum, item| sum + item.cost);
            offset += count;
            item
        })
        .collect()
}

fn hour_label(plot: &[Bucket], index: usize) -> JsString {
    let hour = plot[index].hour;
    if index == 0 || index == plot.len() - 1 || hour % 4.0 == 0.0 {
        format!("{}h", pi_js_compat::number::js_number_to_string(hour)).into()
    } else {
        JsString::default()
    }
}

fn date_label(plot: &[Bucket], index: usize, range: &JsString) -> JsString {
    use pi_js_compat::date::{date_parse_utf16, utc_date_fields};
    let millis = date_parse_utf16(js_format!("{}T00:00:00Z", plot[index].date).units()) as i64;
    let Some(date) = utc_date_fields(millis) else {
        return "NaN".into();
    };
    if range == "1d" || range == "7d" {
        let weekday = (millis.div_euclid(86_400_000) + 4).rem_euclid(7) as usize;
        return ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"][weekday].into();
    }
    if range == "30d" {
        return if date.day == 1 || date.day % 5 == 0 || index == 0 || index == plot.len() - 1 {
            date.day.to_string().into()
        } else {
            JsString::default()
        };
    }
    if index == 0 {
        return MONTHS[date.month as usize].into();
    }
    let previous = date_parse_utf16(js_format!("{}T00:00:00Z", plot[index - 1].date).units()) as i64;
    if utc_date_fields(previous).is_some_and(|previous| previous.month != date.month) {
        MONTHS[date.month as usize].into()
    } else {
        JsString::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pi_js_compat::js_json;
    #[test]
    fn hourly_and_bucketed_daily_charts_match_source() -> Result<(), JsString> {
        let fixture =
            js_json::parse_lossless(include_str!("fixtures/charts.json")).map_err(|error| error.into_message())?;
        for case in fixture.as_array().expect("cases") {
            let summary = StatsSummary {
                daily_spend: js_json::from_value(&case["daily"]).unwrap(),
                hourly_spend: js_json::from_value(&case["hourly"]).unwrap(),
                ..StatsSummary::default()
            };
            let actual = render(
                &summary,
                case["range"].as_string().unwrap(),
                case["height"].as_u64().unwrap() as usize,
                case["width"].as_u64().unwrap() as usize,
                &super::super::layout::Plain,
            );
            let expected: Vec<JsString> = js_json::from_value(&case["lines"]).unwrap();
            assert_eq!(actual, expected);
        }
        Ok(())
    }
}
