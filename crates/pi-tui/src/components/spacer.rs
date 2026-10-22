//! Ported from `packages/tui/src/components/spacer.ts`.

use pi_js_compat::js_json::JsString;

use crate::tui::Component;

pub struct Spacer {
    lines: usize,
}

impl Default for Spacer {
    fn default() -> Self {
        Self::new(1)
    }
}

impl Spacer {
    pub fn new(lines: usize) -> Self {
        Self { lines }
    }

    pub fn set_lines(&mut self, lines: usize) {
        self.lines = lines;
    }

    pub fn lines(&self) -> usize {
        self.lines
    }
}

impl Component for Spacer {
    fn render(&mut self, _width: usize) -> Vec<JsString> {
        vec![JsString::default(); self.lines]
    }

    fn wants_input(&self) -> bool {
        false
    }

    fn invalidate(&mut self) {}
}
