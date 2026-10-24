#[path = "support/components.rs"]
mod components;
#[path = "support/virtual_terminal.rs"]
mod virtual_terminal;

use pi_js_compat::js_json::JsString;
use std::cell::RefCell;
use std::rc::Rc;

use components::{render_count, set_lines, set_lines_owned, test_component};
use pi_tui::TUI;
use pi_tui::terminal_image::{
    CellDimensions, ImageProtocol, KittyEncodeOptions, TerminalCapabilities, delete_kitty_image, encode_kitty,
    reset_capabilities_cache, set_capabilities, set_cell_dimensions,
};
use pi_tui::tui::{TuiExt, TuiRenderer};
use virtual_terminal::{VirtualTerminal, concrete};

static CAPS_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn kitty_caps() -> TerminalCapabilities {
    TerminalCapabilities {
        images: Some(ImageProtocol::Kitty),
        true_color: true,
        hyperlinks: true,
    }
}

fn kitty(columns: u32, rows: u32, image_id: Option<u32>) -> JsString {
    encode_kitty(
        "AAAA",
        &KittyEncodeOptions {
            columns: Some(f64::from(columns)),
            rows: Some(f64::from(rows)),
            image_id,
            move_cursor: Some(false),
        },
    )
}

fn with_kitty_caps(test: impl FnOnce()) {
    let _guard = CAPS_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    set_capabilities(kitty_caps());
    set_cell_dimensions(CellDimensions {
        width_px: 10,
        height_px: 10,
    });
    test();
    reset_capabilities_cache();
    set_cell_dimensions(CellDimensions {
        width_px: 9,
        height_px: 18,
    });
}

fn wait<T: TuiExt>(tui: &Rc<T>, terminal: &Rc<RefCell<VirtualTerminal>>) {
    tui.flush_pending().unwrap();
    terminal.borrow_mut().flush();
}

#[test]
fn first_render_writes_all_lines() {
    let (terminal, handle) = concrete(20, 5);
    let tui = TUI::new(handle);
    let (child, _state) = test_component(&["one", "two"]);
    tui.add_child(child);
    tui.start().unwrap();
    wait(&tui, &terminal);

    assert_eq!(terminal.borrow().viewport(), vec!["one", "two", "", "", ""]);
    tui.stop().unwrap();
}

#[test]
fn changing_middle_line_redraws_only_that_line() {
    let (terminal, handle) = concrete(20, 5);
    let tui = TuiRenderer::new(handle);
    let (child, state) = test_component(&["Header", "Working...", "Footer"]);
    tui.add_child(child.clone());
    tui.start().unwrap();
    wait(&tui, &terminal);
    terminal.borrow_mut().clear_writes();

    set_lines(&state, &["Header", "Working |", "Footer"]);
    tui.request_render(false);
    wait(&tui, &terminal);

    assert_eq!(
        terminal.borrow().viewport(),
        vec!["Header", "Working |", "Footer", "", ""]
    );
    let writes = terminal.borrow().writes();
    assert!(writes.starts_with("\x1b[?2026h"));
    assert!(writes.contains("Working |"));
    assert!(!writes.contains("Header"));
    tui.stop().unwrap();
}

#[test]
fn spinner_frames_update_only_the_middle_line() {
    let (terminal, handle) = concrete(40, 10);
    let tui = TUI::new(handle);
    let (child, state) = test_component(&["Header", "Working...", "Footer"]);
    tui.add_child(child);
    tui.start().unwrap();
    wait(&tui, &terminal);

    for frame in ["|", "/", "-", "\\"] {
        set_lines(&state, &["Header", &format!("Working {frame}"), "Footer"]);
        tui.request_render(false);
        wait(&tui, &terminal);
        let viewport = terminal.borrow().viewport();
        assert_eq!(viewport[0], "Header");
        assert_eq!(viewport[1], format!("Working {frame}"));
        assert_eq!(viewport[2], "Footer");
    }
    tui.stop().unwrap();
}

