use crate::shared_components as components;
use crate::shared_virtual_terminal as virtual_terminal;

use std::cell::RefCell;
use std::rc::Rc;

use components::{TestState, render_count, set_lines, test_component};
use pi_tui::tui::{OverlayOptions, SizeValue, TuiExt, TuiRenderer};
use virtual_terminal::{VirtualTerminal, concrete};

fn wait<T: TuiExt>(tui: &Rc<T>, terminal: &Rc<RefCell<VirtualTerminal>>) {
    tui.flush_pending().unwrap();
    terminal.borrow_mut().flush();
}

fn fixture(columns: usize, rows: usize) -> (Rc<RefCell<VirtualTerminal>>, Rc<TuiRenderer>, Rc<RefCell<TestState>>) {
    let (terminal, handle) = concrete(columns, rows);
    let tui = TuiRenderer::new(handle);
    let (component, state) = test_component(&["body", "› prompt", "footer"]);
    tui.add_child(component);
    tui.start().unwrap();
    wait(&tui, &terminal);
    (terminal, tui, state)
}

#[test]
fn updates_one_visible_cell_without_rendering_the_component_tree() {
    let (terminal, tui, state) = fixture(20, 5);
    let cell = tui.create_fixed_cell();
    terminal.borrow_mut().clear_writes();
    let renders = render_count(&state);

    cell.place(1, 0);
    let updated = cell.update("⠋");
    terminal.borrow_mut().flush();

    assert!(updated);
    assert_eq!(render_count(&state), renders);
    assert_eq!(terminal.borrow().viewport()[1], "⠋ prompt");
    assert!(!terminal.borrow().writes().is_empty());
    tui.stop().unwrap();
}

#[test]
fn synchronizes_the_previous_frame_so_the_next_normal_render_writes_nothing() {
    let (terminal, tui, state) = fixture(20, 5);
    let cell = tui.create_fixed_cell();

    set_lines(&state, &["body", "⠋ prompt", "footer"]);
    cell.place(1, 0);
    assert!(cell.update("⠋"));
    terminal.borrow_mut().flush();
    terminal.borrow_mut().clear_writes();

    tui.request_render(false);
    wait(&tui, &terminal);

    assert_eq!(terminal.borrow().writes(), "");
    assert_eq!(terminal.borrow().viewport()[1], "⠋ prompt");
    tui.stop().unwrap();
}

#[test]
fn rejects_fixed_cell_writes_while_a_normal_render_is_pending() {
    let (terminal, tui, state) = fixture(20, 5);
    let cell = tui.create_fixed_cell();
    terminal.borrow_mut().clear_writes();

    set_lines(&state, &["body", "queued prompt", "footer"]);
    let renders_before = render_count(&state);
    tui.request_render(false);
    cell.place(1, 0);

    assert!(!cell.update("⠋"));
    assert_eq!(terminal.borrow().writes(), "");
    assert_eq!(terminal.borrow().viewport()[1], "› prompt");

    wait(&tui, &terminal);

    assert_eq!(render_count(&state), renders_before + 1);
    assert_eq!(terminal.borrow().viewport()[1], "⠋ueued prompt");
    tui.stop().unwrap();
}

#[test]
fn rejects_writes_before_a_resize_render_and_accepts_after_repositioning() {
    let (terminal, tui, state) = fixture(20, 5);
    let cell = tui.create_fixed_cell();
    terminal.borrow_mut().clear_writes();

    cell.place(1, 0);
    terminal.borrow_mut().resize(30, 6);

    assert!(!cell.update("⠋"));
    assert_eq!(terminal.borrow().writes(), "");

    wait(&tui, &terminal);
    assert_eq!(terminal.borrow().viewport()[1], "⠋ prompt");
    let renders_after_resize = render_count(&state);
    terminal.borrow_mut().clear_writes();

    cell.place(2, 0);
    assert!(cell.update("X"));
    terminal.borrow_mut().flush();

    assert_eq!(render_count(&state), renders_after_resize);
    assert_eq!(terminal.borrow().viewport()[2], "Xooter");
    assert!(!terminal.borrow().writes().is_empty());
    tui.stop().unwrap();
}

#[test]
fn rejects_fixed_cell_writes_while_an_overlay_is_visible() {
    let (terminal, tui, _state) = fixture(20, 5);
    let (overlay, _overlay_state) = test_component(&["overlay"]);
    let cell = tui.create_fixed_cell();
    let overlay_handle = tui.show_overlay(
        overlay,
        Some(OverlayOptions {
            row: Some(SizeValue::Absolute(1.0)),
            col: Some(SizeValue::Absolute(0.0)),
            width: Some(SizeValue::Absolute(7.0)),
            ..Default::default()
        }),
    );
    wait(&tui, &terminal);
    terminal.borrow_mut().clear_writes();

    cell.place(1, 0);
    assert!(!cell.update("⠋"));
    terminal.borrow_mut().flush();
    assert_eq!(terminal.borrow().writes(), "");
    assert!(terminal.borrow().viewport()[1].starts_with("overlay"));
    assert!(!terminal.borrow().viewport()[1].contains("⠋"));

    overlay_handle.hide();
    wait(&tui, &terminal);
    tui.stop().unwrap();
}
