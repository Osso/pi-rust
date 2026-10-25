#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/virtual_terminal.rs"]
mod virtual_terminal;

use pi_js_compat::js_json::JsString;
use serde_json::{Value, json};

fn frozen(scenario: &str) -> Value {
    oracle::TsOracle::spawn("tui-verifier-oracle.mjs", "packages/tui/src/tui.ts")
        .request(&json!({"scenario": scenario}))
}

use pi_tui::tui::{Component, Container, TuiExt, TuiInputListener, TuiInputListenerResult, component};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use virtual_terminal::concrete;

struct Text(Rc<RefCell<JsString>>);
impl Component for Text {
    fn render(&mut self, _: usize) -> Vec<JsString> {
        vec![self.0.borrow().clone()]
    }
}
fn text(value: &str) -> pi_tui::tui::ComponentRef {
    component(Text(Rc::new(RefCell::new(value.into()))))
}

#[test]
fn overwidth_stops_terminal_before_descriptive_error() {
    let (terminal, handle) = concrete(4, 3);
    terminal.borrow_mut().set_capture_cursor_writes(true);
    let tui = pi_tui::TUI::new(handle);
    let value = Rc::new(RefCell::new("ok".into()));
    tui.add_child(component(Text(value.clone())));
    tui.start().unwrap();
    tui.render_now(false).unwrap();
    terminal.borrow_mut().clear_writes();
    *value.borrow_mut() = "long text".into();
    let error = tui.render_now(false).expect_err("overwidth render fails");
    let message = error.message.to_utf8().expect("scalar width error");
    assert!(
        message.starts_with("Rendered line 0 exceeds terminal width (9 > 4)."),
        "{message}"
    );
    assert_eq!(
        json!([message.lines().next().unwrap(), terminal.borrow().writes()]),
        frozen("crash")
    );
    terminal.borrow_mut().clear_writes();
    tui.render_now(false).unwrap();
    assert_eq!(terminal.borrow().writes(), "");
}

#[test]
fn removing_duplicate_child_keeps_second_reference() {
    let child = text("x");
    let mut container = Container::new();
    container.add_child(child.clone());
    container.add_child(text("middle"));
    container.add_child(child.clone());
    container.remove_child(&child);
    assert_eq!(container.render_children(80), ["middle", "x"]);
    let (_, handle) = concrete(80, 24);
    let tui = pi_tui::TUI::new(handle);
    tui.add_child(child.clone());
    tui.add_child(text("middle"));
    tui.add_child(child.clone());
    tui.remove_child(&child);
    assert_eq!(tui.base().children().len(), 2);
    assert!(Rc::ptr_eq(&tui.base().children()[1], &child));
    let root_lines: Vec<_> = tui
        .base()
        .children()
        .iter()
        .flat_map(|child| child.borrow_mut().render(80))
        .collect();
    assert_eq!(json!([container.render_children(80), root_lines]), frozen("children"));
}

#[test]
fn callback_identity_deduplicates_registration_and_unsubscribe() {
    let mut counts = Vec::new();
    let (terminal, handle) = concrete(80, 24);
    let tui = pi_tui::TUI::new(handle);
    let calls = Rc::new(Cell::new(0));
    let count = calls.clone();
    let listener: TuiInputListener = Rc::new(move |_| {
        count.set(count.get() + 1);
        TuiInputListenerResult::default()
    });
    let first = tui.add_input_listener(listener.clone());
    let second = tui.add_input_listener(listener.clone());
    terminal.borrow_mut().send_input("a");
    tui.process_events();
    counts.push(calls.get());
    assert_eq!(calls.get(), 1);
    tui.remove_input_listener(second);
    terminal.borrow_mut().send_input("b");
    tui.process_events();
    counts.push(calls.get());
    assert_eq!(calls.get(), 1);
    tui.remove_input_listener(first);
    tui.add_input_listener(listener);
    terminal.borrow_mut().send_input("c");
    tui.process_events();
    counts.push(calls.get());
    assert_eq!(calls.get(), 2);
    assert_eq!(json!(counts), frozen("input"));
}

