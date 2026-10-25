//! Ported from `packages/tui/test/editor.test.ts` (`Prompt history navigation`)
//! and `editor-history-keybindings.test.ts`.
use pi_js_compat::js_json::JsString;
use pi_tui::tui::Component;

use crate::shared_editor_host as editor_host;

use editor_host::{cursor, editor, render_plain, send};

#[test]
fn does_nothing_on_up_arrow_when_history_is_empty() {
    let (mut editor, _) = editor();
    send(&mut editor, "\x1b[A");
    assert_eq!(editor.get_text(), "");
}

#[test]
fn shows_most_recent_history_entry_on_up_arrow_when_editor_is_empty() {
    let (mut editor, _) = editor();
    editor.add_to_history("first prompt");
    editor.add_to_history("second prompt");
    send(&mut editor, "\x1b[A");
    assert_eq!(editor.get_text(), "second prompt");
}

#[test]
fn cycles_through_history_entries_on_repeated_up_arrow() {
    let (mut editor, _) = editor();
    editor.add_to_history("first");
    editor.add_to_history("second");
    editor.add_to_history("third");
    send(&mut editor, "\x1b[A");
    assert_eq!(editor.get_text(), "third");
    send(&mut editor, "\x1b[A");
    assert_eq!(editor.get_text(), "second");
    send(&mut editor, "\x1b[A");
    assert_eq!(editor.get_text(), "first");
    send(&mut editor, "\x1b[A");
    assert_eq!(editor.get_text(), "first");
}

#[test]
fn jumps_to_start_before_entering_history_from_a_non_empty_draft() {
    let (mut editor, _) = editor();
    editor.add_to_history("prompt");
    editor.set_text("draft");
    send(&mut editor, "\x1b[D");
    send(&mut editor, "\x1b[D");
    send(&mut editor, "\x1b[A");
    assert_eq!(editor.get_text(), "draft");
    assert_eq!(cursor(&editor), (0, 0));
    send(&mut editor, "\x1b[A");
    assert_eq!(editor.get_text(), "prompt");
    send(&mut editor, "\x1b[B");
    assert_eq!(editor.get_text(), "draft");
    assert_eq!(cursor(&editor), (0, 0));
}

#[test]
fn navigates_forward_through_history_with_down_arrow() {
    let (mut editor, _) = editor();
    editor.add_to_history("first");
    editor.add_to_history("second");
    editor.add_to_history("third");
    editor.set_text("draft");
    for _ in 0..4 {
        send(&mut editor, "\x1b[A");
    }
    send(&mut editor, "\x1b[B");
    assert_eq!(editor.get_text(), "second");
    send(&mut editor, "\x1b[B");
    assert_eq!(editor.get_text(), "third");
    send(&mut editor, "\x1b[B");
    assert_eq!(editor.get_text(), "draft");
}

#[test]
fn exits_history_mode_when_typing_a_character() {
    let (mut editor, _) = editor();
    editor.add_to_history("old prompt");
    send(&mut editor, "\x1b[A");
    send(&mut editor, "x");
    assert_eq!(editor.get_text(), "xold prompt");
}

#[test]
fn exits_history_mode_on_set_text() {
    let (mut editor, _) = editor();
    editor.add_to_history("first");
    editor.add_to_history("second");
    send(&mut editor, "\x1b[A");
    editor.set_text("");
    send(&mut editor, "\x1b[A");
    assert_eq!(editor.get_text(), "second");
}

#[test]
fn does_not_add_empty_strings_to_history() {
    let (mut editor, _) = editor();
    editor.add_to_history("");
    editor.add_to_history("   ");
    editor.add_to_history("valid");
    send(&mut editor, "\x1b[A");
    assert_eq!(editor.get_text(), "valid");
    send(&mut editor, "\x1b[A");
    assert_eq!(editor.get_text(), "valid");
}

#[test]
fn does_not_add_consecutive_duplicates_to_history() {
    let (mut editor, _) = editor();
    editor.add_to_history("same");
    editor.add_to_history("same");
    editor.add_to_history("same");
    send(&mut editor, "\x1b[A");
    assert_eq!(editor.get_text(), "same");
    send(&mut editor, "\x1b[A");
    assert_eq!(editor.get_text(), "same");
}

