//! Ported from `packages/tui/test/tui-alt-screen.test.ts`.
//!
//! Timer-driven behavior uses the driver-owned [`TuiAltScreen::poll`] seam.

#[path = "support/components.rs"]
mod components;
#[path = "support/virtual_terminal.rs"]
mod virtual_terminal;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use components::{focusable_component, inputs};
use pi_tui::components::h_stack::HStack;
use pi_tui::components::image::{Image, ImageOptions, ImageTheme};
use pi_tui::components::scroll_view::{ScrollFollow, ScrollState, ScrollView, ScrollViewOptions, ScrollViewScrollbar};
use pi_tui::components::stack::StackOptions;
use pi_tui::components::text::Text;
use pi_tui::components::v_stack::VStack;
use pi_tui::keybindings::{KeybindingsConfig, KeybindingsManager, get_keybindings, set_keybindings, tui_keybindings};
use pi_tui::layout_node::{StackBasis, StackLayoutEntry};
use pi_tui::terminal_image::{
    CellDimensions, ImageDimensions, ImageProtocol, KittyEncodeOptions, KittyImageMetadata, TerminalCapabilities,
    encode_kitty, hyperlink, register_kitty_image_metadata, reset_capabilities_cache, set_capabilities,
    set_cell_dimensions,
};
use pi_tui::tui::{Component, ComponentRef, TuiExt, component};
use pi_tui::tui_alt_screen::{AltScreenEnvironment, TuiAltScreen, TuiAltScreenOptions};
use virtual_terminal::{VirtualTerminal, concrete};

static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|poison| poison.into_inner())
}

fn wait(tui: &Rc<TuiAltScreen>, terminal: &Rc<RefCell<VirtualTerminal>>) {
    tui.flush_pending();
    terminal.borrow_mut().flush();
}

fn send(tui: &Rc<TuiAltScreen>, terminal: &Rc<RefCell<VirtualTerminal>>, data: &str) {
    terminal.borrow_mut().send_input(data);
    tui.process_events();
}

fn viewport(terminal: &Rc<RefCell<VirtualTerminal>>) -> Vec<String> {
    terminal
        .borrow()
        .viewport()
        .iter()
        .map(|line| line.trim_end().to_string())
        .collect()
}

fn writes(terminal: &Rc<RefCell<VirtualTerminal>>) -> String {
    terminal.borrow().writes()
}