#[test]
fn color_callback_identity_deduplicates_registration_and_unsubscribe() {
    let mut counts = Vec::new();
    let (terminal, handle) = concrete(80, 24);
    let tui = pi_tui::TUI::new(handle);
    let calls = Rc::new(Cell::new(0));
    let count = calls.clone();
    let listener: Rc<dyn Fn(pi_tui::TerminalColorScheme)> = Rc::new(move |_| count.set(count.get() + 1));
    let first = tui.on_terminal_color_scheme_change(listener.clone());
    let second = tui.on_terminal_color_scheme_change(listener.clone());
    terminal.borrow_mut().send_input("\x1b[?997;1n");
    tui.process_events();
    counts.push(calls.get());
    assert_eq!(calls.get(), 1);
    tui.base().remove_terminal_color_scheme_listener(second);
    terminal.borrow_mut().send_input("\x1b[?997;2n");
    tui.process_events();
    counts.push(calls.get());
    assert_eq!(calls.get(), 1);
    tui.base().remove_terminal_color_scheme_listener(first);
    tui.on_terminal_color_scheme_change(listener);
    terminal.borrow_mut().send_input("\x1b[?997;2n");
    tui.process_events();
    counts.push(calls.get());
    assert_eq!(calls.get(), 2);
    assert_eq!(json!(counts), frozen("color"));
}

#[test]
fn stale_input_unsubscribe_removes_reregistered_callback() {
    let (terminal, handle) = concrete(80, 24);
    let tui = pi_tui::TUI::new(handle);
    let calls = Rc::new(Cell::new(0));
    let count = calls.clone();
    let listener: TuiInputListener = Rc::new(move |_| {
        count.set(count.get() + 1);
        TuiInputListenerResult::default()
    });
    let first = tui.add_input_listener(listener.clone());
    let second = tui.add_input_listener(listener.clone());
    tui.remove_input_listener(second);
    tui.add_input_listener(listener.clone());
    tui.remove_input_listener(first);
    terminal.borrow_mut().send_input("a");
    tui.process_events();
    let expected = frozen("stale-input");
    assert_eq!(expected, json!(0));
    assert_eq!(json!(calls.get()), expected);

    // Unsubscribe handles remain callable after they have already removed L.
    tui.add_input_listener(listener);
    tui.remove_input_listener(first);
    terminal.borrow_mut().send_input("a");
    tui.process_events();
    assert_eq!(calls.get(), 0);
}

#[test]
fn stale_color_unsubscribe_removes_reregistered_callback() {
    let (terminal, handle) = concrete(80, 24);
    let tui = pi_tui::TUI::new(handle);
    let calls = Rc::new(Cell::new(0));
    let count = calls.clone();
    let listener: Rc<dyn Fn(pi_tui::TerminalColorScheme)> = Rc::new(move |_| count.set(count.get() + 1));
    let first = tui.on_terminal_color_scheme_change(listener.clone());
    let second = tui.on_terminal_color_scheme_change(listener.clone());
    tui.base().remove_terminal_color_scheme_listener(second);
    tui.on_terminal_color_scheme_change(listener.clone());
    tui.base().remove_terminal_color_scheme_listener(first);
    terminal.borrow_mut().send_input("\x1b[?997;1n");
    tui.process_events();
    let expected = frozen("stale-color");
    assert_eq!(expected, json!(0));
    assert_eq!(json!(calls.get()), expected);

    tui.on_terminal_color_scheme_change(listener);
    tui.base().remove_terminal_color_scheme_listener(first);
    terminal.borrow_mut().send_input("\x1b[?997;1n");
    tui.process_events();
    assert_eq!(calls.get(), 0);
}

