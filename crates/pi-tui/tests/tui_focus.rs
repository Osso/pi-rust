use crate::shared_components as components;
use crate::shared_virtual_terminal as virtual_terminal;

use pi_js_compat::js_json::JsString;
use std::cell::RefCell;
use std::rc::Rc;

use components::{focusable_component, inputs, render_count, set_lines, test_component};
use pi_tui::TUI;
use pi_tui::tui::{TuiExt, TuiInputListenerResult};
use virtual_terminal::{VirtualTerminal, concrete};

const FOCUS_IN: &str = "\x1b[I";
const FOCUS_OUT: &str = "\x1b[O";

fn wait<T: TuiExt>(tui: &Rc<T>, terminal: &Rc<RefCell<VirtualTerminal>>) {
    tui.flush_pending().unwrap();
    terminal.borrow_mut().flush();
}

fn send<T: TuiExt>(tui: &Rc<T>, terminal: &Rc<RefCell<VirtualTerminal>>, data: &str) {
    terminal.borrow_mut().send_input(data);
    tui.process_events();
}

#[test]
fn enables_focus_reporting_once_while_running_and_disables_it_once_on_stop() {
    let (terminal, handle) = concrete(80, 24);
    let tui = TUI::new(handle);

    tui.start().unwrap();
    tui.start().unwrap();
    wait(&tui, &terminal);
    tui.stop().unwrap();
    tui.stop().unwrap();

    let writes = terminal.borrow().writes();
    assert_eq!(
        writes
            .units()
            .windows("\x1b[?1004h".len())
            .filter(|window| window.iter().copied().eq("\x1b[?1004h".encode_utf16()))
            .count(),
        1
    );
    assert_eq!(
        writes
            .units()
            .windows("\x1b[?1004l".len())
            .filter(|window| window.iter().copied().eq("\x1b[?1004l".encode_utf16()))
            .count(),
        1
    );
}

#[test]
fn defers_rendering_while_unfocused_and_renders_latest_state_on_focus() {
    let (terminal, handle) = concrete(80, 24);
    let tui = TUI::new(handle);
    let (component, state) = test_component(&["initial"]);
    tui.add_child(component);
    tui.start().unwrap();
    wait(&tui, &terminal);
    let focused_render_count = render_count(&state);

    send(&tui, &terminal, FOCUS_OUT);
    set_lines(&state, &["stale", "state"]);
    tui.request_render(false);
    set_lines(&state, &["latest", "state"]);
    tui.request_render(false);
    wait(&tui, &terminal);

    assert_eq!(render_count(&state), focused_render_count);

    send(&tui, &terminal, FOCUS_IN);
    wait(&tui, &terminal);

    assert_eq!(render_count(&state), focused_render_count + 1);
    assert_eq!(
        &terminal.borrow_mut().flush_and_get_viewport()[..2],
        ["latest", "state"]
    );
    tui.stop().unwrap();
}

#[test]
fn cancels_a_queued_forced_render_when_focus_is_lost() {
    let (terminal, handle) = concrete(80, 24);
    let tui = TUI::new(handle);
    let (component, state) = test_component(&["initial"]);
    tui.add_child(component);
    tui.start().unwrap();
    wait(&tui, &terminal);
    let focused_render_count = render_count(&state);

    let redraws_before_force = tui.full_redraws();
    set_lines(&state, &["forced while hidden"]);
    tui.request_render(true);
    send(&tui, &terminal, FOCUS_OUT);
    wait(&tui, &terminal);

    assert_eq!(render_count(&state), focused_render_count);
    assert_eq!(tui.full_redraws(), redraws_before_force);

    send(&tui, &terminal, FOCUS_IN);
    wait(&tui, &terminal);
    assert_eq!(render_count(&state), focused_render_count + 1);
    assert_eq!(tui.full_redraws(), redraws_before_force + 1);
    tui.stop().unwrap();
}

#[test]
fn renders_pending_unfocused_state_after_stop_and_restart() {
    let (terminal, handle) = concrete(80, 24);
    let tui = TUI::new(handle);
    let (component, state) = test_component(&["initial"]);
    tui.add_child(component);
    tui.start().unwrap();
    wait(&tui, &terminal);
    let initial_render_count = render_count(&state);

    send(&tui, &terminal, FOCUS_OUT);
    set_lines(&state, &["updated while hidden"]);
    tui.request_render(false);
    tui.stop().unwrap();
    tui.start().unwrap();
    wait(&tui, &terminal);

    assert_eq!(render_count(&state), initial_render_count + 1);
    assert_eq!(
        &terminal.borrow_mut().flush_and_get_viewport()[..1],
        ["updated while hidden"]
    );
    tui.stop().unwrap();
}

#[test]
fn renders_state_requested_while_stopped_after_restart() {
    let (terminal, handle) = concrete(80, 24);
    let tui = TUI::new(handle);
    let (component, state) = test_component(&["initial"]);
    tui.add_child(component);
    tui.start().unwrap();
    wait(&tui, &terminal);
    let initial_render_count = render_count(&state);

    tui.stop().unwrap();
    set_lines(&state, &["updated while stopped"]);
    tui.request_render(false);
    tui.start().unwrap();
    wait(&tui, &terminal);

    assert_eq!(render_count(&state), initial_render_count + 1);
    assert_eq!(
        &terminal.borrow_mut().flush_and_get_viewport()[..1],
        ["updated while stopped"]
    );
    tui.stop().unwrap();
}

#[test]
fn consumes_focus_events_instead_of_forwarding_them_to_the_focused_component() {
    let (terminal, handle) = concrete(80, 24);
    let tui = TUI::new(handle);
    let (component, state, _focus) = focusable_component(&["initial"]);
    tui.add_child(component.clone());
    tui.set_focus(Some(component));
    tui.start().unwrap();
    wait(&tui, &terminal);

    let listener_inputs: Rc<RefCell<Vec<JsString>>> = Rc::new(RefCell::new(Vec::new()));
    let recorded = Rc::clone(&listener_inputs);
    tui.add_input_listener(Rc::new(move |data: &JsString| {
        recorded.borrow_mut().push(data.clone());
        TuiInputListenerResult::default()
    }));

    send(&tui, &terminal, FOCUS_OUT);
    send(&tui, &terminal, FOCUS_IN);
    send(&tui, &terminal, "x");

    assert_eq!(*listener_inputs.borrow(), vec!["x".to_string()]);
    assert_eq!(inputs(&state), vec!["x".to_string()]);
    tui.stop().unwrap();
}