fn lines(count: usize) -> String {
    (1..=count)
        .map(|index| format!("line {index}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn text(lines: &str) -> ComponentRef {
    component(Text::new(lines, 0, 0))
}

fn set_text(component: &ComponentRef, value: &str) {
    let mut borrow = component.borrow_mut();
    let any = borrow.as_any_mut().expect("text component downcast");
    any.downcast_mut::<Text>().expect("Text").set_text(value);
}

fn scroll_view(
    child: ComponentRef,
    follow: ScrollFollow,
    primary: bool,
    scrollbar: ScrollViewScrollbar,
    scrollbar_hide_delay_ms: Option<u64>,
) -> (ComponentRef, Rc<RefCell<ScrollState>>) {
    let view = ScrollView::new(
        child,
        ScrollViewOptions {
            follow: Some(follow),
            primary,
            scrollbar: Some(scrollbar),
            scrollbar_hide_delay_ms,
            ..ScrollViewOptions::default()
        },
    );
    let state = Rc::clone(view.state());
    (component(view), state)
}

fn entry(component: ComponentRef) -> StackLayoutEntry {
    StackLayoutEntry::new(component)
}

fn sized(
    component: ComponentRef,
    basis: Option<StackBasis>,
    grow: Option<usize>,
    shrink: Option<usize>,
    min_size: Option<usize>,
) -> StackLayoutEntry {
    StackLayoutEntry {
        component,
        basis,
        grow,
        shrink,
        min_size,
        max_size: None,
        visible: None,
    }
}

fn vstack(entries: Vec<StackLayoutEntry>) -> ComponentRef {
    component(VStack::new(entries, StackOptions::default()))
}

fn hstack(entries: Vec<StackLayoutEntry>) -> ComponentRef {
    component(HStack::new(entries, StackOptions::default()))
}

fn kitty_caps() -> TerminalCapabilities {
    TerminalCapabilities {
        images: Some(ImageProtocol::Kitty),
        true_color: true,
        hyperlinks: true,
    }
}

fn kitty_line(image_id: u32, columns: u32, rows: u32) -> String {
    encode_kitty(
        "AAAA",
        &KittyEncodeOptions {
            columns: Some(columns),
            rows: Some(rows),
            image_id: Some(image_id),
            move_cursor: Some(false),
        },
    )
}

fn register_kitty(image_id: u32, columns: u32, rows: u32, width_px: u32, height_px: u32) {
    register_kitty_image_metadata(KittyImageMetadata {
        image_id,
        columns,
        rows,
        width_px,
        height_px,
    });
}

#[test]
fn renders_terminal_height_viewport_and_preserves_manual_scroll() {
    let _guard = serial();
    let (terminal, handle) = concrete(20, 4);
    let tui = TuiAltScreen::new(handle);
    let body = text(&lines(10));
    tui.add_child(body.clone());
    tui.start().unwrap();
    wait(&tui, &terminal);

    assert_eq!(viewport(&terminal), ["line 7", "line 8", "line 9", "line 10"]);
    assert!(tui.is_following_output());

    send(&tui, &terminal, "\x1b[<64;1;1M");
    wait(&tui, &terminal);
    assert_eq!(viewport(&terminal), ["line 6", "line 7", "line 8", "line 9"]);
    assert_eq!(tui.viewport_top(), 5);
    assert!(!tui.is_following_output());

    set_text(&body, &lines(12));
    tui.request_render(false);
    wait(&tui, &terminal);
    assert_eq!(viewport(&terminal), ["line 6", "line 7", "line 8", "line 9"]);
    tui.stop();
}

#[test]
fn keeps_explicit_dock_fixed_while_transcript_scrolls() {
    let _guard = serial();
    let (terminal, handle) = concrete(20, 6);
    let tui = TuiAltScreen::new(handle);
    let transcript_text = text(&lines(8));
    let (transcript, transcript_state) = scroll_view(
        transcript_text.clone(),
        ScrollFollow::End,
        true,
        ScrollViewScrollbar::Hidden,
        None,
    );
    let dock = vstack(vec![entry(text("editor")), entry(text("footer"))]);
    tui.set_layout_root(Some(vstack(vec![
        sized(transcript.clone(), Some(StackBasis::Cells(0)), Some(1), None, Some(1)),
        sized(dock, Some(StackBasis::Auto), None, None, Some(1)),
    ])));
    tui.start().unwrap();
    wait(&tui, &terminal);
    assert_eq!(
        viewport(&terminal),
        ["line 5", "line 6", "line 7", "line 8", "editor", "footer"]
    );

    send(&tui, &terminal, "\x1b[<64;1;6M");
    wait(&tui, &terminal);
    assert_eq!(
        viewport(&terminal),
        ["line 4", "line 5", "line 6", "line 7", "editor", "footer"]
    );
    assert!(!transcript_state.borrow().is_following_end());

    set_text(&transcript_text, &lines(10));
    tui.request_render(false);
    wait(&tui, &terminal);
    assert_eq!(
        viewport(&terminal),
        ["line 4", "line 5", "line 6", "line 7", "editor", "footer"]
    );

    tui.scroll_to_bottom();
    wait(&tui, &terminal);
    assert_eq!(
        viewport(&terminal),
        ["line 7", "line 8", "line 9", "line 10", "editor", "footer"]
    );
    assert!(transcript_state.borrow().is_following_end());
    tui.stop();
}

struct InvalidateSpy {
    invalidated: Rc<Cell<bool>>,
}

impl Component for InvalidateSpy {
    fn render(&mut self, _width: usize) -> Vec<String> {
        vec!["overlay".to_string()]
    }

    fn invalidate(&mut self) {
        self.invalidated.set(true);
    }
}

#[test]
fn invalidates_overlays_with_an_explicit_layout_root() {
    let _guard = serial();
    let (_terminal, handle) = concrete(20, 4);
    let tui = TuiAltScreen::new(handle);
    let invalidated = Rc::new(Cell::new(false));
    let overlay: ComponentRef = component(InvalidateSpy {
        invalidated: Rc::clone(&invalidated),
    });
    tui.set_layout_root(Some(text("root")));
    tui.show_overlay(overlay, None);

    tui.invalidate();

    assert!(invalidated.get());
    tui.stop();
}

#[test]
fn routes_wheel_input_to_the_scroll_view_under_the_pointer() {
    let _guard = serial();
    let (terminal, handle) = concrete(20, 4);
    let tui = TuiAltScreen::new(handle);
    let (left, left_state) = scroll_view(
        text("a1\na2\na3\na4\na5\na6\na7"),
        ScrollFollow::End,
        true,
        ScrollViewScrollbar::Hidden,
        None,
    );
    let (right, right_state) = scroll_view(
        text("b1\nb2\nb3\nb4\nb5\nb6\nb7"),
        ScrollFollow::End,
        false,
        ScrollViewScrollbar::Hidden,
        None,
    );
    tui.set_layout_root(Some(hstack(vec![
        sized(left, Some(StackBasis::Cells(10)), None, Some(0), None),
        sized(right, Some(StackBasis::Cells(10)), None, Some(0), None),
    ])));
    tui.start().unwrap();
    wait(&tui, &terminal);

    send(&tui, &terminal, "\x1b[<64;15;1M");
    wait(&tui, &terminal);
    assert_eq!(left_state.borrow().scroll_top(), 3);
    assert_eq!(right_state.borrow().scroll_top(), 2);
    assert_eq!(
        viewport(&terminal),
        ["a4        b3", "a5        b4", "a6        b5", "a7        b6"]
    );
    tui.stop();
}

fn multiplexer_environment(term: Option<&str>, tmux: bool, zellij: bool, sty: bool) -> AltScreenEnvironment {
    AltScreenEnvironment {
        term: term.map(str::to_string),
        tmux,
        zellij,
        sty,
        platform: "linux".to_string(),
    }
}

#[test]
fn uses_button_motion_tracking_inside_terminal_multiplexers() {
    let _guard = serial();
    let (terminal, handle) = concrete(20, 4);
    let tui = TuiAltScreen::new(handle);
    tui.set_environment(multiplexer_environment(Some("xterm-256color"), false, false, false));
    tui.start().unwrap();
    assert!(writes(&terminal).contains("\x1b[?1003h"));
    tui.stop();

    let multiplexers = [
        multiplexer_environment(None, true, false, false),
        multiplexer_environment(Some("tmux-256color"), false, false, false),
        multiplexer_environment(None, false, true, false),
        multiplexer_environment(None, false, false, true),
        multiplexer_environment(Some("screen-256color"), false, false, false),
    ];
    for environment in multiplexers {
        let (terminal, handle) = concrete(20, 4);
        let tui = TuiAltScreen::new(handle);
        tui.set_environment(environment);
        tui.start().unwrap();
        let output = writes(&terminal);
        assert!(output.contains("\x1b[?1002h"));
        assert!(!output.contains("\x1b[?1003h"));
        assert!(output.contains("\x1b[?1006h"));
        tui.stop();
    }
}

#[test]
fn invokes_the_right_click_paste_handler_only_on_windows() {
    let _guard = serial();
    let (terminal, handle) = concrete(20, 4);
    let paste_count = Rc::new(Cell::new(0));
    let sink = Rc::clone(&paste_count);
    let tui = TuiAltScreen::with_options(
        handle,
        None,
        None,
        TuiAltScreenOptions {
            on_right_click_paste: Some(Rc::new(move || sink.set(sink.get() + 1))),
            ..TuiAltScreenOptions::default()
        },
    );
    tui.set_environment(multiplexer_environment(Some("xterm-256color"), false, false, false));
    tui.start().unwrap();

    let mut environment = multiplexer_environment(Some("xterm-256color"), false, false, false);
    environment.platform = "win32".to_string();
    tui.set_environment(environment);
    send(&tui, &terminal, "\x1b[<2;1;1M");
    send(&tui, &terminal, "\x1b[<2;1;1m");
    assert_eq!(paste_count.get(), 1);

    tui.set_environment(multiplexer_environment(Some("xterm-256color"), false, false, false));
    send(&tui, &terminal, "\x1b[<2;1;1M");
    assert_eq!(paste_count.get(), 1);
    tui.stop();
}

#[test]
fn drags_a_visible_scrollbar_thumb_and_keeps_it_visible_until_release() {
    let _guard = serial();
    let (terminal, handle) = concrete(10, 5);
    let tui = TuiAltScreen::new(handle);
    let (scroll, state) = scroll_view(
        text(&lines(20)),
        ScrollFollow::None,
        true,
        ScrollViewScrollbar::Auto,
        Some(50),
    );
    tui.set_layout_root(Some(scroll));
    tui.start().unwrap();
    wait(&tui, &terminal);
    assert!(!state.borrow().is_scrollbar_visible());

    send(&tui, &terminal, "\x1b[<65;10;1M");
    wait(&tui, &terminal);
    assert_eq!(state.borrow().scroll_top(), 1);
    assert!(state.borrow().is_scrollbar_visible());

    send(&tui, &terminal, "\x1b[<0;10;1M");
    wait(&tui, &terminal);
    tui.poll(Instant::now() + Duration::from_millis(70));
    assert!(state.borrow().is_scrollbar_visible());

    send(&tui, &terminal, "\x1b[<32;10;4M");
    wait(&tui, &terminal);
    assert_eq!(state.borrow().scroll_top(), 15);
    assert_eq!(
        viewport(&terminal),
        ["line 16", "line 17", "line 18", "line 19", "line 20"]
    );

    send(&tui, &terminal, "\x1b[<0;10;4m");
    wait(&tui, &terminal);
    assert!(state.borrow().is_scrollbar_visible());
    tui.poll(Instant::now() + Duration::from_millis(70));
    assert!(state.borrow().is_scrollbar_visible());
    send(&tui, &terminal, "\x1b[<35;9;4M");
    tui.poll(Instant::now() + Duration::from_millis(70));
    assert!(!state.borrow().is_scrollbar_visible());

    send(&tui, &terminal, "\x1b[<64;10;5M");
    wait(&tui, &terminal);
    assert_eq!(state.borrow().scroll_top(), 14);
    tui.poll(Instant::now() + Duration::from_millis(70));
    assert!(state.borrow().is_scrollbar_visible());
    send(&tui, &terminal, "\x1b[<35;9;5M");
    tui.poll(Instant::now() + Duration::from_millis(70));
    assert!(!state.borrow().is_scrollbar_visible());

    assert!(!writes(&terminal).contains("\x1b]52;c;"));
    tui.stop();
}

#[test]
fn keeps_the_scrollbar_column_selectable_while_the_thumb_is_hidden() {
    let _guard = serial();
    let (terminal, handle) = concrete(10, 2);
    let tui = TuiAltScreen::new(handle);
    let (scroll, state) = scroll_view(
        text("123456789A\nabcdefghij\nmore\nlines"),
        ScrollFollow::None,
        false,
        ScrollViewScrollbar::Auto,
        None,
    );
    tui.set_layout_root(Some(scroll));
    tui.start().unwrap();
    wait(&tui, &terminal);
    assert!(!state.borrow().is_scrollbar_visible());

    send(&tui, &terminal, "\x1b[<0;10;1M");
    send(&tui, &terminal, "\x1b[<32;10;2M");
    send(&tui, &terminal, "\x1b[<0;10;2m");
    wait(&tui, &terminal);

    let expected = format!("\x1b]52;c;{}\x07", base64_encode("A\nabcdefghij"));
    assert!(writes(&terminal).contains(&expected));
    tui.stop();
}

fn base64_encode(value: &str) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = value.as_bytes();
    let mut output = String::new();
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        output.push(ALPHABET[(first >> 2) as usize] as char);
        output.push(ALPHABET[(((first & 0x03) << 4) | (second >> 4)) as usize] as char);
        if chunk.len() > 1 {
            output.push(ALPHABET[(((second & 0x0f) << 2) | (third >> 6)) as usize] as char);
        } else {
            output.push('=');
        }
        if chunk.len() > 2 {
            output.push(ALPHABET[(third & 0x3f) as usize] as char);
        } else {
            output.push('=');
        }
    }
    output
}

