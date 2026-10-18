//! Ported from `packages/coding-agent/src/modes/interactive/components/dynamic-border.ts`.

use std::rc::Rc;

use pi_tui::components::dynamic_border::{BorderFormatter, DynamicBorder};

use super::super::theme::{ThemeColor, with_theme};

/// Border colored with the theme's `border` token (the TypeScript subclass default).
pub fn dynamic_border() -> DynamicBorder {
    DynamicBorder::new(themed_border())
}

fn themed_border() -> BorderFormatter {
    Rc::new(|line: &str| with_theme(|theme| theme.fg(ThemeColor::Border, line)))
}
