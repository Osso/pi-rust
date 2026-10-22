//! Ported from `packages/tui/src/components/truncated-text.ts`.

use pi_js_compat::js_json::JsString;

use crate::tui::Component;
use crate::utils::{truncate_to_width, visible_width};

pub struct TruncatedText {
    text: JsString,
    padding_x: usize,
    padding_y: usize,
}

impl TruncatedText {
    pub fn new(text: impl Into<JsString>, padding_x: usize, padding_y: usize) -> Self {
        Self {
            text: text.into(),
            padding_x,
            padding_y,
        }
    }

    pub fn text(&self) -> &JsString {
        &self.text
    }
}

impl Component for TruncatedText {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        let empty_line = JsString::from(" ".repeat(width));
        let mut result = Vec::new();
        for _ in 0..self.padding_y {
            result.push(empty_line.clone());
        }

        let available_width = width.saturating_sub(self.padding_x * 2).max(1);
        let single_line_text = self.text.split('\n').next().unwrap_or_default();
        let display_text = truncate_to_width(single_line_text, available_width);
        let padding = " ".repeat(self.padding_x);
        let line_with_padding = JsString::from(&padding) + display_text + &padding;
        let padding_needed = width.saturating_sub(visible_width(&line_with_padding));
        result.push(line_with_padding + " ".repeat(padding_needed));

        for _ in 0..self.padding_y {
            result.push(empty_line.clone());
        }
        result
    }

    fn wants_input(&self) -> bool {
        false
    }

    fn invalidate(&mut self) {}
}