#[test]
fn chains_unused_wheel_delta_to_an_outer_scroll_view() {
    let _guard = serial();
    let (terminal, handle) = concrete(20, 4);
    let tui = TuiAltScreen::with_options(
        handle,
        None,
        None,
        TuiAltScreenOptions {
            wheel_scroll_lines: Some(3),
            ..TuiAltScreenOptions::default()
        },
    );
    let (inner, inner_state) = scroll_view(
        text("i1\ni2\ni3\ni4\ni5\ni6"),
        ScrollFollow::None,
        false,
        ScrollViewScrollbar::Hidden,
        None,
    );
    let outer = ScrollView::new(
        vstack(vec![
            sized(inner, Some(StackBasis::Cells(2)), None, None, None),
            entry(text("tail1\ntail2\ntail3\ntail4\ntail5")),
        ]),
        ScrollViewOptions {
            primary: true,
            ..ScrollViewOptions::default()
        },
    );
    let outer_state = Rc::clone(outer.state());
    tui.set_layout_root(Some(component(outer)));
    tui.start().unwrap();
    wait(&tui, &terminal);

    send(&tui, &terminal, "\x1b[<65;1;1M");
    wait(&tui, &terminal);
    assert_eq!(inner_state.borrow().scroll_top(), 3);
    assert_eq!(outer_state.borrow().scroll_top(), 0);

    send(&tui, &terminal, "\x1b[<65;1;1M");
    wait(&tui, &terminal);
    assert_eq!(inner_state.borrow().scroll_top(), 4);
    assert_eq!(outer_state.borrow().scroll_top(), 2);
    tui.stop();
}

#[test]
fn supports_keyboard_viewport_navigation_with_four_rows_of_page_overlap() {
    let _guard = serial();
    let (terminal, handle) = concrete(20, 8);
    let tui = TuiAltScreen::new(handle);
    tui.add_child(text(&lines(12)));
    tui.start().unwrap();
    wait(&tui, &terminal);

    send(&tui, &terminal, "\x1b[57421u");
    send(&tui, &terminal, "\x1b[57421;1:3u");
    wait(&tui, &terminal);
    assert_eq!(
        viewport(&terminal),
        (1..=8).map(|index| format!("line {index}")).collect::<Vec<_>>()
    );

    send(&tui, &terminal, "\x1b[57422u");
    send(&tui, &terminal, "\x1b[57422;1:3u");
    wait(&tui, &terminal);
    assert_eq!(
        viewport(&terminal),
        (5..=12).map(|index| format!("line {index}")).collect::<Vec<_>>()
    );

    send(&tui, &terminal, "\x1bOH");
    wait(&tui, &terminal);
    assert_eq!(
        viewport(&terminal),
        (1..=8).map(|index| format!("line {index}")).collect::<Vec<_>>()
    );

    send(&tui, &terminal, "\x1bOF");
    wait(&tui, &terminal);
    assert_eq!(
        viewport(&terminal),
        (5..=12).map(|index| format!("line {index}")).collect::<Vec<_>>()
    );
    tui.stop();
}

#[test]
fn uses_configured_styles_for_current_and_non_current_search_matches() {
    let _guard = serial();
    let (terminal, handle) = concrete(60, 4);
    let tui = TuiAltScreen::with_options(
        handle,
        None,
        None,
        TuiAltScreenOptions {
            search_match_style: Some(Rc::new(|text: &str| format!("\x1b[41m{text}\x1b[49m"))),
            search_current_match_style: Some(Rc::new(|text: &str| format!("\x1b[42m{text}\x1b[49m"))),
            ..TuiAltScreenOptions::default()
        },
    );
    tui.add_child(text("needle first\nmiddle\nneedle second\nend"));
    tui.start().unwrap();
    wait(&tui, &terminal);

    send(&tui, &terminal, "\x1b[102;6u");
    send(&tui, &terminal, "needle");
    wait(&tui, &terminal);

    let output = writes(&terminal);
    assert!(output.contains("\x1b[42mneedle\x1b[49m"));
    assert!(output.contains("\x1b[41mneedle\x1b[49m"));
    tui.stop();
}

