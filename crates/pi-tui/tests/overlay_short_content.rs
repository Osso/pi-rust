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
fn renders_overlay_when_content_is_shorter_than_terminal_height() {
    let (terminal, handle) = concrete(80, 24);
    let tui = TUI::new(handle);
    let (content, _state) = test_component(&["Line 1", "Line 2", "Line 3"]);
    tui.add_child(content);
    let (overlay, _overlay_state) = test_component(&["OVERLAY_TOP", "OVERLAY_MID", "OVERLAY_BOT"]);
    tui.show_overlay(overlay, None);
    tui.start().unwrap();
    wait(&tui, &terminal);

    assert!(
        terminal.borrow().viewport().iter().any(|line| line.contains("OVERLAY")),
        "overlay should be visible: {:?}",
        terminal.borrow().viewport()
    );
    tui.stop().unwrap();
}
