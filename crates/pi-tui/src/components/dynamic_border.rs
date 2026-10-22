//! Ported from `packages/tui/src/components/dynamic-border.ts`.

use std::rc::Rc;

use pi_js_compat::js_json::JsString;

use crate::tui::Component;

pub type BorderFormatter = Rc<dyn Fn(&JsString) -> JsString>;

pub struct DynamicBorder {
    format: BorderFormatter,
}

impl Default for DynamicBorder {
    fn default() -> Self {
        Self::new(Rc::new(|line: &JsString| line.clone()))
    }
}

impl DynamicBorder {
    pub fn new(format: BorderFormatter) -> Self {
        Self { format }
    }
}

impl Component for DynamicBorder {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        vec![(self.format)(&JsString::from("─".repeat(width.max(1))))]
    }

    fn wants_input(&self) -> bool {
        false
    }

    fn invalidate(&mut self) {}
}