#[test]
fn searches_the_transcript_and_restores_editor_focus_on_close() {
    let _guard = serial();
    let (terminal, handle) = concrete(60, 8);
    let tui = TuiAltScreen::new(handle);
    let transcript_text = text(
        &(1..=12)
            .map(|index| match index {
                5 => "line 5 needle one".to_string(),
                10 => "line 10 needle two".to_string(),
                other => format!("line {other}"),
            })
            .collect::<Vec<_>>()
            .join("\n"),
    );
    let (transcript, transcript_state) = scroll_view(
        transcript_text,
        ScrollFollow::End,
        true,
        ScrollViewScrollbar::Hidden,
        None,
    );
    let (editor, editor_state, _focus) = focusable_component(&["editor"]);
    tui.set_layout_root(Some(vstack(vec![
        sized(transcript.clone(), Some(StackBasis::Cells(0)), Some(1), None, Some(1)),
        sized(editor.clone(), Some(StackBasis::Cells(1)), None, Some(0), None),
    ])));
    tui.set_focus(Some(editor));
    tui.start().unwrap();
    wait(&tui, &terminal);

    send(&tui, &terminal, "\x1b[102;6u");
    send(&tui, &terminal, "needle");
    wait(&tui, &terminal);
    assert!(!transcript_state.borrow().is_following_end());
    assert!(
        viewport(&terminal)
            .iter()
            .any(|line| line.contains("Find transcript") && line.contains("2/2"))
    );
    assert!(
        viewport(&terminal)
            .iter()
            .any(|line| line.contains("line 10 needle two"))
    );
    assert!(inputs(&editor_state).is_empty());
    assert!(writes(&terminal).contains("\x1b[1;7mneedle\x1b[22;27m"));

    for _ in 0..6 {
        send(&tui, &terminal, "\x1b[<64;1;4M");
    }
    wait(&tui, &terminal);
    assert_eq!(transcript_state.borrow().scroll_top(), 0);
    assert!(viewport(&terminal).iter().any(|line| line.contains("> needle")));

    send(&tui, &terminal, "\x07");
    wait(&tui, &terminal);
    assert!(
        viewport(&terminal)
            .iter()
            .any(|line| line.contains("Find transcript") && line.contains("1/2"))
    );
    assert!(
        viewport(&terminal)
            .iter()
            .any(|line| line.contains("line 5 needle one"))
    );

    send(&tui, &terminal, "\x1b[103;6u");
    wait(&tui, &terminal);
    assert!(
        viewport(&terminal)
            .iter()
            .any(|line| line.contains("Find transcript") && line.contains("2/2"))
    );
    assert!(
        viewport(&terminal)
            .iter()
            .any(|line| line.contains("line 10 needle two"))
    );

    send(&tui, &terminal, "\x1b");
    send(&tui, &terminal, "x");
    wait(&tui, &terminal);
    assert!(!viewport(&terminal).iter().any(|line| line.contains("Find transcript")));
    assert_eq!(inputs(&editor_state), vec!["x".to_string()]);
    tui.stop();
}

#[test]
fn scrolls_the_transcript_by_half_a_page_with_custom_bindings() {
    let _guard = serial();
    let original = get_keybindings();
    let (terminal, handle) = concrete(20, 10);
    let tui = TuiAltScreen::new(handle);
    let bindings: KeybindingsConfig = vec![
        ("tui.altScreen.halfPageUp".to_string(), vec!["ctrl+u".to_string()]),
        ("tui.altScreen.halfPageDown".to_string(), vec!["ctrl+d".to_string()]),
    ];
    set_keybindings(KeybindingsManager::new(tui_keybindings(), bindings));
    tui.add_child(text(&lines(30)));
    tui.start().unwrap();
    wait(&tui, &terminal);
    assert_eq!(tui.viewport_top(), 20);

    send(&tui, &terminal, "\x15");
    wait(&tui, &terminal);
    assert_eq!(tui.viewport_top(), 15);

    send(&tui, &terminal, "\x04");
    wait(&tui, &terminal);
    assert_eq!(tui.viewport_top(), 20);
    tui.stop();
    set_keybindings(original);
}

#[test]
fn scrolls_the_transcript_by_one_line_with_custom_bindings() {
    let _guard = serial();
    let original = get_keybindings();
    let (terminal, handle) = concrete(20, 10);
    let tui = TuiAltScreen::new(handle);
    let bindings: KeybindingsConfig = vec![
        ("tui.altScreen.lineUp".to_string(), vec!["ctrl+y".to_string()]),
        ("tui.altScreen.lineDown".to_string(), vec!["ctrl+e".to_string()]),
    ];
    set_keybindings(KeybindingsManager::new(tui_keybindings(), bindings));
    tui.add_child(text(&lines(30)));
    tui.start().unwrap();
    wait(&tui, &terminal);
    assert_eq!(tui.viewport_top(), 20);

    send(&tui, &terminal, "\x19");
    wait(&tui, &terminal);
    assert_eq!(tui.viewport_top(), 19);

    send(&tui, &terminal, "\x05");
    wait(&tui, &terminal);
    assert_eq!(tui.viewport_top(), 20);
    tui.stop();
    set_keybindings(original);
}

#[test]
fn routes_ctrl_modified_viewport_navigation_to_the_focused_component() {
    let _guard = serial();
    let (terminal, handle) = concrete(20, 6);
    let tui = TuiAltScreen::new(handle);
    let (transcript, transcript_state) = scroll_view(
        text(&lines(12)),
        ScrollFollow::End,
        true,
        ScrollViewScrollbar::Hidden,
        None,
    );
    let (editor, editor_state, _focus) = focusable_component(&["editor"]);
    tui.set_layout_root(Some(vstack(vec![
        sized(transcript.clone(), Some(StackBasis::Cells(0)), Some(1), None, Some(1)),
        sized(editor.clone(), Some(StackBasis::Cells(1)), None, Some(0), None),
    ])));
    tui.set_focus(Some(editor));
    tui.start().unwrap();
    wait(&tui, &terminal);

    send(&tui, &terminal, "\x1bOH");
    wait(&tui, &terminal);
    assert_eq!(transcript_state.borrow().scroll_top(), 0);
    assert!(inputs(&editor_state).is_empty());

    let modified_inputs = ["\x1b[1;5H", "\x1b[1;5F", "\x1b[5;5~", "\x1b[6;5~", "\x1b[57423;5u"];
    for input in modified_inputs {
        send(&tui, &terminal, input);
    }
    send(&tui, &terminal, "\x1b[57423;5:3u");
    wait(&tui, &terminal);
    assert_eq!(transcript_state.borrow().scroll_top(), 0);
    assert_eq!(inputs(&editor_state), modified_inputs);

    send(&tui, &terminal, "\x1b[6~");
    wait(&tui, &terminal);
    assert_eq!(transcript_state.borrow().scroll_top(), 1);
    assert_eq!(inputs(&editor_state), modified_inputs);
    tui.stop();
}

