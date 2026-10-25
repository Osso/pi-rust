use crate::shared_components as components;
use crate::shared_virtual_terminal as virtual_terminal;

use std::cell::RefCell;
use std::rc::Rc;

use components::test_component;
use pi_tui::TUI;
use pi_tui::tui::TuiExt;
use virtual_terminal::{VirtualTerminal, concrete};

fn wait<T: TuiExt>(tui: &Rc<T>, terminal: &Rc<RefCell<VirtualTerminal>>) {
    tui.flush_pending().unwrap();
    terminal.borrow_mut().flush();
}

#[test]
fn does_not_leak_styles_when_a_trailing_reset_sits_beyond_the_last_visible_column() {
    let width = 20;
    let base_line = format!("\x1b[3m{}\x1b[23m", "X".repeat(width));
    let (terminal, handle) = concrete(width, 6);
    let tui = TUI::new(handle);
    let (child, _state) = test_component(&[&base_line, "INPUT"]);
    tui.add_child(child);
    tui.start().unwrap();
    tui.request_render(true);
    wait(&tui, &terminal);

    assert!(!terminal.borrow().cell_italic(1, 0));
    tui.stop().unwrap();
}

#[test]
fn does_not_leak_styles_when_overlay_slicing_drops_trailing_sgr_resets() {
    let width = 20;
    let base_line = format!("\x1b[3m{}\x1b[23m", "X".repeat(width));
    let (terminal, handle) = concrete(width, 6);
    let tui = TUI::new(handle);
    let (child, _state) = test_component(&[&base_line, "INPUT"]);
    tui.add_child(child);
    let (overlay, _overlay_state) = test_component(&["OVR"]);
    tui.show_overlay(
        overlay,
        Some(pi_tui::tui::OverlayOptions {
            row: Some(pi_tui::tui::SizeValue::Absolute(0.0)),
            col: Some(pi_tui::tui::SizeValue::Absolute(5.0)),
            width: Some(pi_tui::tui::SizeValue::Absolute(3.0)),
            ..Default::default()
        }),
    );
    tui.start().unwrap();
    tui.request_render(true);
    wait(&tui, &terminal);

    assert!(!terminal.borrow().cell_italic(1, 0));
    tui.stop().unwrap();
}
