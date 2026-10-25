//! Ported from `packages/tui/test/editor.test.ts` (`Scroll indicators`,
//! `Grapheme-aware text wrapping`).
use pi_js_compat::js_json::JsString;
use pi_tui::tui::Component;

use crate::shared_editor_host as editor_host;

use editor_host::{editor, editor_with_rows, plain_theme, send};
use pi_tui::components::editor::{Editor, EditorOptions};
use pi_tui::utils::{strip_terminal_sequences, visible_width};

fn content_lines(lines: &[JsString]) -> Vec<JsString> {
    lines[1..lines.len() - 1]
        .iter()
        .map(|line| strip_terminal_sequences(line).trim())
        .collect()
}

#[test]
fn approved_scroll_indicators_keep_color_resets_and_fit_width() {
    let width = 10;
    let border_color: pi_tui::components::StyleFn =
        std::rc::Rc::new(|text: &JsString| JsString::from("\x1b[35m") + text + "\x1b[39m");
    let (host, _) = editor_host::fake_host(24);
    let mut theme = plain_theme();
    theme.border_color = std::rc::Rc::clone(&border_color);
    let mut editor = Editor::new(host, theme, EditorOptions::default());
    editor.set_text(
        (0..20)
            .map(|index| format!("line {index}"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    editor.render(width);
    for _ in 0..10 {
        send(&mut editor, "\x1b[A");
    }
    let lines = editor.render(width);
    let top = lines.first().unwrap();
    let bottom = lines.last().unwrap();
    assert!(strip_terminal_sequences(top).starts_with("─── ↑"));
    assert!(strip_terminal_sequences(bottom).starts_with("─── ↓"));
    assert_eq!(*top, "\x1b[35m─── ↑ 9\x1b[0m...\x1b[0m\x1b[39m");
    assert_eq!(*bottom, "\x1b[35m─── ↓ 4\x1b[0m...\x1b[0m\x1b[39m");
    for line in &lines {
        assert_eq!(visible_width(line), width, "line exceeds width: {line:?}");
    }
}

#[test]
fn renders_an_optional_prompt_prefix_before_the_first_input_line() {
    let (host, _) = editor_host::fake_host(24);
    let mut theme = plain_theme();
    theme.prompt_prefix = Some(std::rc::Rc::new(|text| JsString::from("\x1b[36m") + text + "\x1b[0m"));
    let mut editor = Editor::new(host, theme, EditorOptions::default());
    editor.set_text("hello");
    let lines = editor.render(12);
    let content = strip_terminal_sequences(&lines[1]);
    assert!(content.starts_with("› hello"), "{content:?}");
    assert_eq!(visible_width(&lines[1]), 12);
}

#[test]
fn wraps_lines_correctly_when_text_contains_wide_emojis() {
    let (mut editor, _) = editor();
    let width = 20;
    editor.set_text("Hello ✅ World");
    let lines = editor.render(width);
    for line in &lines[1..lines.len() - 1] {
        assert_eq!(visible_width(line), width);
    }
}

#[test]
fn wraps_long_text_with_emojis_at_correct_positions() {
    let (mut editor, _) = editor();
    let width = 10;
    editor.set_text("✅✅✅✅✅✅");
    let lines = editor.render(width);
    for line in &lines[1..lines.len() - 1] {
        assert_eq!(visible_width(line), width);
    }
}

#[test]
fn renders_isolated_thai_and_lao_am_clusters_without_width_drift() {
    for text in ["ำabc", "ຳabc"] {
        let (mut editor, _) = editor();
        editor.set_text(text);
        for line in editor.render(8) {
            assert_eq!(visible_width(&line), 8, "width drift for {text:?}: {line:?}");
        }
    }
}

#[test]
fn wraps_cjk_characters_correctly() {
    let (mut editor, _) = editor();
    let width = 11;
    editor.set_text("日本語テスト");
    let lines = editor.render(width);
    for line in &lines[1..lines.len() - 1] {
        assert_eq!(visible_width(line), width);
    }
    let content = content_lines(&lines);
    assert_eq!(content.len(), 2);
    assert_eq!(content[0], "日本語テス");
    assert_eq!(content[1], "ト");
}

#[test]
fn handles_mixed_ascii_and_wide_characters_in_wrapping() {
    let (mut editor, _) = editor();
    let width = 16;
    editor.set_text("Test ✅ OK 日本");
    let lines = editor.render(width);
    let content = &lines[1..lines.len() - 1];
    assert_eq!(content.len(), 1);
    assert_eq!(visible_width(&content[0]), width);
}

#[test]
fn renders_cursor_correctly_on_wide_characters() {
    let (mut editor, _) = editor();
    editor.set_text("A✅B");
    let lines = editor.render(20);
    assert!(lines[1].contains("\x1b[7m"));
    assert_eq!(visible_width(&lines[1]), 20);
}

#[test]
fn does_not_exceed_terminal_width_with_emoji_at_wrap_boundary() {
    let (mut editor, _) = editor();
    let width = 11;
    editor.set_text("0123456789✅");
    let lines = editor.render(width);
    for line in &lines[1..lines.len() - 1] {
        assert!(visible_width(line) <= width, "line exceeds max: {line:?}");
    }
}

#[test]
fn shows_cursor_at_end_of_line_before_wrap_and_wraps_on_next_char() {
    for padding_x in [0, 1] {
        let width = 10;
        let (host, _) = editor_host::fake_host(24);
        let mut editor = Editor::new(
            host,
            plain_theme(),
            EditorOptions {
                padding_x: Some(padding_x),
                ..EditorOptions::default()
            },
        );
        for _ in 0..9 {
            send(&mut editor, "a");
        }
        let lines = editor.render(width + padding_x);
        let content = &lines[1..lines.len() - 1];
        assert_eq!(content.len(), 1);
        assert!(content[0].ends_with("\x1b[7m \x1b[0m"));
        send(&mut editor, "a");
        let lines = editor.render(width + padding_x);
        assert_eq!(&lines[1..lines.len() - 1].len(), &2);
    }
}

#[test]
fn reports_prompt_column_after_editor_padding() {
    let (host, _) = editor_host::fake_host(24);
    let mut theme = plain_theme();
    theme.prompt_prefix = Some(std::rc::Rc::new(|text| text.clone()));
    let mut editor = Editor::new(
        host,
        theme,
        EditorOptions {
            padding_x: Some(3),
            ..EditorOptions::default()
        },
    );
    editor.render(40);
    assert_eq!(editor.get_prompt_position(), Some((1, 3)));
}

#[test]
fn keeps_prompt_cell_at_the_first_wrapped_editor_row() {
    let (host, state) = editor_host::fake_host(24);
    let mut theme = plain_theme();
    theme.prompt_prefix = Some(std::rc::Rc::new(|text| text.clone()));
    let mut editor = Editor::new(host, theme, EditorOptions::default());
    editor.set_text("a long line that wraps across multiple editor rows");
    let rendered = editor.render(20);
    assert!(rendered.len() > 3);
    assert_eq!(editor.get_prompt_position(), Some((1, 0)));
    editor.set_screen_origin(4, 3);
    editor.set_working(true);
    let state = state.borrow();
    let last = state.writes.last().unwrap();
    assert_eq!((last.row, last.col), (5, 3));
}

#[test]
fn does_not_write_a_prompt_cell_when_vertical_scrolling_hides_the_prompt() {
    let (host, state) = editor_host::fake_host(10);
    let mut theme = plain_theme();
    theme.prompt_prefix = Some(std::rc::Rc::new(|text| text.clone()));
    let mut editor = Editor::new(host, theme, EditorOptions::default());
    editor.set_text(
        (0..20)
            .map(|index| format!("line {index}"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    editor.render(40);
    editor.set_screen_origin(2, 1);
    assert_eq!(editor.get_prompt_position(), None);
    editor.set_working(true);
    editor.poll_working(std::time::Instant::now() + std::time::Duration::from_secs(1));
    assert_eq!(state.borrow().writes.len(), 0);
}

#[test]
fn stops_fixed_cell_writes_after_screen_origin_is_cleared() {
    let (host, state) = editor_host::fake_host(24);
    let mut theme = plain_theme();
    theme.prompt_prefix = Some(std::rc::Rc::new(|text| text.clone()));
    let mut editor = Editor::new(host, theme, EditorOptions::default());
    editor.render(40);
    editor.set_screen_origin(2, 1);
    editor.set_working(true);
    let writes = state.borrow().writes.len();
    editor.clear_screen_origin();
    editor.poll_working(std::time::Instant::now() + std::time::Duration::from_secs(1));
    assert_eq!(state.borrow().writes.len(), writes);
}

#[test]
fn renders_cursor_marker_when_focused() {
    let (mut editor, _) = editor();
    editor.set_text("hi");
    let unfocused = editor.render(20);
    assert!(!unfocused[1].contains(pi_tui::tui::CURSOR_MARKER));
    editor.set_focused(true);
    let focused = editor.render(20);
    assert!(focused[1].contains(pi_tui::tui::CURSOR_MARKER));
}

#[test]
fn renders_history_search_lines_above_the_border() {
    let (mut editor, _) = editor();
    editor.add_to_history("alpha");
    send(&mut editor, "\x12");
    send(&mut editor, "a");
    let lines = editor.render(40);
    assert!(strip_terminal_sequences(&lines[0]).contains("> alpha"));
}

#[test]
fn renders_empty_editor_with_borders_and_cursor() {
    let (mut editor, _) = editor();
    let lines = editor.render(10);
    assert_eq!(lines.len(), 3);
    assert_eq!(visible_width(&lines[0]), 10);
    assert_eq!(visible_width(&lines[2]), 10);
}

#[test]
fn custom_padding_is_clamped_to_width() {
    let (host, _) = editor_host::fake_host(24);
    let mut editor = Editor::new(
        host,
        plain_theme(),
        EditorOptions {
            padding_x: Some(100),
            ..EditorOptions::default()
        },
    );
    editor.set_text("hello");
    for line in editor.render(10) {
        assert_eq!(visible_width(&line), 10);
    }
}

#[test]
fn scroll_indicator_handles_narrow_widths() {
    let (mut editor, state) = editor_with_rows(5);
    editor.set_text(
        (0..20)
            .map(|index| format!("line {index}"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    editor.render(3);
    for _ in 0..5 {
        send(&mut editor, "\x1b[A");
    }
    let lines = editor.render(3);
    for line in &lines {
        assert!(visible_width(line) <= 3, "{line:?}");
    }
    assert_eq!(lines.last().unwrap(), "\x1b[0m...\x1b[0m");
    assert!(state.borrow().rows > 0);
}

use pi_tui::tui::Focusable;