#[test]
fn jumps_between_osc133_semantic_prompt_markers() {
    let _guard = serial();
    let (terminal, handle) = concrete(20, 3);
    let tui = TuiAltScreen::new(handle);
    let body = (1..=4)
        .flat_map(|message| [format!("\x1b]133;A\x07message {message}"), "detail".to_string()])
        .collect::<Vec<_>>()
        .join("\n");
    tui.add_child(text(&body));
    tui.start().unwrap();
    wait(&tui, &terminal);
    assert_eq!(tui.viewport_top(), 5);

    send(&tui, &terminal, "\x1b[57419;6u");
    send(&tui, &terminal, "\x1b[57419;6:3u");
    wait(&tui, &terminal);
    assert_eq!(tui.viewport_top(), 4);
    assert_eq!(viewport(&terminal)[0], "message 3");

    send(&tui, &terminal, "\x1b[1;6A");
    wait(&tui, &terminal);
    assert_eq!(tui.viewport_top(), 2);
    assert_eq!(viewport(&terminal)[0], "message 2");

    send(&tui, &terminal, "\x1b[57420;6u");
    send(&tui, &terminal, "\x1b[57420;6:3u");
    wait(&tui, &terminal);
    assert_eq!(tui.viewport_top(), 4);
    assert_eq!(viewport(&terminal)[0], "message 3");

    send(&tui, &terminal, "\x1b[1;6B");
    wait(&tui, &terminal);
    assert_eq!(tui.viewport_top(), 5);
    assert_eq!(viewport(&terminal)[1], "message 4");
    assert!(tui.is_following_output());
    tui.stop();
}

#[test]
fn does_not_emit_kitty_graphics_or_osc133_zones_in_iterm2() {
    let _guard = serial();
    set_capabilities(TerminalCapabilities {
        images: Some(ImageProtocol::Iterm2),
        true_color: true,
        hyperlinks: true,
    });
    let (terminal, handle) = concrete(20, 3);
    let tui = TuiAltScreen::new(handle);
    tui.add_child(component(StaticComponent {
        lines: vec!["\x1b]133;B\x07\x1b]133;C\x07\x1b]133;A\x07content".to_string()],
    }));
    tui.add_child(component(Image::new(
        "AAAA",
        "image/png",
        Rc::new(ImageTheme {
            fallback_color: Rc::new(|value: &str| value.to_string()),
        }),
        ImageOptions {
            filename: Some("example.png".to_string()),
            ..ImageOptions::default()
        },
        Some(ImageDimensions {
            width_px: 10,
            height_px: 10,
        }),
    )));
    tui.start().unwrap();
    wait(&tui, &terminal);
    tui.stop();

    let output = writes(&terminal);
    assert!(!output.contains("\x1b_G"));
    assert!(!output.contains("\x1b]133;"));
    assert!(!output.contains("\x1b]1337;File="));
    assert!(output.contains("[Image:"));
    reset_capabilities_cache();
}

struct StaticComponent {
    lines: Vec<String>,
}

impl Component for StaticComponent {
    fn render(&mut self, _width: usize) -> Vec<String> {
        self.lines.clone()
    }

    fn wants_input(&self) -> bool {
        false
    }
}

#[test]
fn clears_stale_iterm2_image_placements_when_they_leave_the_viewport() {
    let _guard = serial();
    set_capabilities(TerminalCapabilities {
        images: Some(ImageProtocol::Iterm2),
        true_color: true,
        hyperlinks: true,
    });
    let (terminal, handle) = concrete(20, 3);
    let tui = TuiAltScreen::new(handle);
    let image_line = "\x1b]1337;File=inline=1;width=2;height=auto:AAAA\x07".to_string();
    tui.add_child(component(StaticComponent {
        lines: vec![
            image_line,
            String::new(),
            String::new(),
            "after".to_string(),
            "more".to_string(),
            "end".to_string(),
        ],
    }));
    tui.start().unwrap();
    wait(&tui, &terminal);
    tui.scroll_to_top();
    wait(&tui, &terminal);
    let event_count = terminal.borrow().writes().len();

    tui.scroll_by(1);
    wait(&tui, &terminal);
    assert!(terminal.borrow().writes()[event_count..].contains("\x1b[2J"));
    tui.stop();
    reset_capabilities_cache();
}

#[test]
fn crops_a_kitty_image_whose_first_line_is_above_the_viewport() {
    let _guard = serial();
    set_capabilities(kitty_caps());
    set_cell_dimensions(CellDimensions {
        width_px: 10,
        height_px: 10,
    });
    let (terminal, handle) = concrete(20, 3);
    let tui = TuiAltScreen::new(handle);
    let image_line = kitty_line(123, 2, 3);
    register_kitty(123, 2, 3, 100, 100);
    tui.add_child(component(StaticComponent {
        lines: vec![
            "before".to_string(),
            image_line,
            String::new(),
            String::new(),
            "after".to_string(),
            "end".to_string(),
        ],
    }));
    tui.start().unwrap();
    wait(&tui, &terminal);

    assert_eq!(tui.viewport_top(), 3);
    let output = writes(&terminal);
    assert!(output.contains("i=123"));
    assert!(output.contains("y=66,h=34,r=1"));
    tui.stop();
    reset_capabilities_cache();
}

#[test]
fn reuses_moved_kitty_images_without_dropping_hstack_siblings() {
    let _guard = serial();
    set_capabilities(kitty_caps());
    set_cell_dimensions(CellDimensions {
        width_px: 10,
        height_px: 10,
    });
    let (terminal, handle) = concrete(20, 6);
    let tui = TuiAltScreen::new(handle);
    let label = text("left");
    let image = component(Image::new(
        "A".repeat(8192),
        "image/png",
        Rc::new(ImageTheme {
            fallback_color: Rc::new(|value: &str| value.to_string()),
        }),
        ImageOptions::default(),
        Some(ImageDimensions {
            width_px: 100,
            height_px: 100,
        }),
    ));
    let header = text("header");
    let row = hstack(vec![
        sized(label.clone(), Some(StackBasis::Cells(10)), None, None, None),
        sized(image, Some(StackBasis::Cells(10)), None, None, None),
    ]);
    tui.set_layout_root(Some(vstack(vec![
        sized(header.clone(), Some(StackBasis::Auto), None, None, None),
        sized(row, Some(StackBasis::Cells(4)), None, None, None),
    ])));
    tui.start().unwrap();
    wait(&tui, &terminal);
    assert!(writes(&terminal).contains("\x1b_Ga=T"));

    let event_count = terminal.borrow().writes().len();
    set_text(&label, "changed");
    set_text(&header, "header\nsecond");
    tui.request_render(false);
    wait(&tui, &terminal);
    let redraw_writes = terminal.borrow().writes()[event_count..].to_string();
    let placement_index = redraw_writes.find("\x1b_Ga=p,q=2").expect("placement-only redraw");
    assert!(redraw_writes.contains("\x1b_Ga=d,d=a,q=2\x1b\\"));
    assert!(placement_index > redraw_writes.find("changed").expect("changed label"));
    assert!(!redraw_writes.contains("\x1b_Ga=T"));
    assert!(
        redraw_writes.len() < 2000,
        "expected placement-only redraw, got {} bytes",
        redraw_writes.len()
    );
    assert!(viewport(&terminal).iter().any(|line| line == "changed"));
    tui.stop();
    reset_capabilities_cache();
}