#[test]
fn allows_non_consecutive_duplicates_in_history() {
    let (mut editor, _) = editor();
    editor.add_to_history("first");
    editor.add_to_history("second");
    editor.add_to_history("first");
    send(&mut editor, "\x1b[A");
    assert_eq!(editor.get_text(), "first");
    send(&mut editor, "\x1b[A");
    assert_eq!(editor.get_text(), "second");
    send(&mut editor, "\x1b[A");
    assert_eq!(editor.get_text(), "first");
}

#[test]
fn sets_prompt_history_from_persisted_newest_first_entries() {
    let (mut editor, _) = editor();
    editor.set_history(vec![
        JsString::from("newest"),
        JsString::from("older"),
        JsString::default(),
        JsString::from("   "),
    ]);
    assert_eq!(editor.get_history(), vec!["newest", "older"]);
    send(&mut editor, "\x1b[A");
    assert_eq!(editor.get_text(), "newest");
    send(&mut editor, "\x1b[A");
    assert_eq!(editor.get_text(), "older");
}

#[test]
fn uses_cursor_movement_instead_of_history_when_editor_has_content() {
    let (mut editor, _) = editor();
    editor.add_to_history("history item");
    editor.set_text("line1\nline2");
    send(&mut editor, "\x1b[A");
    send(&mut editor, "X");
    assert_eq!(editor.get_text(), "line1X\nline2");
}

#[test]
fn limits_history_to_100_entries() {
    let (mut editor, _) = editor();
    for index in 0..105 {
        editor.add_to_history(format!("prompt {index}"));
    }
    for _ in 0..100 {
        send(&mut editor, "\x1b[A");
    }
    assert_eq!(editor.get_text(), "prompt 5");
    send(&mut editor, "\x1b[A");
    assert_eq!(editor.get_text(), "prompt 5");
}

#[test]
fn places_cursor_at_start_after_browsing_history_upward() {
    let (mut editor, _) = editor();
    editor.add_to_history("older entry");
    editor.add_to_history("line1\nline2\nline3");
    send(&mut editor, "\x1b[A");
    assert_eq!(editor.get_text(), "line1\nline2\nline3");
    assert_eq!(cursor(&editor), (0, 0));
    send(&mut editor, "\x1b[A");
    assert_eq!(editor.get_text(), "older entry");
    assert_eq!(cursor(&editor), (0, 0));
}

#[test]
fn places_cursor_at_end_after_browsing_history_downward() {
    let (mut editor, _) = editor();
    editor.add_to_history("older entry");
    editor.add_to_history("line1\nline2\nline3");
    editor.add_to_history("newer entry");
    send(&mut editor, "\x1b[A");
    send(&mut editor, "\x1b[A");
    send(&mut editor, "\x1b[A");
    send(&mut editor, "\x1b[B");
    assert_eq!(editor.get_text(), "line1\nline2\nline3");
    assert_eq!(cursor(&editor), (2, 5));
    send(&mut editor, "\x1b[B");
    assert_eq!(editor.get_text(), "newer entry");
}

#[test]
fn allows_opposite_direction_cursor_movement_within_multiline_history_entry() {
    let (mut editor, _) = editor();
    editor.add_to_history("line1\nline2\nline3");
    send(&mut editor, "\x1b[A");
    assert_eq!(cursor(&editor), (0, 0));
    send(&mut editor, "\x1b[B");
    assert_eq!(editor.get_text(), "line1\nline2\nline3");
    assert_eq!(cursor(&editor), (1, 0));
    send(&mut editor, "\x1b[A");
    assert_eq!(editor.get_text(), "line1\nline2\nline3");
    assert_eq!(cursor(&editor), (0, 0));
}

#[test]
fn shows_reverse_history_search_on_ctrl_r() {
    let (mut editor, _) = editor();
    send(&mut editor, "\x12");
    let rendered = JsString::join(render_plain(&mut editor, 40), "\n");
    assert!(rendered.contains("0/0"), "{rendered:?}");
}

