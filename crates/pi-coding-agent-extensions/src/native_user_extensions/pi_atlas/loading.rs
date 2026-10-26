use super::{cache::LoadingProgress, layout::Paint};
use pi_js_compat::js_json::JsString;

pub fn render(progress: &LoadingProgress, width: usize, paint: &dyn Paint) -> Vec<JsString> {
    use super::{
        format,
        layout::{self, Border, BoxStyle},
    };
    use crate::modes::interactive::theme::ThemeColor;
    let inner = width.saturating_sub(4).max(1);
    let remaining = progress
        .remaining_time_ms
        .map(|time| {
            let time = if time < 60_000.0 {
                pi_js_compat::js_format!("~{}s", format::format_number(time / 1000.0, false))
            } else {
                format!(
                    "~{}m {}s",
                    (time / 60_000.0).floor(),
                    pi_js_compat::number::math_round(time % 60_000.0 / 1000.0)
                )
                .into()
            };
            paint.fg(ThemeColor::Dim, time + " remaining · ")
        })
        .unwrap_or_default();
    let count = if progress.total > 0 {
        JsString::from(progress.done.to_string()) + paint.fg(ThemeColor::Dim, format!("/{}", progress.total).into())
    } else {
        JsString::default()
    };
    let right = if count.is_empty() {
        JsString::default()
    } else {
        remaining + count
    };
    let label = layout::align_lr(
        paint.fg(ThemeColor::Text, "Parsing session logs...".into()),
        right,
        inner,
        &" ".into(),
    );
    let filled = pi_js_compat::number::math_round(progress.pct.clamp(0.0, 100.0) / 100.0 * inner as f64) as usize;
    let bar = paint.bold(paint.fg(ThemeColor::Success, "█".repeat(filled).into()))
        + paint.fg(ThemeColor::Muted, "░".repeat(inner.saturating_sub(filled)).into());
    let mut children = layout::text(label, inner);
    children.extend(layout::text(bar, inner));
    children.push(" ".repeat(inner).into());
    children.extend(layout::text(
        paint.fg(ThemeColor::Dim, "Esc/q to cancel and close".into()),
        inner,
    ));
    let style = BoxStyle {
        border: Border::Single,
        padding: 1,
        top: 1,
        titles: vec![(
            paint.bold("Pi Atlas".into()) + paint.fg(ThemeColor::Dim, " · v0.2.1".into()),
            false,
        )],
        ..BoxStyle::default()
    };
    layout::border_box(&style, children, width, paint)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pi_js_compat::js_json;
    #[test]
    fn loading_overlay_matches_frozen_progress() -> Result<(), JsString> {
        let fixture =
            js_json::parse_lossless(include_str!("fixtures/render.json")).map_err(|error| error.into_message())?;
        let expected: Vec<JsString> =
            js_json::from_value(&fixture["loading"]).map_err(|error| JsString::from(error.to_string()))?;
        let progress = LoadingProgress {
            total: 12,
            done: 3,
            pct: 25.0,
            remaining_time_ms: Some(12345.0),
        };
        assert_eq!(render(&progress, 100, &super::super::layout::Plain), expected);
        Ok(())
    }
}
