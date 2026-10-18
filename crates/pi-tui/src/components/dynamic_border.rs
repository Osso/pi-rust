//! Ported from `packages/tui/src/components/dynamic-border.ts`.

use std::rc::Rc;

use crate::tui::Component;

pub type BorderFormatter = Rc<dyn Fn(&str) -> String>;

pub struct DynamicBorder {
    format: BorderFormatter,
}

impl Default for DynamicBorder {
    fn default() -> Self {
        Self::new(Rc::new(|line: &str| line.to_string()))
    }
}

impl DynamicBorder {
    pub fn new(format: BorderFormatter) -> Self {
        Self { format }
    }
}

impl Component for DynamicBorder {
    fn render(&mut self, width: usize) -> Vec<String> {
        vec![(self.format)(&"─".repeat(width.max(1)))]
    }

    fn wants_input(&self) -> bool {
        false
    }

    fn invalidate(&mut self) {}
}