#[test]
fn resets_styles_after_each_rendered_line() {
    let (terminal, handle) = concrete(20, 6);
    let tui = TUI::new(handle);
    let (child, _state) = test_component(&["\x1b[3mItalic", "Plain"]);
    tui.add_child(child);
    tui.start().unwrap();
    wait(&tui, &terminal);

    assert!(!terminal.borrow().cell_italic(1, 0));
    tui.stop().unwrap();
}

#[test]
fn renders_first_line_change_and_keeps_rest() {
    let (terminal, handle) = concrete(40, 10);
    let tui = TUI::new(handle);
    let (child, state) = test_component(&["Line 0", "Line 1", "Line 2", "Line 3"]);
    tui.add_child(child);
    tui.start().unwrap();
    wait(&tui, &terminal);

    set_lines(&state, &["CHANGED", "Line 1", "Line 2", "Line 3"]);
    tui.request_render(false);
    wait(&tui, &terminal);

    assert_eq!(
        &terminal.borrow().viewport()[..4],
        ["CHANGED", "Line 1", "Line 2", "Line 3"]
    );
    tui.stop().unwrap();
}

#[test]
fn renders_last_line_change_and_keeps_rest() {
    let (terminal, handle) = concrete(40, 10);
    let tui = TUI::new(handle);
    let (child, state) = test_component(&["Line 0", "Line 1", "Line 2", "Line 3"]);
    tui.add_child(child);
    tui.start().unwrap();
    wait(&tui, &terminal);

    set_lines(&state, &["Line 0", "Line 1", "Line 2", "CHANGED"]);
    tui.request_render(false);
    wait(&tui, &terminal);

    assert_eq!(
        &terminal.borrow().viewport()[..4],
        ["Line 0", "Line 1", "Line 2", "CHANGED"]
    );
    tui.stop().unwrap();
}

#[test]
fn renders_non_adjacent_line_changes() {
    let (terminal, handle) = concrete(40, 10);
    let tui = TUI::new(handle);
    let (child, state) = test_component(&["Line 0", "Line 1", "Line 2", "Line 3", "Line 4"]);
    tui.add_child(child);
    tui.start().unwrap();
    wait(&tui, &terminal);

    set_lines(&state, &["Line 0", "CHANGED 1", "Line 2", "CHANGED 3", "Line 4"]);
    tui.request_render(false);
    wait(&tui, &terminal);

    assert_eq!(
        &terminal.borrow().viewport()[..5],
        ["Line 0", "CHANGED 1", "Line 2", "CHANGED 3", "Line 4"]
    );
    tui.stop().unwrap();
}

#[test]
fn handles_transition_to_empty_and_back() {
    let (terminal, handle) = concrete(40, 10);
    let tui = TUI::new(handle);
    let (child, state) = test_component(&["Line 0", "Line 1", "Line 2"]);
    tui.add_child(child);
    tui.start().unwrap();
    wait(&tui, &terminal);
    assert_eq!(terminal.borrow().viewport()[0], "Line 0");

    set_lines(&state, &[]);
    tui.request_render(false);
    wait(&tui, &terminal);

    set_lines(&state, &["New Line 0", "New Line 1"]);
    tui.request_render(false);
    wait(&tui, &terminal);

    assert_eq!(&terminal.borrow().viewport()[..2], ["New Line 0", "New Line 1"]);
    tui.stop().unwrap();
}

#[test]
fn repaints_only_the_viewport_when_deleted_lines_move_it_upward() {
    let (terminal, handle) = concrete(20, 5);
    let tui = TUI::new(handle);
    let initial: Vec<String> = (0..12).map(|index| format!("Line {index}")).collect();
    let (child, state) = test_component(&[]);
    set_lines_owned(&state, initial);
    tui.add_child(child);
    tui.start().unwrap();
    wait(&tui, &terminal);
    let initial_redraws = tui.full_redraws();

    set_lines_owned(&state, (0..7).map(|index| format!("Line {index}")).collect());
    tui.request_render(false);
    wait(&tui, &terminal);

    assert_eq!(tui.full_redraws(), initial_redraws);
    assert_eq!(
        terminal.borrow().viewport(),
        vec!["Line 2", "Line 3", "Line 4", "Line 5", "Line 6"]
    );
    tui.stop().unwrap();
}

