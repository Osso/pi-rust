//! The used pi-tui-extras border/grid behavior, layered on native pi-tui.
use crate::modes::interactive::theme::{Theme, ThemeBg, ThemeColor};
use pi_js_compat::{js_format, js_json::JsString};
use pi_tui::{tui::Component, utils::visible_width};

pub trait Paint {
    fn fg(&self, color: ThemeColor, text: JsString) -> JsString;
    fn bg(&self, text: JsString) -> JsString;
    fn bold(&self, text: JsString) -> JsString;
    fn italic(&self, text: JsString) -> JsString;
}
impl Paint for Theme {
    fn fg(&self, color: ThemeColor, text: JsString) -> JsString {
        self.fg(color, text)
    }
    fn bg(&self, text: JsString) -> JsString {
        self.bg(ThemeBg::SelectedBg, text)
    }
    fn bold(&self, text: JsString) -> JsString {
        self.bold(text)
    }
    fn italic(&self, text: JsString) -> JsString {
        self.italic(text)
    }
}

#[cfg(test)]
pub struct Plain;
#[cfg(test)]
impl Paint for Plain {
    fn fg(&self, _: ThemeColor, text: JsString) -> JsString {
        text
    }
    fn bg(&self, text: JsString) -> JsString {
        text
    }
    fn bold(&self, text: JsString) -> JsString {
        text
    }
    fn italic(&self, text: JsString) -> JsString {
        text
    }
}

pub fn text(text: impl Into<JsString>, width: usize) -> Vec<JsString> {
    pi_tui::components::text::Text::new(text, 0, 0).render(width)
}

pub fn pad(line: JsString, width: usize) -> JsString {
    let remaining = width.saturating_sub(visible_width(&line));
    line + " ".repeat(remaining)
}

pub fn truncate(mut text: JsString, width: usize) -> JsString {
    if visible_width(&text) <= width {
        return text;
    }
    while !text.is_empty() && visible_width(&(text.clone() + "…")) > width {
        text = text.slice(0, text.len() - 1);
    }
    text + "…"
}

pub fn align_lr(left: JsString, right: JsString, width: usize, fill: &JsString) -> JsString {
    let middle = width.saturating_sub(visible_width(&left) + visible_width(&right));
    left + JsString::join(std::iter::repeat_n(fill.clone(), middle), "") + right
}

#[derive(Clone, Copy)]
pub enum Border {
    Single,
    Rounded,
    Heavy,
}

pub struct BoxStyle {
    pub border: Border,
    pub color: Option<ThemeColor>,
    pub palette: Option<(JsString, bool)>,
    pub titles: Vec<(JsString, bool)>,
    pub footers: Vec<(JsString, bool)>,
    pub padding: usize,
    pub top: usize,
}
impl Default for BoxStyle {
    fn default() -> Self {
        Self {
            border: Border::Rounded,
            color: None,
            palette: None,
            titles: Vec::new(),
            footers: Vec::new(),
            padding: 0,
            top: 0,
        }
    }
}

fn border_line(
    style: &BoxStyle,
    titles: &[(JsString, bool)],
    width: usize,
    bottom: bool,
    paint: &dyn Paint,
) -> JsString {
    let (top_l, top_r, bottom_l, bottom_r, horizontal, _) = chars(style.border);
    let (left, right) = if bottom { (bottom_l, bottom_r) } else { (top_l, top_r) };
    let color = |s: &str| paint_border(style, s, paint);
    let horizontal = color(horizontal);
    let inner = width.saturating_sub(2).max(1);
    let content = match titles {
        [] => JsString::join(std::iter::repeat_n(horizontal.clone(), inner), ""),
        [(title, align_right)] => {
            let title = truncate(title.clone(), inner.saturating_sub(4).max(1));
            let decor = if inner < 4 {
                js_format!(
                    "{}{}{}",
                    horizontal,
                    truncate(title, inner.saturating_sub(2).max(1)),
                    horizontal
                )
            } else {
                js_format!("{} {} {}", horizontal, title, horizontal)
            };
            let fill = JsString::join(
                std::iter::repeat_n(horizontal.clone(), inner.saturating_sub(visible_width(&decor))),
                "",
            );
            if *align_right { fill + decor } else { decor + fill }
        }
        [(left, _), (right, _)] => {
            let left = truncate(left.clone(), inner.saturating_sub(6).max(1));
            let left_decor = js_format!("{} {} {}", horizontal, left, horizontal);
            let right = truncate(
                right.clone(),
                inner.saturating_sub(visible_width(&left_decor) + 4).max(1),
            );
            align_lr(
                left_decor,
                js_format!("{} {} {}", horizontal, right, horizontal),
                inner,
                &horizontal,
            )
        }
        _ => unreachable!("atlas has at most two border titles"),
    };
    pad(js_format!("{}{}{}", color(left), content, color(right)), width)
}

fn chars(
    style: Border,
) -> (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
) {
    match style {
        Border::Single => ("┌", "┐", "└", "┘", "─", "│"),
        Border::Rounded => ("╭", "╮", "╰", "╯", "─", "│"),
        Border::Heavy => ("┏", "┓", "┗", "┛", "━", "┃"),
    }
}

fn paint_border(style: &BoxStyle, text: &str, paint: &dyn Paint) -> JsString {
    if let Some((name, provider)) = &style.palette {
        return super::palette::paint(name, *provider, text.into());
    }
    style
        .color
        .map_or_else(|| text.into(), |color| paint.fg(color, text.into()))
}

pub fn border_box(style: &BoxStyle, children: Vec<JsString>, width: usize, paint: &dyn Paint) -> Vec<JsString> {
    let inner_width = width.saturating_sub(2).max(1);
    let child_width = inner_width.saturating_sub(style.padding * 2).max(1);
    let vertical = chars(style.border).5;
    let vertical: JsString = paint_border(style, vertical, paint);
    let content_line = |line: JsString| pad(js_format!("{}{}{}", vertical, line, vertical), width);
    let mut result = vec![border_line(style, &style.titles, width, false, paint)];
    result.extend((0..style.top).map(|_| content_line(" ".repeat(inner_width).into())));
    if children.is_empty() {
        result.push(content_line(" ".repeat(inner_width).into()));
    }
    for child in children {
        let fill = child_width.saturating_sub(visible_width(&child)) + style.padding;
        result.push(content_line(js_format!(
            "{}{}{}",
            " ".repeat(style.padding),
            child,
            " ".repeat(fill)
        )));
    }
    result.push(border_line(style, &style.footers, width, true, paint));
    result
}

pub fn grid(children: Vec<Vec<JsString>>, widths: &[usize]) -> Vec<JsString> {
    let height = children.iter().map(Vec::len).max().unwrap_or(0);
    (0..height)
        .map(|row| {
            JsString::join(
                children
                    .iter()
                    .enumerate()
                    .map(|(index, lines)| lines.get(row).cloned().unwrap_or_default().pad_end(widths[index])),
                "",
            )
        })
        .collect()
}

pub fn stat_card(label: &str, value: JsString, color: ThemeColor, width: usize, paint: &dyn Paint) -> Vec<JsString> {
    let mut result = text(label, width);
    result.extend(text(paint.fg(color, paint.bold(value)), width));
    result
}
