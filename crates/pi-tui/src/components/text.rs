//! Ported from `packages/tui/src/components/text.ts`.
//!
//! Multi-line text with word wrapping, padding, and optional background styling.
//! The TypeScript cache is keyed by text and width; the Rust port keeps the same
//! three-field cache so repeated renders return the previously built lines.

use std::rc::Rc;

use crate::tui::Component;
use crate::utils::{apply_background_to_line, visible_width, wrap_text_with_ansi};

/// Terminal styling callback (`(text) => text`).
pub type StyleFn = Rc<dyn Fn(&str) -> String>;

#[derive(Default)]
pub struct Text {
    text: String,
    padding_x: usize,
    padding_y: usize,
    custom_bg_fn: Option<StyleFn>,
    cached_text: Option<String>,
    cached_width: Option<usize>,
    cached_lines: Option<Vec<String>>,
}

impl Text {
    pub fn new(text: impl Into<String>, padding_x: usize, padding_y: usize) -> Self {
        Self {
            text: text.into(),
            padding_x,
            padding_y,
            custom_bg_fn: None,
            cached_text: None,
            cached_width: None,
            cached_lines: None,
        }
    }

    pub fn with_background(text: impl Into<String>, padding_x: usize, padding_y: usize, custom_bg_fn: StyleFn) -> Self {
        Self {
            custom_bg_fn: Some(custom_bg_fn),
            ..Self::new(text, padding_x, padding_y)
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.invalidate();
    }

    pub fn set_custom_bg_fn(&mut self, custom_bg_fn: Option<StyleFn>) {
        self.custom_bg_fn = custom_bg_fn;
        self.invalidate();
    }

    fn pad_to_width(&self, line: &str, width: usize) -> String {
        match &self.custom_bg_fn {
            Some(bg_fn) => apply_background_to_line(line, width, |text| bg_fn(text)),
            None => {
                let padding_needed = width.saturating_sub(visible_width(line));
                format!("{line}{}", " ".repeat(padding_needed))
            }
        }
    }

    fn build_lines(&self, width: usize) -> Vec<String> {
        let normalized_text = self.text.replace('\t', "   ");
        let content_width = width.saturating_sub(self.padding_x * 2).max(1);
        let wrapped_lines = wrap_text_with_ansi(&normalized_text, content_width);

        let margin = " ".repeat(self.padding_x);
        let content_lines: Vec<String> = wrapped_lines
            .into_iter()
            .map(|line| self.pad_to_width(&format!("{margin}{line}{margin}"), width))
            .collect();

        let empty_line = " ".repeat(width);
        let empty_lines: Vec<String> = (0..self.padding_y)
            .map(|_| self.pad_to_width(&empty_line, width))
            .collect();

        let mut result = Vec::with_capacity(empty_lines.len() * 2 + content_lines.len());
        result.extend(empty_lines.iter().cloned());
        result.extend(content_lines);
        result.extend(empty_lines);
        result
    }
}

impl Component for Text {
    fn render(&mut self, width: usize) -> Vec<String> {
        if let (Some(lines), Some(cached_width), Some(cached_text)) =
            (&self.cached_lines, self.cached_width, &self.cached_text)
            && cached_width == width
            && *cached_text == self.text
        {
            return lines.clone();
        }

        if self.text.trim().is_empty() {
            self.cached_text = Some(self.text.clone());
            self.cached_width = Some(width);
            self.cached_lines = Some(Vec::new());
            return Vec::new();
        }

        let result = self.build_lines(width);
        self.cached_text = Some(self.text.clone());
        self.cached_width = Some(width);
        self.cached_lines = Some(result.clone());
        if result.is_empty() { vec![String::new()] } else { result }
    }

    fn invalidate(&mut self) {
        self.cached_text = None;
        self.cached_width = None;
        self.cached_lines = None;
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}