#[test]
fn renders_keyboard_input_without_waiting_for_a_throttled_frame() {
    let (terminal, handle) = concrete(40, 10);
    let tui = TUI::new(handle);
    let (child, state) = test_component(&["initial"]);
    tui.add_child(child.clone());
    tui.set_focus(Some(child));
    tui.start().unwrap();
    tui.render_now(false).unwrap();
    let renders_before = render_count(&state);

    state.borrow_mut().on_input = Some(Rc::new({
        let state = Rc::clone(&state);
        move |data: &JsString| {
            state.borrow_mut().lines = vec![data.clone()];
        }
    }));
    tui.request_render(false);
    terminal.borrow_mut().send_input("first");
    terminal.borrow_mut().send_input("second");
    terminal.borrow_mut().send_input("typed");
    tui.flush_pending().unwrap();

    assert_eq!(render_count(&state), renders_before + 1);
    assert_eq!(state.borrow().lines, vec!["typed".to_string()]);
    tui.stop().unwrap();
}

#[test]
fn renders_chat_growth_before_the_editor_without_clearing_scrollback() {
    let (terminal, handle) = concrete(20, 5);
    let tui = TUI::new(handle);
    let (chat, chat_state) = test_component(&["history 0", "history 1", "history 2", "history 3"]);
    let (editor, _editor_state) = test_component(&["editor", "footer"]);
    tui.add_child(chat);
    tui.add_child(editor);
    tui.start().unwrap();
    wait(&tui, &terminal);
    let initial_redraws = tui.full_redraws();
    terminal.borrow_mut().clear_writes();

    let mut lines = chat_state.borrow().lines.clone();
    lines.push("incoming update".into());
    chat_state.borrow_mut().lines = lines;
    tui.request_render(false);
    wait(&tui, &terminal);

    assert_eq!(tui.full_redraws(), initial_redraws);
    let writes = terminal.borrow().writes();
    assert!(!writes.contains("\x1b[2J"));
    assert!(!writes.contains("\x1b[3J"));
    assert_eq!(
        terminal.borrow().viewport(),
        vec!["history 2", "history 3", "incoming update", "editor", "footer"]
    );
    assert_eq!(&terminal.borrow().scroll_buffer()[..2], ["history 0", "history 1"]);
    tui.stop().unwrap();
}

#[test]
fn clears_reserved_kitty_image_rows_before_drawing_appended_placements() {
    with_kitty_caps(|| {
        let (terminal, handle) = concrete(40, 10);
        let tui = TUI::new(handle);
        let (child, state) = test_component(&["before"]);
        tui.add_child(child);
        tui.start().unwrap();
        wait(&tui, &terminal);
        terminal.borrow_mut().clear_writes();

        let image = kitty(2, 2, None);
        set_lines_owned(
            &state,
            vec![JsString::from("before"), image.clone(), JsString::default()],
        );
        tui.request_render(false);
        wait(&tui, &terminal);

        let writes = terminal.borrow().writes();
        assert!(
            writes.contains(&(JsString::from("\x1b[2K\r\n\x1b[2K\x1b[1A") + &image + "\x1b[1B")),
            "reserved rows should be cleared before the image placement is drawn: {writes:?}"
        );
        assert!(
            !writes.contains(&(image.clone() + "\r\n\x1b[2K")),
            "reserved row clears must not run after the image placement is drawn"
        );
        tui.stop().unwrap();
    });
}