#[test]
fn retains_recently_offscreen_kitty_images_for_placement_only_reuse() {
    let _guard = serial();
    set_capabilities(kitty_caps());
    set_cell_dimensions(CellDimensions {
        width_px: 10,
        height_px: 10,
    });
    let (terminal, handle) = concrete(20, 1);
    let tui = TuiAltScreen::new(handle);
    let image_line = kitty_line(321, 2, 1);
    register_kitty(321, 2, 1, 100, 50);
    let (scroll, _state) = scroll_view(
        component(StaticComponent {
            lines: vec![image_line, "after".to_string()],
        }),
        ScrollFollow::None,
        true,
        ScrollViewScrollbar::Hidden,
        None,
    );
    tui.set_layout_root(Some(scroll));
    tui.start().unwrap();
    wait(&tui, &terminal);
    assert!(writes(&terminal).contains("\x1b_Ga=T"));

    let event_count = terminal.borrow().writes().len();
    tui.scroll_by(1);
    wait(&tui, &terminal);
    tui.scroll_by(-1);
    wait(&tui, &terminal);
    let reentry_writes = terminal.borrow().writes()[event_count..].to_string();
    assert!(reentry_writes.contains("\x1b_Ga=p,q=2"));
    assert!(!reentry_writes.contains("\x1b_Ga=T"));
    assert!(!reentry_writes.contains("\x1b_Ga=d,d=I,i=321,q=2\x1b\\"));
    tui.stop();
    reset_capabilities_cache();
}

#[test]
fn evicts_the_least_recently_visible_kitty_image_when_the_cache_is_full() {
    let _guard = serial();
    set_capabilities(kitty_caps());
    set_cell_dimensions(CellDimensions {
        width_px: 10,
        height_px: 10,
    });
    let (terminal, handle) = concrete(20, 1);
    let tui = TuiAltScreen::new(handle);
    let first_image_id = 500;
    let image_lines: Vec<String> = (0..18)
        .map(|index| {
            let image_id = first_image_id + index;
            register_kitty(image_id, 2, 1, 100, 50);
            kitty_line(image_id, 2, 1)
        })
        .collect();
    let (scroll, _state) = scroll_view(
        component(StaticComponent {
            lines: image_lines.clone(),
        }),
        ScrollFollow::None,
        true,
        ScrollViewScrollbar::Hidden,
        None,
    );
    tui.set_layout_root(Some(scroll));
    tui.start().unwrap();
    wait(&tui, &terminal);
    for _ in 1..image_lines.len() {
        tui.scroll_by(1);
        wait(&tui, &terminal);
    }
    assert!(writes(&terminal).contains(&format!("\x1b_Ga=d,d=I,i={first_image_id},q=2\x1b\\")));

    let event_count = terminal.borrow().writes().len();
    tui.scroll_to_top();
    wait(&tui, &terminal);
    assert!(terminal.borrow().writes()[event_count..].contains("\x1b_Ga=T"));
    tui.stop();
    reset_capabilities_cache();
}

#[test]
fn evicts_offscreen_kitty_images_when_decoded_raster_memory_exceeds_the_quota() {
    let _guard = serial();
    set_capabilities(kitty_caps());
    set_cell_dimensions(CellDimensions {
        width_px: 10,
        height_px: 10,
    });
    let (terminal, handle) = concrete(20, 1);
    let tui = TuiAltScreen::new(handle);
    let first_image_id = 600;
    let image_lines: Vec<String> = (0..4)
        .map(|index| {
            let image_id = first_image_id + index;
            register_kitty(image_id, 2, 1, 3840, 2160);
            kitty_line(image_id, 2, 1)
        })
        .collect();
    let (scroll, _state) = scroll_view(
        component(StaticComponent {
            lines: image_lines.clone(),
        }),
        ScrollFollow::None,
        true,
        ScrollViewScrollbar::Hidden,
        None,
    );
    tui.set_layout_root(Some(scroll));
    tui.start().unwrap();
    wait(&tui, &terminal);
    for _ in 1..image_lines.len() {
        tui.scroll_by(1);
        wait(&tui, &terminal);
    }
    assert!(writes(&terminal).contains(&format!("\x1b_Ga=d,d=I,i={first_image_id},q=2\x1b\\")));
    tui.stop();
    reset_capabilities_cache();
}

#[test]
fn opens_an_osc8_hyperlink_on_click_but_not_on_drag() {
    let _guard = serial();
    let (terminal, handle) = concrete(20, 3);
    let opened_urls: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&opened_urls);
    let tui = TuiAltScreen::with_options(
        handle,
        None,
        None,
        TuiAltScreenOptions {
            open_url: Some(Rc::new(move |url: &str| {
                sink.borrow_mut().push(url.to_string());
            })),
            ..TuiAltScreenOptions::default()
        },
    );
    let url = "https://example.com/path?q=1";
    let bel_url = "https://example.com/bel";
    let emoji_url = "https://example.com/emoji";
    tui.add_child(text(&format!(
        "{}\n\x1b]8;;{bel_url}\x07link\x1b]8;;\x07\n{}",
        hyperlink("link", url),
        hyperlink("🙂", emoji_url)
    )));
    tui.start().unwrap();
    wait(&tui, &terminal);

    send(&tui, &terminal, "\x1b[<0;2;1M");
    send(&tui, &terminal, "\x1b[<0;2;1m");
    wait(&tui, &terminal);
    assert_eq!(*opened_urls.borrow(), vec![url.to_string()]);

    send(&tui, &terminal, "\x1b[<0;2;2M");
    send(&tui, &terminal, "\x1b[<0;2;2m");
    wait(&tui, &terminal);
    assert_eq!(*opened_urls.borrow(), vec![url.to_string(), bel_url.to_string()]);

    send(&tui, &terminal, "\x1b[<0;1;3M");
    send(&tui, &terminal, "\x1b[<0;1;3m");
    wait(&tui, &terminal);
    assert_eq!(
        *opened_urls.borrow(),
        vec![url.to_string(), bel_url.to_string(), emoji_url.to_string()]
    );

    send(&tui, &terminal, "\x1b[<0;2;1M");
    send(&tui, &terminal, "\x1b[<32;4;1M");
    send(&tui, &terminal, "\x1b[<0;4;1m");
    wait(&tui, &terminal);
    assert_eq!(
        *opened_urls.borrow(),
        vec![url.to_string(), bel_url.to_string(), emoji_url.to_string()]
    );
    tui.stop();
}

