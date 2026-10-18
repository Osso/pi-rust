//! Ported from `packages/tui/src/components/spacer.ts`.

use crate::tui::Component;

#[derive(Default)]
pub struct Spacer {
    lines: usize,
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
    fn render(&mut self, _width: usize) -> Vec<String> {
        vec![String::new(); self.lines]
    }

    fn wants_input(&self) -> bool {
        false
    }

    fn invalidate(&mut self) {}
}