#[test]
fn termux_environment_is_nonempty_and_read_at_render_time() {
    let mut redraws = Vec::new();
    for late in [false, true] {
        pi_js_compat::process_env::remove("TERMUX_VERSION");
        if !late {
            pi_js_compat::process_env::set("TERMUX_VERSION", "");
        }
        let (terminal, handle) = concrete(8, 10);
        let tui = pi_tui::TUI::new(handle);
        tui.add_child(text("a"));
        tui.start().unwrap();
        tui.render_now(false).unwrap();
        if late {
            pi_js_compat::process_env::set("TERMUX_VERSION", "1");
        }
        terminal.borrow_mut().resize(8, 9);
        tui.render_now(false).unwrap();
        redraws.push(tui.full_redraws());
        assert_eq!(tui.full_redraws(), if late { 1 } else { 2 }, "late={late}");
        tui.stop().unwrap();
    }
    pi_js_compat::process_env::remove("TERMUX_VERSION");
    assert_eq!(json!(redraws), frozen("termux"));
}

#[test]
fn nested_csi_keeps_sgr_on_wrapped_line() {
    assert_eq!(
        pi_tui::wrap_text_with_ansi("\x1b[x\x1b[31mabcdef", 3),
        serde_json::from_value::<Vec<String>>(frozen("sgr")).unwrap()
    );
}

#[test]
fn input_listener_changes_during_dispatch_follow_live_set_order() {
    let (terminal, handle) = concrete(80, 24);
    let tui = pi_tui::TUI::new(handle);
    let calls = Rc::new(RefCell::new(Vec::new()));
    let removed_id = Rc::new(Cell::new(0));
    let added_calls = calls.clone();
    let added: TuiInputListener = Rc::new(move |_| {
        added_calls.borrow_mut().push("added");
        TuiInputListenerResult::default()
    });
    let weak = Rc::downgrade(&tui);
    let first_calls = calls.clone();
    let remove = removed_id.clone();
    tui.add_input_listener(Rc::new(move |_| {
        first_calls.borrow_mut().push("first");
        let tui = weak.upgrade().unwrap();
        tui.remove_input_listener(remove.get());
        tui.add_input_listener(added.clone());
        TuiInputListenerResult::default()
    }));
    let removed_calls = calls.clone();
    removed_id.set(tui.add_input_listener(Rc::new(move |_| {
        removed_calls.borrow_mut().push("removed");
        TuiInputListenerResult::default()
    })));
    terminal.borrow_mut().send_input("a");
    tui.process_events();
    assert_eq!(json!(*calls.borrow()), frozen("live-input"));
}

#[test]
fn color_listener_changes_during_dispatch_follow_live_set_order() {
    let (terminal, handle) = concrete(80, 24);
    let tui = pi_tui::TUI::new(handle);
    let calls = Rc::new(RefCell::new(Vec::new()));
    let removed_id = Rc::new(Cell::new(0));
    let added_calls = calls.clone();
    let added: Rc<dyn Fn(pi_tui::TerminalColorScheme)> = Rc::new(move |_| added_calls.borrow_mut().push("added"));
    let weak = Rc::downgrade(&tui);
    let first_calls = calls.clone();
    let remove = removed_id.clone();
    tui.on_terminal_color_scheme_change(Rc::new(move |_| {
        first_calls.borrow_mut().push("first");
        let tui = weak.upgrade().unwrap();
        tui.base().remove_terminal_color_scheme_listener(remove.get());
        tui.on_terminal_color_scheme_change(added.clone());
    }));
    let removed_calls = calls.clone();
    removed_id.set(tui.on_terminal_color_scheme_change(Rc::new(move |_| removed_calls.borrow_mut().push("removed"))));
    terminal.borrow_mut().send_input("\x1b[?997;1n");
    tui.process_events();
    assert_eq!(json!(*calls.borrow()), frozen("live-color"));
}