#[test]
fn selects_visible_text_with_the_mouse_and_copies_it_with_osc52() {
    let _guard = serial();
    let (terminal, handle) = concrete(20, 4);
    let tui = TuiAltScreen::new(handle);
    tui.add_child(text("\x1b[1mal\x1b[0mpha\nbeta\ngamma\ndelta"));
    tui.start().unwrap();
    wait(&tui, &terminal);

    send(&tui, &terminal, "\x1b[<0;1;1M");
    send(&tui, &terminal, "\x1b[<32;4;2M");
    send(&tui, &terminal, "\x1b[<0;4;2m");
    wait(&tui, &terminal);

    let expected = format!("\x1b]52;c;{}\x07", base64_encode("alpha\nbeta"));
    let output = writes(&terminal);
    assert!(output.contains(&expected));
    assert!(output.contains("\x1b[7m"));
    assert!(output.contains("al\x1b[0m\x1b[7mpha"));
    assert!(viewport(&terminal).iter().any(|line| line.contains("Copied!")));
    tui.stop();
}

#[test]
fn does_not_append_whitespace_to_double_click_word_highlighting() {
    let _guard = serial();
    let (terminal, handle) = concrete(20, 1);
    let tui = TuiAltScreen::new(handle);
    tui.add_child(text("foo  bar"));
    tui.start().unwrap();
    wait(&tui, &terminal);

    send(&tui, &terminal, "\x1b[<0;1;1M");
    send(&tui, &terminal, "\x1b[<0;1;1m");
    send(&tui, &terminal, "\x1b[<0;3;1M");
    wait(&tui, &terminal);

    assert!(writes(&terminal).contains("foo\x1b[27m"));
    tui.stop();
}

#[test]
fn highlights_a_complete_whitespace_segment_during_a_word_drag() {
    let _guard = serial();
    let (terminal, handle) = concrete(20, 1);
    let tui = TuiAltScreen::new(handle);
    tui.add_child(text("foo  bar"));
    tui.start().unwrap();
    wait(&tui, &terminal);

    send(&tui, &terminal, "\x1b[<0;1;1M");
    send(&tui, &terminal, "\x1b[<0;1;1m");
    send(&tui, &terminal, "\x1b[<0;2;1M");
    send(&tui, &terminal, "\x1b[<32;4;1M");
    wait(&tui, &terminal);

    assert!(writes(&terminal).contains("foo  \x1b[27m"));
    tui.stop();
}

#[test]
fn selects_whole_words_on_double_click_and_lines_on_triple_click() {
    let _guard = serial();
    let (terminal, handle) = concrete(20, 2);
    let tui = TuiAltScreen::new(handle);
    tui.add_child(text("zero alpha beta\ngamma delta"));
    tui.start().unwrap();
    wait(&tui, &terminal);

    send(&tui, &terminal, "\x1b[<0;6;1M");
    send(&tui, &terminal, "\x1b[<0;6;1m");
    send(&tui, &terminal, "\x1b[<0;10;1M");
    send(&tui, &terminal, "\x1b[<0;10;1m");
    wait(&tui, &terminal);
    assert!(writes(&terminal).contains(&format!("\x1b]52;c;{}\x07", base64_encode("alpha"))));

    send(&tui, &terminal, "\x1b[<0;12;1M");
    send(&tui, &terminal, "\x1b[<0;12;1m");
    send(&tui, &terminal, "\x1b[<0;14;1M");
    send(&tui, &terminal, "\x1b[<32;3;2M");
    send(&tui, &terminal, "\x1b[<0;3;2m");
    wait(&tui, &terminal);
    assert!(writes(&terminal).contains(&format!("\x1b]52;c;{}\x07", base64_encode("beta\ngamma"))));

    send(&tui, &terminal, "\x1b[<0;7;2M");
    send(&tui, &terminal, "\x1b[<0;7;2m");
    send(&tui, &terminal, "\x1b[<0;9;2M");
    send(&tui, &terminal, "\x1b[<0;9;2m");
    send(&tui, &terminal, "\x1b[<0;11;2M");
    send(&tui, &terminal, "\x1b[<0;11;2m");
    wait(&tui, &terminal);
    assert!(writes(&terminal).contains(&format!("\x1b]52;c;{}\x07", base64_encode("gamma delta"))));
    tui.stop();
}

#[test]
fn does_not_repaint_idle_or_zero_width_selections_on_focus_loss() {
    let _guard = serial();
    let (terminal, handle) = concrete(20, 4);
    let tui = TuiAltScreen::new(handle);
    tui.add_child(text("alpha\nbeta\ngamma\ndelta"));
    tui.start().unwrap();
    wait(&tui, &terminal);

    let write_count = |terminal: &Rc<RefCell<VirtualTerminal>>| terminal.borrow().writes().len();
    let idle_write_count = write_count(&terminal);
    send(&tui, &terminal, "\x1b[O");
    send(&tui, &terminal, "\x1b[I");
    wait(&tui, &terminal);
    assert_eq!(write_count(&terminal), idle_write_count);

    send(&tui, &terminal, "\x1b[<0;1;1M");
    send(&tui, &terminal, "\x1b[<0;1;1m");
    send(&tui, &terminal, "\x1b[<32;4;2M");
    send(&tui, &terminal, "\x1b[<0;4;2m");
    wait(&tui, &terminal);
    assert!(!writes(&terminal).contains("\x1b]52;c;"));

    send(&tui, &terminal, "\x1b[<0;1;3M");
    wait(&tui, &terminal);
    let pressed_write_count = write_count(&terminal);
    send(&tui, &terminal, "\x1b[O");
    send(&tui, &terminal, "\x1b[I");
    wait(&tui, &terminal);
    assert_eq!(write_count(&terminal), pressed_write_count);
    send(&tui, &terminal, "\x1b[<32;4;2M");
    send(&tui, &terminal, "\x1b[<0;4;2m");
    wait(&tui, &terminal);
    assert!(!writes(&terminal).contains("\x1b]52;c;"));
    assert!(writes(&terminal).contains("\x1b[?1004h"));

    tui.stop();
    assert!(writes(&terminal).contains("\x1b[?1004l"));
}

#[test]
fn clears_an_active_visible_selection_on_focus_loss_and_ignores_orphan_events() {
    let _guard = serial();
    let (terminal, handle) = concrete(20, 4);
    let tui = TuiAltScreen::new(handle);
    tui.add_child(text("alpha\nbeta\ngamma\ndelta"));
    tui.start().unwrap();
    wait(&tui, &terminal);

    send(&tui, &terminal, "\x1b[<0;1;1M");
    send(&tui, &terminal, "\x1b[<32;4;2M");
    wait(&tui, &terminal);
    let focus_loss_event_count = terminal.borrow().writes().len();
    send(&tui, &terminal, "\x1b[O");
    send(&tui, &terminal, "\x1b[I");
    wait(&tui, &terminal);
    let focus_loss_writes = terminal.borrow().writes()[focus_loss_event_count..].to_string();
    assert!(focus_loss_writes.contains("alpha"));
    assert!(focus_loss_writes.contains("beta"));
    assert!(!focus_loss_writes.contains("\x1b[7m"));

    send(&tui, &terminal, "\x1b[<32;4;2M");
    send(&tui, &terminal, "\x1b[<0;4;2m");
    wait(&tui, &terminal);
    assert!(!writes(&terminal).contains("\x1b]52;c;"));
    tui.stop();
}