#[test]
fn falls_back_to_full_redraw_when_kitty_image_pre_clear_would_scroll() {
    with_kitty_caps(|| {
        let (terminal, handle) = concrete(40, 2);
        let tui = TUI::new(handle);
        let (child, state) = test_component(&["before"]);
        tui.add_child(child);
        tui.start().unwrap();
        wait(&tui, &terminal);
        let redraws_before = tui.full_redraws();
        terminal.borrow_mut().clear_writes();

        let image = kitty(3, 3, None);
        set_lines_owned(
            &state,
            vec![
                JsString::from("before"),
                image,
                JsString::default(),
                JsString::default(),
                JsString::from("after"),
            ],
        );
        tui.request_render(false);
        wait(&tui, &terminal);

        assert!(tui.full_redraws() > redraws_before);
        assert!(terminal.borrow().writes().contains("\x1b[2J"));
        tui.stop().unwrap();
    });
}

#[test]
fn reserves_kitty_image_rows_before_drawing_during_full_redraw_fallbacks() {
    with_kitty_caps(|| {
        let (terminal, handle) = concrete(40, 5);
        let tui = TUI::new(handle);
        let (child, state) = test_component(&["l0", "l1", "l2", "l3", "l4"]);
        tui.add_child(child);
        tui.start().unwrap();
        wait(&tui, &terminal);
        let redraws_before = tui.full_redraws();
        terminal.borrow_mut().clear_writes();

        let image = kitty(3, 3, None);
        set_lines_owned(
            &state,
            vec![
                JsString::from("l0"),
                JsString::from("l1"),
                JsString::from("l2"),
                JsString::from("l3"),
                JsString::from("l4"),
                image.clone(),
                JsString::default(),
                JsString::default(),
                JsString::from("after"),
            ],
        );
        tui.request_render(false);
        wait(&tui, &terminal);

        let writes = terminal.borrow().writes();
        assert!(tui.full_redraws() > redraws_before);
        assert!(
            writes.contains(&(JsString::from("\r\n\r\n\x1b[2A") + &image + "\x1b[2B")),
            "full redraw should reserve visible image rows: {writes:?}"
        );
        assert!(!writes.contains(&(image.clone() + "\r\n\x1b[0m")));
        tui.stop().unwrap();
    });
}

#[test]
fn does_not_use_cursor_up_placement_for_kitty_images_taller_than_the_viewport() {
    with_kitty_caps(|| {
        let (terminal, handle) = concrete(40, 5);
        let tui = TUI::new(handle);
        let (child, state) = test_component(&["before"]);
        tui.add_child(child);
        tui.start().unwrap();
        wait(&tui, &terminal);
        terminal.borrow_mut().clear_writes();

        let image = kitty(6, 6, None);
        let mut lines = vec![JsString::from("before"), image.clone()];
        lines.extend(std::iter::repeat_n(JsString::default(), 5));
        lines.push(JsString::from("after"));
        set_lines_owned(&state, lines);
        tui.request_render(true);
        wait(&tui, &terminal);

        let writes = terminal.borrow().writes();
        assert!(writes.contains(&image));
        assert!(!writes.contains(&(JsString::from("\x1b[5A") + &image)));
        tui.stop().unwrap();
    });
}

#[test]
fn deletes_changed_image_ids_before_drawing_moved_placements() {
    let (terminal, handle) = concrete(40, 10);
    let tui = TUI::new(handle);
    let (child, state) = test_component(&["top"]);
    tui.add_child(child);
    let old_image = kitty(2, 2, Some(42));
    set_lines_owned(&state, vec![JsString::from("top"), old_image]);
    tui.start().unwrap();
    wait(&tui, &terminal);
    terminal.borrow_mut().clear_writes();

    let new_image = kitty(2, 1, Some(42));
    set_lines_owned(&state, vec![new_image.clone(), JsString::default()]);
    tui.request_render(false);
    wait(&tui, &terminal);

    let writes = terminal.borrow().writes();
    let delete = delete_kitty_image(42);
    let delete_index = writes.find(&delete).expect("old image should be deleted");
    let draw_index = writes.find(&new_image).expect("new image should be drawn");
    assert!(delete_index < draw_index);
    tui.stop().unwrap();
}

