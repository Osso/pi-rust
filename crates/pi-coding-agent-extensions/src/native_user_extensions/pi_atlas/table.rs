//! SortedTable displays pre-sorted domain rows; input moves focus and scroll.
use super::layout::Paint;
use crate::modes::interactive::theme::ThemeColor;
use pi_js_compat::{js_format, js_json::JsString, number::math_round};
use pi_tui::utils::truncate_to_width_with;

pub enum Cell {
    Text(JsString),
    Marquee(JsString),
    Bar(f64, Option<(JsString, bool)>),
}
impl Cell {
    fn render(&self, width: usize, focused: bool, tick: usize) -> JsString {
        match self {
            Self::Text(text) => truncate_to_width_with(text.clone(), width, "", false),
            Self::Marquee(text) if focused && text.len() > width => {
                let extended = text.clone() + "     ";
                let offset = tick % extended.len();
                (extended.clone() + extended).slice(offset, offset + width)
            }
            Self::Marquee(text) => truncate_to_width_with(text.clone(), width, "…", false),
            Self::Bar(pct, palette) => {
                let filled = math_round(pct.clamp(0.0, 100.0) / 100.0 * width as f64) as usize;
                let fill = "■".repeat(filled).into();
                let fill = match palette {
                    Some((name, provider)) => super::palette::paint(name, *provider, fill),
                    None => fill,
                };
                fill + " ".repeat(width.saturating_sub(filled))
            }
        }
    }
}

pub struct Column {
    pub header: &'static str,
    /// None is the single fill column. Atlas does not use percentage columns.
    pub width: Option<usize>,
}

pub struct Table {
    pub columns: Vec<Column>,
    pub rows: Vec<Vec<Cell>>,
    pub sort_column: usize,
    pub height: usize,
    pub focus: usize,
    pub scroll: usize,
    pub marquee_tick: usize,
}
impl Table {
    fn widths(&self, width: usize) -> Vec<usize> {
        let content = width.saturating_sub(self.columns.len().saturating_sub(1));
        let fixed: usize = self.columns.iter().filter_map(|col| col.width).sum();
        let mut widths: Vec<_> = self
            .columns
            .iter()
            .map(|col| col.width.unwrap_or_else(|| content.saturating_sub(fixed).max(1)))
            .collect();
        let total: usize = widths.iter().sum();
        if total > content {
            let mut used = 0;
            for (index, _) in self.columns.iter().enumerate().filter(|(_, col)| col.width.is_some()) {
                let scaled = (widths[index] as f64 * content as f64 / total as f64).floor() as usize;
                widths[index] = scaled.max(1);
                used += widths[index];
            }
            if let Some(index) = self.columns.iter().position(|col| col.width.is_none()) {
                widths[index] = content.saturating_sub(used).max(1);
            }
        }
        widths
    }

    pub fn focused_overflows(&self, width: usize) -> bool {
        let widths = self.widths(width);
        self.rows.get(self.focus).is_some_and(|row| {
            row.iter()
                .zip(widths)
                .any(|(cell, width)| matches!(cell, Cell::Marquee(text) if text.len() > width))
        })
    }

    pub fn render(&self, width: usize, paint: &dyn Paint) -> Vec<JsString> {
        let widths = self.widths(width);
        let header = JsString::join(
            self.columns.iter().enumerate().map(|(index, col)| {
                let indicator = if index == self.sort_column { " ▼" } else { "" };
                let content = widths[index].saturating_sub(indicator.chars().count());
                let text = truncate_to_width_with(col.header, content, "", false) + indicator;
                truncate_to_width_with(text, widths[index], "", true) + " "
            }),
            "",
        )
        .trim_end();
        let header = truncate_to_width_with(js_format!("  {}", header), width, "", true);
        let mut result = vec![paint.bold(paint.fg(ThemeColor::Accent, header))];
        let visible = self.height.saturating_sub(1).max(1);
        let scroll = self.scroll.min(self.rows.len().saturating_sub(visible));
        for (index, row) in self.rows.iter().enumerate().skip(scroll).take(visible) {
            let focused = index == self.focus;
            let line = JsString::join(
                row.iter().zip(&widths).map(|(cell, width)| {
                    truncate_to_width_with(cell.render(*width, focused, self.marquee_tick), *width, "", true)
                }),
                " ",
            );
            let prefix = if focused { "▌ " } else { "  " };
            let line = JsString::from(prefix) + truncate_to_width_with(line, width.saturating_sub(2), "", true);
            result.push(if focused { paint.bg(line) } else { line });
        }
        result
    }
}

pub fn columns(headers: &[(&'static str, Option<usize>)]) -> Vec<Column> {
    headers
        .iter()
        .map(|(header, width)| Column { header, width: *width })
        .collect()
}