#[test]
fn keeps_the_search_query_in_the_editor_while_previewing_the_selected_match() {
    let (mut editor, _) = editor();
    editor.add_to_history("open config docs");
    editor.add_to_history("fix ctrl r history");
    editor.add_to_history("check tests");
    send(&mut editor, "\x12");
    send(&mut editor, "c");
    send(&mut editor, "o");
    assert_eq!(editor.get_text(), "co");
    let rendered = JsString::join(render_plain(&mut editor, 60), "\n");
    assert!(rendered.contains("> open config docs"), "{rendered:?}");
    assert!(rendered.contains("1/1"), "{rendered:?}");
}

#[test]
fn steps_through_older_reverse_search_matches_on_repeated_ctrl_r() {
    let (mut editor, _) = editor();
    editor.add_to_history("older config note");
    editor.add_to_history("newer config fix");
    send(&mut editor, "\x12");
    send(&mut editor, "c");
    send(&mut editor, "o");
    assert!(JsString::join(render_plain(&mut editor, 60), "\n").contains("> newer config fix"));
    send(&mut editor, "\x12");
    assert!(JsString::join(render_plain(&mut editor, 60), "\n").contains("> older config note"));
    assert_eq!(editor.get_text(), "co");
}

#[test]
fn shows_each_reverse_search_match_as_a_single_first_line_row() {
    let (mut editor, _) = editor();
    editor.add_to_history("older config note\nhidden detail");
    editor.add_to_history("```\n  wrapped config prompt\n```");
    editor.add_to_history("newer config fix");
    editor.add_to_history("unrelated prompt");
    send(&mut editor, "\x12");
    send(&mut editor, "c");
    send(&mut editor, "o");
    let raw = JsString::join(editor.render(60), "\n");
    let rendered = pi_tui::utils::strip_terminal_sequences(&raw);
    assert!(rendered.contains("> newer config fix"));
    assert!(rendered.contains("  wrapped config prompt"));
    assert!(!rendered.contains("    wrapped config prompt"));
    assert!(rendered.contains("  older config note"));
    assert!(!rendered.contains("```"));
    assert!(!rendered.contains("hidden detail"));
    assert!(rendered.contains("1/3"));
    assert!(raw.contains("\x1b[7mco\x1b[27m"));
}

#[test]
fn previews_the_matching_line_from_multiline_reverse_search_results() {
    let (mut editor, _) = editor();
    editor.add_to_history("Goal set - starting work\nsay hello twice");
    send(&mut editor, "\x12");
    send(&mut editor, "h");
    send(&mut editor, "e");
    let rendered = JsString::join(render_plain(&mut editor, 60), "\n");
    assert!(rendered.contains("> say hello twice"), "{rendered:?}");
    assert!(!rendered.contains("Goal set - starting work"));
}

#[test]
fn limits_reverse_search_rendering_to_20_rows_while_keeping_all_matches_navigable() {
    let (mut editor, _) = editor();
    for index in 1..=25 {
        editor.add_to_history(format!("match {index}"));
    }
    send(&mut editor, "\x12");
    send(&mut editor, "m");
    send(&mut editor, "a");
    let rendered = JsString::join(render_plain(&mut editor, 60), "\n");
    let visible = rendered
        .split('\n')
        .filter(|line| line.units().get(1) == Some(&32) && line.slice(2, line.len()).starts_with("match "))
        .count();
    assert_eq!(visible, 20, "{rendered:?}");
    assert!(rendered.contains("1/25"));
    assert!(!rendered.contains("match 5"));

    for _ in 0..20 {
        send(&mut editor, "\x12");
    }
    let stepped = JsString::join(render_plain(&mut editor, 60), "\n");
    assert!(stepped.contains("> match 5"), "{stepped:?}");
    assert!(stepped.contains("21/25"));
}

#[test]
fn accepts_the_selected_reverse_search_match_on_enter() {
    let (mut editor, _) = editor();
    editor.add_to_history("open config docs");
    send(&mut editor, "\x12");
    send(&mut editor, "c");
    send(&mut editor, "o");
    send(&mut editor, "\r");
    assert_eq!(editor.get_text(), "open config docs");
}