#[test]
fn redraws_image_lines_when_an_earlier_reserved_image_row_changes() {
    let (terminal, handle) = concrete(40, 10);
    let tui = TUI::new(handle);
    let (child, state) = test_component(&[""]);
    tui.add_child(child);
    let image = kitty(2, 2, Some(88));
    set_lines_owned(&state, vec![JsString::default(), image.clone()]);
    tui.start().unwrap();
    wait(&tui, &terminal);
    terminal.borrow_mut().clear_writes();

    set_lines_owned(&state, vec![JsString::from("covered"), image.clone()]);
    tui.request_render(false);
    wait(&tui, &terminal);

    let writes = terminal.borrow().writes();
    let delete = delete_kitty_image(88);
    let delete_index = writes.find(&delete).expect("image should be deleted");
    let draw_index = writes.find(&image).expect("image should be redrawn");
    assert!(delete_index < draw_index);
    assert!(!writes.contains("\x1b[2J"));
    tui.stop().unwrap();
}

#[test]
fn deletes_previously_rendered_image_ids_during_full_redraws() {
    let (terminal, handle) = concrete(40, 10);
    let tui = TUI::new(handle);
    let (child, state) = test_component(&[]);
    tui.add_child(child);
    set_lines_owned(&state, vec![kitty(2, 2, Some(77))]);
    tui.start().unwrap();
    wait(&tui, &terminal);
    terminal.borrow_mut().clear_writes();

    set_lines(&state, &["plain text"]);
    tui.request_render(true);
    wait(&tui, &terminal);

    let writes = terminal.borrow().writes();
    let delete = delete_kitty_image(77);
    let delete_index = writes.find(&delete).expect("previous image should be deleted");
    let clear_index = writes.find("\x1b[2J").expect("screen should be cleared");
    assert!(delete_index < clear_index);
    tui.stop().unwrap();
}

#[test]
fn triggers_full_render_when_terminal_height_changes() {
    let (terminal, handle) = concrete(40, 10);
    let tui = TUI::new(handle);
    tui.set_termux_session(false);
    let (child, _state) = test_component(&["Line 0", "Line 1", "Line 2"]);
    tui.add_child(child);
    tui.start().unwrap();
    wait(&tui, &terminal);
    let initial_redraws = tui.full_redraws();

    terminal.borrow_mut().resize(40, 15);
    wait(&tui, &terminal);

    assert!(tui.full_redraws() > initial_redraws);
    assert!(terminal.borrow().viewport()[0].contains("Line 0"));
    tui.stop().unwrap();
}

#[test]
fn skips_full_rerender_on_height_changes_in_termux() {
    let (terminal, handle) = concrete(40, 10);
    let tui = TUI::new(handle);
    tui.set_termux_session(true);
    let lines: Vec<String> = (0..20).map(|index| format!("Line {index}")).collect();
    let (child, state) = test_component(&[]);
    set_lines_owned(&state, lines);
    tui.add_child(child);
    tui.start().unwrap();
    wait(&tui, &terminal);
    terminal.borrow_mut().clear_writes();
    let initial_redraws = tui.full_redraws();

    for height in [15, 8, 14, 11] {
        terminal.borrow_mut().resize(40, height);
        wait(&tui, &terminal);
    }
    assert_eq!(tui.full_redraws(), initial_redraws);
    let writes = terminal.borrow().writes();
    assert!(!writes.contains("\x1b[2J"));
    assert!(!writes.contains("\x1b[3J"));
    assert!(
        terminal.borrow().viewport().join("\n").contains("Line 19"),
        "latest content remains visible: {:?}",
        terminal.borrow().viewport()
    );
    tui.stop().unwrap();
}

