#[path = "support/components.rs"]
mod components;
#[path = "support/virtual_terminal.rs"]
mod virtual_terminal;

use std::cell::RefCell;
use std::rc::Rc;

use components::focusable_component;
use pi_tui::terminal_image::{
    CellDimensions, ImageProtocol, TerminalCapabilities, get_cell_dimensions, reset_capabilities_cache,
    set_capabilities, set_cell_dimensions,
};
use pi_tui::tui::TuiExt;
use pi_tui::tui_main_screen::TuiMainScreen;
use virtual_terminal::{VirtualTerminal, concrete};

static CAPS_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn with_image_terminal(test: impl FnOnce()) {
    let _guard = CAPS_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    set_capabilities(TerminalCapabilities {
        images: Some(ImageProtocol::Kitty),
        true_color: true,
        hyperlinks: true,
    });
    test();
    reset_capabilities_cache();
}

fn wait<T: TuiExt>(tui: &Rc<T>, terminal: &Rc<RefCell<VirtualTerminal>>) {
    tui.flush_pending();
    terminal.borrow_mut().flush();
}

fn send<T: TuiExt>(tui: &Rc<T>, terminal: &Rc<RefCell<VirtualTerminal>>, data: &str) {
    terminal.borrow_mut().send_input(data);
    tui.process_events();
}

#[test]
fn invokes_the_global_debug_handler_only_for_the_initial_key_press() {
    let (terminal, handle) = concrete(80, 24);
    let tui = TuiMainScreen::new(handle);
    let calls = Rc::new(std::cell::Cell::new(0));
    let recorded = Rc::clone(&calls);
    tui.set_on_debug(Rc::new(move || recorded.set(recorded.get() + 1)));
    tui.start().unwrap();

    send(&tui, &terminal, "\x1b[100;6:1u");
    send(&tui, &terminal, "\x1b[100;6:2u");
    send(&tui, &terminal, "\x1b[100;6:3u");

    assert_eq!(calls.get(), 1);
    tui.stop();
}

#[test]
fn forwards_bare_escape_even_when_a_cell_size_query_was_sent_at_startup() {
    with_image_terminal(|| {
        let (terminal, handle) = concrete(80, 24);
        let tui = TuiMainScreen::new(handle);
        let (recorder, state, _focus) = focusable_component(&[""]);
        tui.set_focus(Some(recorder));
        tui.start().unwrap();

        send(&tui, &terminal, "\x1b");

        assert_eq!(state.borrow().inputs, vec!["\x1b".to_string()]);
        tui.stop();
    });
}

#[test]
fn consumes_cell_size_responses_and_still_forwards_later_user_input() {
    with_image_terminal(|| {
        set_cell_dimensions(CellDimensions {
            width_px: 9,
            height_px: 18,
        });

        let (terminal, handle) = concrete(80, 24);
        let tui = TuiMainScreen::new(handle);
        let (recorder, state, _focus) = focusable_component(&[""]);
        tui.set_focus(Some(recorder));
        tui.start().unwrap();

        send(&tui, &terminal, "\x1b[6;20;10t");
        assert!(state.borrow().inputs.is_empty());
        assert_eq!(
            get_cell_dimensions(),
            CellDimensions {
                width_px: 10,
                height_px: 20,
            }
        );

        send(&tui, &terminal, "q");
        assert_eq!(state.borrow().inputs, vec!["q".to_string()]);
        tui.stop();
        wait(&tui, &terminal);
    });
}
