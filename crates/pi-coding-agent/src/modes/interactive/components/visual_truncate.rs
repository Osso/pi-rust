//! Ported from `coding-agent/src/modes/interactive/components/visual-truncate.ts`.

use pi_js_compat::js_json::JsString;
use pi_tui::{Component, Text};

/// The visual lines to display plus how many were skipped (hidden).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisualTruncateResult {
    pub visual_lines: Vec<JsString>,
    pub skipped_count: usize,
}

/// Truncate text to the last `max_visual_lines` rendered lines at `width`.
pub fn truncate_to_visual_lines(
    text: &JsString,
    max_visual_lines: usize,
    width: usize,
    padding_x: usize,
) -> VisualTruncateResult {
    if text.is_empty() {
        return VisualTruncateResult {
            visual_lines: Vec::new(),
            skipped_count: 0,
        };
    }

    let mut temp_text = Text::new(text, padding_x, 0);
    let all_visual_lines = temp_text.render(width);
    if all_visual_lines.len() <= max_visual_lines {
        return VisualTruncateResult {
            visual_lines: all_visual_lines,
            skipped_count: 0,
        };
    }

    // JS `slice(-0)` returns the whole array, so a zero limit keeps every line.
    let visual_lines = if max_visual_lines == 0 {
        all_visual_lines.clone()
    } else {
        all_visual_lines[all_visual_lines.len() - max_visual_lines..].to_vec()
    };
    VisualTruncateResult {
        visual_lines,
        skipped_count: all_visual_lines.len() - max_visual_lines,
    }
}
