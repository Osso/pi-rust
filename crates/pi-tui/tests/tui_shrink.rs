#[path = "support/components.rs"]
mod components;
#[path = "support/virtual_terminal.rs"]
mod virtual_terminal;

use std::cell::RefCell;
use std::rc::Rc;

use components::test_component;
use pi_tui::TUI;
use pi_tui::tui::TuiExt;
use virtual_terminal::{VirtualTerminal, concrete};

fn wait<T: TuiExt>(tui: &Rc<T>, terminal: &Rc<RefCell<VirtualTerminal>>) {
    tui.flush_pending();
    terminal.borrow_mut().flush();
}

#[test]
fn clears_all_rendered_lines_when_content_shrinks_to_zero() {
    let (terminal, handle) = concrete(40, 10);
    let tui = TUI::new(handle);
    let (content, _state) = test_component(&["first", "second", "third"]);
    tui.add_child(content);
    tui.start().unwrap();
    wait(&tui, &terminal);

    let viewport = terminal.borrow().viewport();
    assert!(viewport.iter().any(|line| line.contains("first")));
    assert!(viewport.iter().any(|line| line.contains("second")));
    assert!(viewport.iter().any(|line| line.contains("third")));

    tui.clear();
    tui.request_render(false);
    wait(&tui, &terminal);

    let viewport = terminal.borrow().viewport();
    assert!(!viewport.iter().any(|line| line.contains("first")));
    assert!(!viewport.iter().any(|line| line.contains("second")));
    assert!(!viewport.iter().any(|line| line.contains("third")));
    tui.stop();
}
