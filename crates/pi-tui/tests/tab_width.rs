//! Ported from `packages/tui/test/tab-width.test.ts`.
//!
//! The first three cases (slice/segment widths and control-sequence tabs) are
//! already covered by `tests/utils.rs`; this file ports the overlay case that
//! keeps a tab-containing overlay on one physical terminal row.

#[path = "support/virtual_terminal.rs"]
mod virtual_terminal;

use pi_tui::tui::{Component, OverlayOptions, SizeValue, TuiExt};
use pi_tui::tui_main_screen::TuiMainScreen;
use virtual_terminal::concrete;

struct FullViewportContent;

impl Component for FullViewportContent {
    fn render(&mut self, width: usize) -> Vec<String> {
        ["base 0", "base 1", "base 2"]
            .iter()
            .map(|line| format!("{line:<width$}"))
            .collect()
    }
}

struct TabStatusOverlay;

impl Component for TabStatusOverlay {
    fn render(&mut self, _width: usize) -> Vec<String> {
        vec!["\tX".to_string()]
    }
}

#[test]
fn keeps_tab_containing_overlays_on_one_physical_terminal_row() {
    let (terminal, handle) = concrete(16, 3);
    let tui = TuiMainScreen::new(handle);
    tui.add_child(pi_tui::tui::component(FullViewportContent));
    tui.show_overlay(
        pi_tui::tui::component(TabStatusOverlay),
        Some(OverlayOptions {
            width: Some(SizeValue::Absolute(4.0)),
            row: Some(SizeValue::Absolute(1.0)),
            col: Some(SizeValue::Absolute(4.0)),
            ..OverlayOptions::default()
        }),
    );
    tui.start().unwrap();

    tui.flush_pending();
    terminal.borrow_mut().flush();
    let viewport = terminal.borrow().viewport();
    // VirtualTerminal rows drop trailing blanks; content and row placement match the
    // TypeScript capturing terminal after trimming.
    assert_eq!(viewport, vec!["base 0", "base   X", "base 2"]);
    assert!(!terminal.borrow().writes().contains('\t'));

    tui.stop();
}