#[test]
fn retains_a_completed_visible_selection_across_focus_changes() {
    let _guard = serial();
    let (terminal, handle) = concrete(20, 4);
    let tui = TuiAltScreen::new(handle);
    tui.add_child(text("alpha\nbeta\ngamma\ndelta"));
    tui.start().unwrap();
    wait(&tui, &terminal);

    send(&tui, &terminal, "\x1b[<0;1;1M");
    send(&tui, &terminal, "\x1b[<32;4;2M");
    send(&tui, &terminal, "\x1b[<0;4;2m");
    wait(&tui, &terminal);
    let completed_write_count = terminal.borrow().writes().len();
    send(&tui, &terminal, "\x1b[O");
    send(&tui, &terminal, "\x1b[I");
    wait(&tui, &terminal);
    assert_eq!(terminal.borrow().writes().len(), completed_write_count);

    let redraw_event_count = terminal.borrow().writes().len();
    tui.render_now(true);
    let redraw_writes = terminal.borrow().writes()[redraw_event_count..].to_string();
    assert!(redraw_writes.contains("alpha"));
    assert!(redraw_writes.contains("beta"));
    assert!(redraw_writes.contains("\x1b[7m"));
    tui.stop();
}

#[test]
fn stacks_flash_messages_and_collapses_them_as_they_expire() {
    let _guard = serial();
    let (terminal, handle) = concrete(20, 4);
    let tui = TuiAltScreen::new(handle);
    tui.add_child(text("one\ntwo\nthree\nfour"));
    tui.start().unwrap();
    wait(&tui, &terminal);

    tui.flash("First", Some(Duration::from_millis(80)));
    tui.flash("Second", Some(Duration::from_millis(500)));
    wait(&tui, &terminal);
    let current = viewport(&terminal);
    assert!(current[0].ends_with(" First"));
    assert!(current[1].ends_with(" Second"));

    tui.poll(Instant::now() + Duration::from_millis(100));
    wait(&tui, &terminal);
    let current = viewport(&terminal);
    assert!(current[0].ends_with(" Second"));
    assert!(!current.iter().any(|line| line.contains("First")));
    tui.stop();
}

#[test]
fn auto_scrolls_and_extends_a_drag_selection_held_at_the_viewport_edge() {
    let _guard = serial();
    let (terminal, handle) = concrete(20, 4);
    let tui = TuiAltScreen::new(handle);
    tui.add_child(text(&lines(10)));
    tui.start().unwrap();
    wait(&tui, &terminal);
    assert_eq!(tui.viewport_top(), 6);

    send(&tui, &terminal, "\x1b[<0;1;3M");
    send(&tui, &terminal, "\x1b[<32;1;1M");
    tui.poll(Instant::now());
    tui.poll(Instant::now());
    wait(&tui, &terminal);

    let selection_top = tui.viewport_top();
    assert!(
        selection_top < 6,
        "expected auto-scroll above row 6, got {selection_top}"
    );
    send(&tui, &terminal, "\x1b[<0;1;1m");
    wait(&tui, &terminal);

    let mut selected_lines: Vec<String> = (selection_top..8).map(|index| format!("line {}", index + 1)).collect();
    selected_lines.push("l".to_string());
    let expected = format!("\x1b]52;c;{}\x07", base64_encode(&selected_lines.join("\n")));
    assert!(writes(&terminal).contains(&expected));
    tui.stop();
}

#[test]
fn snaps_mouse_selection_to_cjk_emoji_and_combining_grapheme_boundaries() {
    let _guard = serial();
    let (terminal, handle) = concrete(20, 2);
    let tui = TuiAltScreen::new(handle);
    tui.add_child(text("A界🙂e\u{301}Z"));
    tui.start().unwrap();
    wait(&tui, &terminal);

    let wide_selection = format!("\x1b]52;c;{}\x07", base64_encode("界🙂"));
    send(&tui, &terminal, "\x1b[<0;3;1M");
    send(&tui, &terminal, "\x1b[<32;4;1M");
    send(&tui, &terminal, "\x1b[<0;4;1m");
    wait(&tui, &terminal);
    assert_eq!(writes(&terminal).matches(&wide_selection).count(), 1);

    send(&tui, &terminal, "\x1b[<0;5;1M");
    send(&tui, &terminal, "\x1b[<32;2;1M");
    send(&tui, &terminal, "\x1b[<0;2;1m");
    wait(&tui, &terminal);
    assert_eq!(writes(&terminal).matches(&wide_selection).count(), 2);

    let combining_selection = format!("\x1b]52;c;{}\x07", base64_encode("e\u{301}Z"));
    send(&tui, &terminal, "\x1b[<0;6;1M");
    send(&tui, &terminal, "\x1b[<32;7;1M");
    send(&tui, &terminal, "\x1b[<0;7;1m");
    wait(&tui, &terminal);
    assert!(writes(&terminal).contains(&combining_selection));
    tui.stop();
}

#[test]
fn ignores_horizontal_trackpad_wheel_events() {
    let _guard = serial();
    let (terminal, handle) = concrete(20, 4);
    let tui = TuiAltScreen::new(handle);
    tui.add_child(text(&lines(8)));
    tui.start().unwrap();
    wait(&tui, &terminal);

    send(&tui, &terminal, "\x1b[<66;1;1M");
    send(&tui, &terminal, "\x1b[<67;1;1M");
    wait(&tui, &terminal);
    assert_eq!(tui.viewport_top(), 4);
    assert_eq!(viewport(&terminal), ["line 5", "line 6", "line 7", "line 8"]);
    tui.stop();
}

#[test]
fn restores_keyboard_state_before_leaving_alt_mode_and_prints_the_full_document() {
    let _guard = serial();
    let (terminal, handle) = concrete(20, 3);
    terminal.borrow_mut().set_capture_cursor_writes(true);
    let tui = TuiAltScreen::new(handle);
    tui.add_child(text("first\nsecond\nthird\nfourth\nfifth\nsixth"));
    tui.start().unwrap();
    wait(&tui, &terminal);
    tui.stop();

    let output = writes(&terminal);
    let start_index = output.find("\x1b[?2004h").expect("terminal start");
    let alt_screen_enter_index = output.find("\x1b[?1049h").expect("alt screen enter");
    let stop_index = output.find("\x1b[?2004l").expect("terminal stop");
    let mouse_disable_index = output.find("\x1b[?1006l").expect("mouse disable");
    let main_screen_restore_index = output.find("\x1b[?1049l").expect("alt screen exit");
    assert!(alt_screen_enter_index < start_index);
    assert!(mouse_disable_index < stop_index);
    assert!(main_screen_restore_index > stop_index);

    let restore = &output[main_screen_restore_index..];
    for line in ["first", "second", "third", "fourth", "fifth", "sixth"] {
        assert!(restore.contains(line));
    }
    assert!(restore.find("first").unwrap() < restore.find("sixth").unwrap());
}