#[test]
fn triggers_full_render_when_terminal_width_changes() {
    let (terminal, handle) = concrete(40, 10);
    let tui = TUI::new(handle);
    let (child, _state) = test_component(&["Line 0", "Line 1", "Line 2"]);
    tui.add_child(child);
    tui.start().unwrap();
    wait(&tui, &terminal);
    let initial_redraws = tui.full_redraws();

    terminal.borrow_mut().resize(60, 10);
    wait(&tui, &terminal);

    assert!(tui.full_redraws() > initial_redraws);
    tui.stop().unwrap();
}

#[test]
fn clears_empty_rows_when_content_shrinks_significantly() {
    let (terminal, handle) = concrete(40, 10);
    let tui = TUI::new(handle);
    tui.set_clear_on_shrink(true);
    let (child, state) = test_component(&["Line 0", "Line 1", "Line 2", "Line 3", "Line 4", "Line 5"]);
    tui.add_child(child);
    tui.start().unwrap();
    wait(&tui, &terminal);
    let initial_redraws = tui.full_redraws();

    set_lines(&state, &["Line 0", "Line 1"]);
    tui.request_render(false);
    wait(&tui, &terminal);

    assert!(tui.full_redraws() > initial_redraws);
    let viewport = terminal.borrow().viewport();
    assert_eq!(viewport[0], "Line 0");
    assert_eq!(viewport[1], "Line 1");
    assert_eq!(viewport[2].trim(), "");
    assert_eq!(viewport[3].trim(), "");
    tui.stop().unwrap();
}

#[test]
fn handles_shrink_to_single_line_and_empty() {
    let (terminal, handle) = concrete(40, 10);
    let tui = TUI::new(handle);
    tui.set_clear_on_shrink(true);
    let (child, state) = test_component(&["Line 0", "Line 1", "Line 2", "Line 3"]);
    tui.add_child(child);
    tui.start().unwrap();
    wait(&tui, &terminal);

    set_lines(&state, &["Only line"]);
    tui.request_render(false);
    wait(&tui, &terminal);
    let viewport = terminal.borrow().viewport();
    assert_eq!(viewport[0], "Only line");
    assert_eq!(viewport[1].trim(), "");

    set_lines(&state, &[]);
    tui.request_render(false);
    wait(&tui, &terminal);
    let viewport = terminal.borrow().viewport();
    assert_eq!(viewport[0].trim(), "");
    assert_eq!(viewport[1].trim(), "");
    tui.stop().unwrap();
}

#[test]
fn tracks_cursor_when_content_shrinks_with_unchanged_remaining_lines() {
    let (terminal, handle) = concrete(40, 10);
    let tui = TUI::new(handle);
    let (child, state) = test_component(&["Line 0", "Line 1", "Line 2", "Line 3", "Line 4"]);
    tui.add_child(child);
    tui.start().unwrap();
    wait(&tui, &terminal);

    set_lines(&state, &["Line 0", "Line 1", "Line 2"]);
    tui.request_render(false);
    wait(&tui, &terminal);

    set_lines(&state, &["Line 0", "CHANGED", "Line 2"]);
    tui.request_render(false);
    wait(&tui, &terminal);

    assert_eq!(terminal.borrow().viewport()[1], "CHANGED");
    tui.stop().unwrap();
}

#[test]
fn appends_after_a_shrink_without_another_full_redraw() {
    let (terminal, handle) = concrete(20, 5);
    let tui = TUI::new(handle);
    let (child, state) = test_component(&[]);
    set_lines_owned(&state, (0..8).map(|index| format!("Line {index}")).collect());
    tui.add_child(child);
    tui.start().unwrap();
    wait(&tui, &terminal);
    let initial_redraws = tui.full_redraws();

    set_lines(&state, &["Line 0", "Line 1"]);
    tui.request_render(false);
    wait(&tui, &terminal);
    assert_eq!(tui.full_redraws(), initial_redraws);
    let redraws_after_shrink = tui.full_redraws();

    set_lines(&state, &["Line 0", "Line 1", "Line 2"]);
    tui.request_render(false);
    wait(&tui, &terminal);

    assert_eq!(tui.full_redraws(), redraws_after_shrink);
    assert_eq!(terminal.borrow().viewport(), vec!["Line 0", "Line 1", "Line 2", "", ""]);
    tui.stop().unwrap();
}

