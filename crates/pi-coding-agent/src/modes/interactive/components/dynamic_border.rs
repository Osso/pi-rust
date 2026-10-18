//! Ported from `coding-agent/src/modes/interactive/components/dynamic-border.ts`.

use std::rc::Rc;

use pi_tui::DynamicBorder;

use crate::modes::interactive::theme_api::{ThemeApi, ThemeColor};

/// `DynamicBorder` preconfigured with the theme's `border` color.
pub fn dynamic_border(theme: &Rc<dyn ThemeApi>) -> DynamicBorder {
    let theme = theme.clone();
    DynamicBorder::new(Rc::new(move |line: &str| theme.fg(ThemeColor::Border, line)))
}

/// `new DynamicBorder(color)` with a caller-supplied formatter.
pub fn dynamic_border_with(color: Rc<dyn Fn(&str) -> String>) -> DynamicBorder {
    DynamicBorder::new(color)
}