#[test]
fn clears_stale_content_when_max_lines_rendered_was_inflated() {
    let (terminal, handle) = concrete(40, 10);
    let tui = TUI::new(handle);
    let (chat, chat_state) = test_component(&[]);
    let (editor, editor_state) = test_component(&[]);
    tui.add_child(chat);
    tui.add_child(editor);

    let long_chat: Vec<String> = (0..15).map(|index| format!("Chat {index}")).collect();
    let short_chat: Vec<String> = (0..12).map(|index| format!("Chat {index}")).collect();
    let editor_lines = ["Editor 0", "Editor 1", "Editor 2"];
    let selector_lines: Vec<String> = (0..8).map(|index| format!("Selector {index}")).collect();

    set_lines_owned(&chat_state, long_chat);
    set_lines(&editor_state, &editor_lines);
    tui.start().unwrap();
    wait(&tui, &terminal);

    set_lines_owned(&editor_state, selector_lines);
    tui.request_render(false);
    wait(&tui, &terminal);

    set_lines(&editor_state, &editor_lines);
    tui.request_render(false);
    wait(&tui, &terminal);

    let redraws_before_switch = tui.full_redraws();
    set_lines_owned(&chat_state, short_chat);
    tui.request_render(false);
    wait(&tui, &terminal);

    assert_eq!(tui.full_redraws(), redraws_before_switch);
    assert_eq!(
        terminal.borrow().viewport(),
        vec![
            "Chat 5", "Chat 6", "Chat 7", "Chat 8", "Chat 9", "Chat 10", "Chat 11", "Editor 0", "Editor 1", "Editor 2"
        ]
    );
    tui.stop().unwrap();
}

#[test]
fn writes_redraw_logs_to_the_provided_directory() {
    let log_dir = std::env::temp_dir().join(format!(
        "pi-tui-log-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&log_dir).unwrap();
    {
        let (terminal, handle) = concrete(40, 10);
        let tui = TUI::with_options(handle, None, Some(log_dir.clone()));
        tui.set_debug_redraw(true);
        let (child, _state) = test_component(&["test"]);
        tui.add_child(child);
        tui.start().unwrap();
        wait(&tui, &terminal);
        tui.stop().unwrap();
    }
    let contents = std::fs::read_to_string(log_dir.join("pi-debug.log")).unwrap();
    assert!(contents.contains("fullRender: first render"));
    let stamp = contents.trim_start_matches('[').split(']').next().expect("timestamp");
    assert!(
        stamp.len() == 24 && stamp.ends_with('Z') && stamp.as_bytes()[10] == b'T',
        "expected new Date().toISOString(): {contents}"
    );
    let _ = std::fs::remove_dir_all(log_dir);
}

#[test]
fn crash_logs_use_iso_timestamps() {
    let log_dir = std::env::temp_dir().join(format!(
        "pi-tui-crash-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&log_dir).unwrap();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let (terminal, handle) = concrete(10, 5);
        let tui = TUI::with_options(handle, None, Some(log_dir.clone()));
        let (child, state) = test_component(&["short"]);
        tui.add_child(child);
        tui.start().unwrap();
        wait(&tui, &terminal);
        state.borrow_mut().set_lines(&["this line is wider than ten cells"]);
        tui.request_render(false);
        wait(&tui, &terminal);
    }));
    assert!(outcome.is_err(), "an over-wide line must panic");
    let contents = std::fs::read_to_string(log_dir.join("pi-crash.log")).unwrap();
    let stamp = contents
        .strip_prefix("Crash at ")
        .and_then(|rest| rest.split('\n').next())
        .expect("crash timestamp");
    assert!(
        stamp.len() == 24 && stamp.ends_with('Z') && stamp.as_bytes()[10] == b'T',
        "expected new Date().toISOString(): {contents}"
    );
    let _ = std::fs::remove_dir_all(log_dir);
}
