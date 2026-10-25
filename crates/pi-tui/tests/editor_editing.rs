//! Ported from `packages/tui/test/editor.test.ts` (`public state accessors`,
//! `Backslash+Enter`, `Kitty CSI-u`, `Unicode text editing`, `Kill ring`,
//! `Undo`, `Character jump`).
use pi_js_compat::js_json::JsString;
use pi_tui::tui::Component;

use crate::shared_editor_host as editor_host;

use editor_host::{cursor, editor, send};
use pi_tui::components::editor::EditorTheme;
use pi_tui::tui::Focusable;

fn typed(
    text: &str,
) -> (
    pi_tui::components::editor::Editor,
    std::rc::Rc<std::cell::RefCell<editor_host::FakeHostState>>,
) {
    let (mut editor, state) = editor();
    for ch in text.chars() {
        send(&mut editor, ch.to_string());
    }
    (editor, state)
}

#[test]
fn returns_cursor_position() {
    let (mut editor, _) = editor();
    assert_eq!(cursor(&editor), (0, 0));
    send(&mut editor, "a");
    send(&mut editor, "b");
    send(&mut editor, "c");
    assert_eq!(cursor(&editor), (0, 3));
    send(&mut editor, "\x1b[D");
    assert_eq!(cursor(&editor), (0, 2));
}

#[test]
fn returns_lines_as_a_defensive_copy() {
    let (mut editor, _) = editor();
    editor.set_text("a\nb");
    let mut lines = editor.get_lines();
    assert_eq!(lines, vec!["a", "b"]);
    lines[0] = JsString::from("mutated");
    assert_eq!(editor.get_lines(), vec!["a", "b"]);
}

#[test]
fn inserts_backslash_immediately_without_buffering() {
    let (mut editor, _) = editor();
    send(&mut editor, "\\");
    assert_eq!(editor.get_text(), "\\");
}

#[test]
fn converts_standalone_backslash_to_newline_on_enter() {
    let (mut editor, _) = editor();
    send(&mut editor, "\\");
    send(&mut editor, "\r");
    assert_eq!(editor.get_text(), "\n");
}

#[test]
fn inserts_backslash_normally_when_followed_by_other_characters() {
    let (mut editor, _) = editor();
    send(&mut editor, "\\");
    send(&mut editor, "x");
    assert_eq!(editor.get_text(), "\\x");
}

#[test]
fn does_not_trigger_newline_when_backslash_is_not_immediately_before_cursor() {
    let (mut editor, _) = editor();
    let submitted = std::rc::Rc::new(std::cell::Cell::new(false));
    let flag = std::rc::Rc::clone(&submitted);
    editor.on_submit = Some(std::rc::Rc::new(move |_| flag.set(true)));
    send(&mut editor, "\\");
    send(&mut editor, "x");
    send(&mut editor, "\r");
    assert!(submitted.get());
}

#[test]
fn only_removes_one_backslash_when_multiple_are_present() {
    let (mut editor, _) = editor();
    send(&mut editor, "\\");
    send(&mut editor, "\\");
    send(&mut editor, "\\");
    assert_eq!(editor.get_text(), "\\\\\\");
    send(&mut editor, "\r");
    assert_eq!(editor.get_text(), "\\\\\n");
}

#[test]
fn ignores_printable_csi_u_sequences_with_unsupported_modifiers() {
    let (mut editor, _) = editor();
    send(&mut editor, "\x1b[99;9u");
    assert_eq!(editor.get_text(), "");
}

#[test]
fn inserts_shifted_csi_u_letters_as_text() {
    let (mut editor, _) = editor();
    send(&mut editor, "\x1b[69;2u");
    assert_eq!(editor.get_text(), "E");
}

#[test]
fn inserts_shifted_xterm_modify_other_keys_letters_as_text() {
    let (mut editor, _) = editor();
    send(&mut editor, "\x1b[27;2;69~");
    assert_eq!(editor.get_text(), "E");
}

#[test]
fn inserts_mixed_ascii_umlauts_and_emojis_as_literal_text() {
    let (mut editor, _) = editor();
    for ch in ["H", "e", "l", "l", "o", " ", "ä", "ö", "ü", " ", "😀"] {
        send(&mut editor, ch);
    }
    assert_eq!(editor.get_text(), "Hello äöü 😀");
}

#[test]
fn deletes_single_code_unit_unicode_characters_with_backspace() {
    let (mut editor, _) = editor();
    send(&mut editor, "ä");
    send(&mut editor, "ö");
    send(&mut editor, "ü");
    send(&mut editor, "\x7f");
    assert_eq!(editor.get_text(), "äö");
}

#[test]
fn deletes_multi_code_unit_emojis_with_single_backspace() {
    let (mut editor, _) = editor();
    send(&mut editor, "😀");
    send(&mut editor, "👍");
    send(&mut editor, "\x7f");
    assert_eq!(editor.get_text(), "😀");
}

#[test]
fn inserts_characters_at_the_correct_position_after_cursor_movement_over_umlauts() {
    let (mut editor, _) = editor();
    send(&mut editor, "ä");
    send(&mut editor, "ö");
    send(&mut editor, "ü");
    send(&mut editor, "\x1b[D");
    send(&mut editor, "\x1b[D");
    send(&mut editor, "x");
    assert_eq!(editor.get_text(), "äxöü");
}

#[test]
fn moves_cursor_across_multi_code_unit_emojis_with_single_arrow_key() {
    let (mut editor, _) = editor();
    send(&mut editor, "😀");
    send(&mut editor, "👍");
    send(&mut editor, "🎉");
    send(&mut editor, "\x1b[D");
    send(&mut editor, "\x1b[D");
    send(&mut editor, "x");
    assert_eq!(editor.get_text(), "😀x👍🎉");
}

#[test]
fn preserves_umlauts_across_line_breaks() {
    let (mut editor, _) = editor();
    for ch in ["ä", "ö", "ü", "\n", "Ä", "Ö", "Ü"] {
        send(&mut editor, ch);
    }
    assert_eq!(editor.get_text(), "äöü\nÄÖÜ");
}

#[test]
fn replaces_the_entire_document_with_unicode_text_via_set_text() {
    let (mut editor, _) = editor();
    editor.set_text("Hällö Wörld! 😀 äöüÄÖÜß");
    assert_eq!(editor.get_text(), "Hällö Wörld! 😀 äöüÄÖÜß");
}

#[test]
fn moves_cursor_to_document_start_on_ctrl_a_and_inserts_at_the_beginning() {
    let (mut editor, _) = editor();
    send(&mut editor, "a");
    send(&mut editor, "b");
    send(&mut editor, "\x01");
    send(&mut editor, "x");
    assert_eq!(editor.get_text(), "xab");
}

#[test]
fn deletes_words_correctly_with_ctrl_w_and_alt_backspace() {
    let (mut editor, _) = editor();
    editor.set_text("foo bar baz");
    send(&mut editor, "\x17");
    assert_eq!(editor.get_text(), "foo bar ");

    editor.set_text("foo bar   ");
    send(&mut editor, "\x17");
    assert_eq!(editor.get_text(), "foo ");

    editor.set_text("foo bar...");
    send(&mut editor, "\x17");
    assert_eq!(editor.get_text(), "foo bar");

    editor.set_text("foo.bar");
    send(&mut editor, "\x17");
    assert_eq!(editor.get_text(), "foo.");

    editor.set_text("foo:bar");
    send(&mut editor, "\x17");
    assert_eq!(editor.get_text(), "foo:");

    editor.set_text("line one\nline two");
    send(&mut editor, "\x17");
    assert_eq!(editor.get_text(), "line one\nline ");

    editor.set_text("line one\n");
    send(&mut editor, "\x17");
    assert_eq!(editor.get_text(), "line one");

    editor.set_text("foo 😀😀 bar");
    send(&mut editor, "\x17");
    assert_eq!(editor.get_text(), "foo 😀😀 ");
    send(&mut editor, "\x17");
    assert_eq!(editor.get_text(), "foo ");

    editor.set_text("foo bar");
    send(&mut editor, "\x1b\x7f");
    assert_eq!(editor.get_text(), "foo ");
}

#[test]
fn navigates_words_correctly_with_ctrl_left_and_right() {
    let (mut editor, _) = editor();
    editor.set_text("foo bar... baz");

    send(&mut editor, "\x1b[1;5D");
    assert_eq!(cursor(&editor), (0, 11));
    send(&mut editor, "\x1b[1;5D");
    assert_eq!(cursor(&editor), (0, 7));
    send(&mut editor, "\x1b[1;5D");
    assert_eq!(cursor(&editor), (0, 4));

    send(&mut editor, "\x1b[1;5C");
    assert_eq!(cursor(&editor), (0, 7));
    send(&mut editor, "\x1b[1;5C");
    assert_eq!(cursor(&editor), (0, 10));
    send(&mut editor, "\x1b[1;5C");
    assert_eq!(cursor(&editor), (0, 14));

    editor.set_text("   foo bar");
    send(&mut editor, "\x01");
    send(&mut editor, "\x1b[1;5C");
    assert_eq!(cursor(&editor), (0, 6));

    editor.set_text("foo.bar baz");
    send(&mut editor, "\x1b[1;5D");
    assert_eq!(cursor(&editor), (0, 8));
    send(&mut editor, "\x1b[1;5D");
    assert_eq!(cursor(&editor), (0, 4));
    send(&mut editor, "\x1b[1;5D");
    assert_eq!(cursor(&editor), (0, 3));

    send(&mut editor, "\x01");
    send(&mut editor, "\x1b[1;5C");
    assert_eq!(cursor(&editor), (0, 3));
    send(&mut editor, "\x1b[1;5C");
    assert_eq!(cursor(&editor), (0, 4));
    send(&mut editor, "\x1b[1;5C");
    assert_eq!(cursor(&editor), (0, 7));
}

#[test]
fn stops_at_fullwidth_chinese_punctuation() {
    let (mut editor, _) = editor();
    editor.set_text("你好，世界");

    send(&mut editor, "\x1b[1;5D");
    assert_eq!(cursor(&editor), (0, 3));
    send(&mut editor, "\x1b[1;5D");
    assert_eq!(cursor(&editor), (0, 2));
    send(&mut editor, "\x1b[1;5D");
    assert_eq!(cursor(&editor), (0, 0));

    send(&mut editor, "\x1b[1;5C");
    assert_eq!(cursor(&editor), (0, 2));
    send(&mut editor, "\x1b[1;5C");
    assert_eq!(cursor(&editor), (0, 3));
    send(&mut editor, "\x1b[1;5C");
    assert_eq!(cursor(&editor), (0, 5));
}

#[test]
fn handles_mixed_cjk_and_ascii_word_movement() {
    let (mut editor, _) = editor();
    editor.set_text("hello你好，world世界");

    send(&mut editor, "\x1b[1;5D");
    assert_eq!(cursor(&editor), (0, 13));
    send(&mut editor, "\x1b[1;5D");
    assert_eq!(cursor(&editor), (0, 8));
    send(&mut editor, "\x1b[1;5D");
    assert_eq!(cursor(&editor), (0, 7));
    send(&mut editor, "\x1b[1;5D");
    assert_eq!(cursor(&editor), (0, 5));
    send(&mut editor, "\x1b[1;5D");
    assert_eq!(cursor(&editor), (0, 0));

    send(&mut editor, "\x1b[1;5C");
    assert_eq!(cursor(&editor), (0, 5));
    send(&mut editor, "\x1b[1;5C");
    assert_eq!(cursor(&editor), (0, 7));
    send(&mut editor, "\x1b[1;5C");
    assert_eq!(cursor(&editor), (0, 8));
    send(&mut editor, "\x1b[1;5C");
    assert_eq!(cursor(&editor), (0, 13));
    send(&mut editor, "\x1b[1;5C");
    assert_eq!(cursor(&editor), (0, 15));
}

#[test]
fn ctrl_w_saves_deleted_text_to_kill_ring_and_ctrl_y_yanks_it() {
    let (mut editor, _) = editor();
    editor.set_text("foo bar baz");
    send(&mut editor, "\x17");
    assert_eq!(editor.get_text(), "foo bar ");
    send(&mut editor, "\x01");
    send(&mut editor, "\x19");
    assert_eq!(editor.get_text(), "bazfoo bar ");
}

#[test]
fn ctrl_u_saves_deleted_text_to_kill_ring() {
    let (mut editor, _) = editor();
    editor.set_text("hello world");
    send(&mut editor, "\x01");
    for _ in 0..6 {
        send(&mut editor, "\x1b[C");
    }
    send(&mut editor, "\x15");
    assert_eq!(editor.get_text(), "world");
    send(&mut editor, "\x19");
    assert_eq!(editor.get_text(), "hello world");
}

#[test]
fn ctrl_k_saves_deleted_text_to_kill_ring() {
    let (mut editor, _) = editor();
    editor.set_text("hello world");
    send(&mut editor, "\x01");
    send(&mut editor, "\x0b");
    assert_eq!(editor.get_text(), "");
    send(&mut editor, "\x19");
    assert_eq!(editor.get_text(), "hello world");
}

#[test]
fn ctrl_y_does_nothing_when_kill_ring_is_empty() {
    let (mut editor, _) = editor();
    editor.set_text("test");
    send(&mut editor, "\x19");
    assert_eq!(editor.get_text(), "test");
}

#[test]
fn alt_y_cycles_through_kill_ring_after_ctrl_y() {
    let (mut editor, _) = editor();
    editor.set_text("first");
    send(&mut editor, "\x17");
    editor.set_text("second");
    send(&mut editor, "\x17");
    editor.set_text("third");
    send(&mut editor, "\x17");
    assert_eq!(editor.get_text(), "");

    send(&mut editor, "\x19");
    assert_eq!(editor.get_text(), "third");
    send(&mut editor, "\x1by");
    assert_eq!(editor.get_text(), "second");
    send(&mut editor, "\x1by");
    assert_eq!(editor.get_text(), "first");
    send(&mut editor, "\x1by");
    assert_eq!(editor.get_text(), "third");
}

#[test]
fn alt_y_does_nothing_if_not_preceded_by_yank() {
    let (mut editor, _) = editor();
    editor.set_text("test");
    send(&mut editor, "\x17");
    editor.set_text("other");
    send(&mut editor, "x");
    assert_eq!(editor.get_text(), "otherx");
    send(&mut editor, "\x1by");
    assert_eq!(editor.get_text(), "otherx");
}

#[test]
fn alt_y_does_nothing_if_kill_ring_has_one_entry() {
    let (mut editor, _) = editor();
    editor.set_text("only");
    send(&mut editor, "\x17");
    send(&mut editor, "\x19");
    assert_eq!(editor.get_text(), "only");
    send(&mut editor, "\x1by");
    assert_eq!(editor.get_text(), "only");
}

#[test]
fn consecutive_ctrl_w_accumulates_into_one_kill_ring_entry() {
    let (mut editor, _) = editor();
    editor.set_text("one two three");
    send(&mut editor, "\x17");
    send(&mut editor, "\x17");
    send(&mut editor, "\x17");
    assert_eq!(editor.get_text(), "");
    send(&mut editor, "\x19");
    assert_eq!(editor.get_text(), "one two three");
}

#[test]
fn ctrl_u_accumulates_multiline_deletes_including_newlines() {
    let (mut editor, _) = editor();
    editor.set_text("line1\nline2\nline3");
    send(&mut editor, "\x15");
    assert_eq!(editor.get_text(), "line1\nline2\n");
    send(&mut editor, "\x15");
    assert_eq!(editor.get_text(), "line1\nline2");
    send(&mut editor, "\x15");
    assert_eq!(editor.get_text(), "line1\n");
    send(&mut editor, "\x15");
    assert_eq!(editor.get_text(), "line1");
    send(&mut editor, "\x15");
    assert_eq!(editor.get_text(), "");
    send(&mut editor, "\x19");
    assert_eq!(editor.get_text(), "line1\nline2\nline3");
}

#[test]
fn backward_deletions_prepend_forward_deletions_append_during_accumulation() {
    let (mut editor, _) = editor();
    editor.set_text("prefix|suffix");
    send(&mut editor, "\x01");
    for _ in 0..6 {
        send(&mut editor, "\x1b[C");
    }
    send(&mut editor, "\x0b");
    send(&mut editor, "\x0b");
    assert_eq!(editor.get_text(), "prefix");
    send(&mut editor, "\x19");
    assert_eq!(editor.get_text(), "prefix|suffix");
}

#[test]
fn non_delete_actions_break_kill_accumulation() {
    let (mut editor, _) = editor();
    editor.set_text("foo bar baz");
    send(&mut editor, "\x17");
    assert_eq!(editor.get_text(), "foo bar ");
    send(&mut editor, "x");
    assert_eq!(editor.get_text(), "foo bar x");
    send(&mut editor, "\x17");
    assert_eq!(editor.get_text(), "foo bar ");
    send(&mut editor, "\x19");
    assert_eq!(editor.get_text(), "foo bar x");
    send(&mut editor, "\x1by");
    assert_eq!(editor.get_text(), "foo bar baz");
}

#[test]
fn non_yank_actions_break_alt_y_chain() {
    let (mut editor, _) = editor();
    editor.set_text("first");
    send(&mut editor, "\x17");
    editor.set_text("second");
    send(&mut editor, "\x17");
    editor.set_text("");
    send(&mut editor, "\x19");
    assert_eq!(editor.get_text(), "second");
    send(&mut editor, "x");
    assert_eq!(editor.get_text(), "secondx");
    send(&mut editor, "\x1by");
    assert_eq!(editor.get_text(), "secondx");
}

#[test]
fn kill_ring_rotation_persists_after_cycling() {
    let (mut editor, _) = editor();
    editor.set_text("first");
    send(&mut editor, "\x17");
    editor.set_text("second");
    send(&mut editor, "\x17");
    editor.set_text("third");
    send(&mut editor, "\x17");
    editor.set_text("");

    send(&mut editor, "\x19");
    send(&mut editor, "\x1by");
    assert_eq!(editor.get_text(), "second");
    send(&mut editor, "x");
    editor.set_text("");
    send(&mut editor, "\x19");
    assert_eq!(editor.get_text(), "second");
}

#[test]
fn consecutive_deletions_across_lines_coalesce_into_one_entry() {
    let (mut editor, _) = editor();
    editor.set_text("1\n2\n3");
    send(&mut editor, "\x17");
    assert_eq!(editor.get_text(), "1\n2\n");
    send(&mut editor, "\x17");
    assert_eq!(editor.get_text(), "1\n2");
    send(&mut editor, "\x17");
    assert_eq!(editor.get_text(), "1\n");
    send(&mut editor, "\x17");
    assert_eq!(editor.get_text(), "1");
    send(&mut editor, "\x17");
    assert_eq!(editor.get_text(), "");
    send(&mut editor, "\x19");
    assert_eq!(editor.get_text(), "1\n2\n3");
}

#[test]
fn ctrl_k_at_line_end_deletes_newline_and_coalesces() {
    let (mut editor, _) = editor();
    editor.set_text("");
    send(&mut editor, "a");
    send(&mut editor, "b");
    send(&mut editor, "\n");
    send(&mut editor, "c");
    send(&mut editor, "d");
    send(&mut editor, "\x1b[A");
    send(&mut editor, "\x05");
    send(&mut editor, "\x0b");
    assert_eq!(editor.get_text(), "abcd");
    send(&mut editor, "\x0b");
    assert_eq!(editor.get_text(), "ab");
    send(&mut editor, "\x19");
    assert_eq!(editor.get_text(), "ab\ncd");
}

#[test]
fn handles_yank_in_middle_of_text() {
    let (mut editor, _) = editor();
    editor.set_text("word");
    send(&mut editor, "\x17");
    editor.set_text("hello world");
    send(&mut editor, "\x01");
    for _ in 0..6 {
        send(&mut editor, "\x1b[C");
    }
    send(&mut editor, "\x19");
    assert_eq!(editor.get_text(), "hello wordworld");
}

#[test]
fn handles_yank_pop_in_middle_of_text() {
    let (mut editor, _) = editor();
    editor.set_text("FIRST");
    send(&mut editor, "\x17");
    editor.set_text("SECOND");
    send(&mut editor, "\x17");

    editor.set_text("hello world");
    send(&mut editor, "\x01");
    for _ in 0..6 {
        send(&mut editor, "\x1b[C");
    }
    send(&mut editor, "\x19");
    assert_eq!(editor.get_text(), "hello SECONDworld");
    send(&mut editor, "\x1by");
    assert_eq!(editor.get_text(), "hello FIRSTworld");
}

#[test]
fn multiline_yank_and_yank_pop_in_middle_of_text() {
    let (mut editor, _) = editor();
    editor.set_text("SINGLE");
    send(&mut editor, "\x17");

    editor.set_text("A\nB");
    send(&mut editor, "\x15");
    send(&mut editor, "\x15");
    send(&mut editor, "\x15");

    editor.set_text("hello world");
    send(&mut editor, "\x01");
    for _ in 0..6 {
        send(&mut editor, "\x1b[C");
    }
    send(&mut editor, "\x19");
    assert_eq!(editor.get_text(), "hello A\nBworld");
    send(&mut editor, "\x1by");
    assert_eq!(editor.get_text(), "hello SINGLEworld");
}

#[test]
fn alt_d_deletes_word_forward_and_saves_to_kill_ring() {
    let (mut editor, _) = editor();
    editor.set_text("hello world test");
    send(&mut editor, "\x01");
    send(&mut editor, "\x1bd");
    assert_eq!(editor.get_text(), " world test");
    send(&mut editor, "\x1bd");
    assert_eq!(editor.get_text(), " test");
    send(&mut editor, "\x19");
    assert_eq!(editor.get_text(), "hello world test");
}

#[test]
fn alt_d_at_end_of_line_deletes_newline() {
    let (mut editor, _) = editor();
    editor.set_text("line1\nline2");
    send(&mut editor, "\x1b[A");
    send(&mut editor, "\x05");
    send(&mut editor, "\x1bd");
    assert_eq!(editor.get_text(), "line1line2");
    send(&mut editor, "\x19");
    assert_eq!(editor.get_text(), "line1\nline2");
}

#[test]
fn does_nothing_when_undo_stack_is_empty() {
    let (mut editor, _) = editor();
    send(&mut editor, "\x1b[45;5u");
    assert_eq!(editor.get_text(), "");
}

#[test]
fn coalesces_consecutive_word_characters_into_one_undo_unit() {
    let (mut editor, _) = typed("hello world");
    send(&mut editor, "\x1b[45;5u");
    assert_eq!(editor.get_text(), "hello");
    send(&mut editor, "\x1b[45;5u");
    assert_eq!(editor.get_text(), "");
}

#[test]
fn undoes_spaces_one_at_a_time() {
    let (mut editor, _) = typed("hello  ");
    send(&mut editor, "\x1b[45;5u");
    assert_eq!(editor.get_text(), "hello ");
    send(&mut editor, "\x1b[45;5u");
    assert_eq!(editor.get_text(), "hello");
    send(&mut editor, "\x1b[45;5u");
    assert_eq!(editor.get_text(), "");
}

#[test]
fn undoes_newlines_and_signals_next_word_to_capture_state() {
    let (mut editor, _) = typed("hello\nworld");
    send(&mut editor, "\x1b[45;5u");
    assert_eq!(editor.get_text(), "hello\n");
    send(&mut editor, "\x1b[45;5u");
    assert_eq!(editor.get_text(), "hello");
    send(&mut editor, "\x1b[45;5u");
    assert_eq!(editor.get_text(), "");
}

#[test]
fn undoes_backspace() {
    let (mut editor, _) = typed("hello");
    send(&mut editor, "\x7f");
    assert_eq!(editor.get_text(), "hell");
    send(&mut editor, "\x1b[45;5u");
    assert_eq!(editor.get_text(), "hello");
}

#[test]
fn undoes_forward_delete() {
    let (mut editor, _) = typed("hello");
    send(&mut editor, "\x01");
    send(&mut editor, "\x1b[C");
    send(&mut editor, "\x1b[3~");
    assert_eq!(editor.get_text(), "hllo");
    send(&mut editor, "\x1b[45;5u");
    assert_eq!(editor.get_text(), "hello");
}

#[test]
fn undoes_ctrl_w_delete_word_backward() {
    let (mut editor, _) = typed("hello world");
    send(&mut editor, "\x17");
    assert_eq!(editor.get_text(), "hello ");
    send(&mut editor, "\x1b[45;5u");
    assert_eq!(editor.get_text(), "hello world");
}

#[test]
fn undoes_ctrl_k_delete_to_line_end() {
    let (mut editor, _) = typed("hello world");
    send(&mut editor, "\x01");
    for _ in 0..6 {
        send(&mut editor, "\x1b[C");
    }
    send(&mut editor, "\x0b");
    assert_eq!(editor.get_text(), "hello ");
    send(&mut editor, "\x1b[45;5u");
    assert_eq!(editor.get_text(), "hello world");
    send(&mut editor, "|");
    assert_eq!(editor.get_text(), "hello |world");
}

#[test]
fn undoes_ctrl_u_delete_to_line_start() {
    let (mut editor, _) = typed("hello world");
    send(&mut editor, "\x01");
    for _ in 0..6 {
        send(&mut editor, "\x1b[C");
    }
    send(&mut editor, "\x15");
    assert_eq!(editor.get_text(), "world");
    send(&mut editor, "\x1b[45;5u");
    assert_eq!(editor.get_text(), "hello world");
}

#[test]
fn undoes_yank() {
    let (mut editor, _) = typed("hello ");
    send(&mut editor, "\x17");
    send(&mut editor, "\x19");
    assert_eq!(editor.get_text(), "hello ");
    send(&mut editor, "\x1b[45;5u");
    assert_eq!(editor.get_text(), "");
}

#[test]
fn undoes_single_line_paste_atomically() {
    let (mut editor, _) = editor();
    editor.set_text("hello world");
    send(&mut editor, "\x01");
    for _ in 0..5 {
        send(&mut editor, "\x1b[C");
    }
    send(&mut editor, "\x1b[200~beep boop\x1b[201~");
    assert_eq!(editor.get_text(), "hellobeep boop world");
    send(&mut editor, "\x1b[45;5u");
    assert_eq!(editor.get_text(), "hello world");
    send(&mut editor, "|");
    assert_eq!(editor.get_text(), "hello| world");
}

#[test]
fn decodes_csi_u_ctrl_letter_sequences_inside_bracketed_paste() {
    let (mut editor, _) = editor();
    send(&mut editor, "\x1b[200~line1\x1b[106;5uline2\x1b[106;5uline3\x1b[201~");
    assert_eq!(editor.get_text(), "line1\nline2\nline3");
}

#[test]
fn undoes_multiline_paste_atomically() {
    let (mut editor, _) = editor();
    editor.set_text("hello world");
    send(&mut editor, "\x01");
    for _ in 0..5 {
        send(&mut editor, "\x1b[C");
    }
    send(&mut editor, "\x1b[200~line1\nline2\nline3\x1b[201~");
    assert_eq!(editor.get_text(), "helloline1\nline2\nline3 world");
    send(&mut editor, "\x1b[45;5u");
    assert_eq!(editor.get_text(), "hello world");
    send(&mut editor, "|");
    assert_eq!(editor.get_text(), "hello| world");
}

#[test]
fn undoes_insert_text_at_cursor_atomically() {
    let (mut editor, _) = editor();
    editor.set_text("hello world");
    send(&mut editor, "\x01");
    for _ in 0..5 {
        send(&mut editor, "\x1b[C");
    }
    editor.insert_text_at_cursor("/tmp/image.png");
    assert_eq!(editor.get_text(), "hello/tmp/image.png world");
    send(&mut editor, "\x1b[45;5u");
    assert_eq!(editor.get_text(), "hello world");
    send(&mut editor, "|");
    assert_eq!(editor.get_text(), "hello| world");
}

#[test]
fn insert_text_at_cursor_handles_multiline_text() {
    let (mut editor, _) = editor();
    editor.set_text("hello world");
    send(&mut editor, "\x01");
    for _ in 0..5 {
        send(&mut editor, "\x1b[C");
    }
    editor.insert_text_at_cursor("line1\nline2\nline3");
    assert_eq!(editor.get_text(), "helloline1\nline2\nline3 world");
    assert_eq!(cursor(&editor), (2, 5));
    send(&mut editor, "\x1b[45;5u");
    assert_eq!(editor.get_text(), "hello world");
}

#[test]
fn insert_text_at_cursor_normalizes_crlf_and_cr_line_endings() {
    let (mut editor, _) = editor();
    editor.set_text("");
    editor.insert_text_at_cursor("a\r\nb\r\nc");
    assert_eq!(editor.get_text(), "a\nb\nc");
    send(&mut editor, "\x1b[45;5u");
    assert_eq!(editor.get_text(), "");
    editor.insert_text_at_cursor("x\ry\rz");
    assert_eq!(editor.get_text(), "x\ny\nz");
}

#[test]
fn undoes_set_text_to_empty_string() {
    let (mut editor, _) = typed("hello world");
    editor.set_text("");
    assert_eq!(editor.get_text(), "");
    send(&mut editor, "\x1b[45;5u");
    assert_eq!(editor.get_text(), "hello world");
}

#[test]
fn clears_undo_stack_on_submit() {
    let (mut editor, _) = typed("hello");
    let submitted = std::rc::Rc::new(std::cell::RefCell::new(JsString::default()));
    let value = std::rc::Rc::clone(&submitted);
    editor.on_submit = Some(std::rc::Rc::new(move |text| {
        *value.borrow_mut() = text.clone();
    }));
    send(&mut editor, "\r");
    assert_eq!(*submitted.borrow(), "hello");
    assert_eq!(editor.get_text(), "");
    send(&mut editor, "\x1b[45;5u");
    assert_eq!(editor.get_text(), "");
}

#[test]
fn exits_history_browsing_mode_on_undo() {
    let (mut editor, _) = typed("world");
    editor.add_to_history("hello");
    send(&mut editor, "\x17");
    assert_eq!(editor.get_text(), "");
    send(&mut editor, "\x1b[A");
    assert_eq!(editor.get_text(), "hello");
    send(&mut editor, "\x1b[45;5u");
    assert_eq!(editor.get_text(), "");
    send(&mut editor, "\x1b[45;5u");
    assert_eq!(editor.get_text(), "world");
}

#[test]
fn undo_restores_to_pre_history_state_even_after_multiple_history_navigations() {
    let (mut editor, _) = typed("current");
    editor.add_to_history("first");
    editor.add_to_history("second");
    editor.add_to_history("third");
    send(&mut editor, "\x17");
    assert_eq!(editor.get_text(), "");
    send(&mut editor, "\x1b[A");
    assert_eq!(editor.get_text(), "third");
    send(&mut editor, "\x1b[A");
    assert_eq!(editor.get_text(), "second");
    send(&mut editor, "\x1b[A");
    assert_eq!(editor.get_text(), "first");
    send(&mut editor, "\x1b[45;5u");
    assert_eq!(editor.get_text(), "");
    send(&mut editor, "\x1b[45;5u");
    assert_eq!(editor.get_text(), "current");
}

#[test]
fn cursor_movement_starts_new_undo_unit() {
    let (mut editor, _) = typed("hello world");
    for _ in 0..5 {
        send(&mut editor, "\x1b[D");
    }
    send(&mut editor, "l");
    send(&mut editor, "o");
    send(&mut editor, "l");
    assert_eq!(editor.get_text(), "hello lolworld");
    send(&mut editor, "\x1b[45;5u");
    assert_eq!(editor.get_text(), "hello world");
    send(&mut editor, "|");
    assert_eq!(editor.get_text(), "hello |world");
}

#[test]
fn no_op_delete_operations_do_not_push_undo_snapshots() {
    let (mut editor, _) = typed("hello");
    send(&mut editor, "\x17");
    assert_eq!(editor.get_text(), "");
    send(&mut editor, "\x17");
    send(&mut editor, "\x17");
    send(&mut editor, "\x1b[45;5u");
    assert_eq!(editor.get_text(), "hello");
}

#[test]
fn jumps_forward_to_first_occurrence_of_character_on_same_line() {
    let (mut editor, _) = editor();
    editor.set_text("hello world");
    send(&mut editor, "\x01");
    send(&mut editor, "\x1d");
    send(&mut editor, "o");
    assert_eq!(cursor(&editor), (0, 4));
}

#[test]
fn jumps_forward_to_next_occurrence_after_cursor() {
    let (mut editor, _) = editor();
    editor.set_text("hello world");
    send(&mut editor, "\x01");
    for _ in 0..4 {
        send(&mut editor, "\x1b[C");
    }
    send(&mut editor, "\x1d");
    send(&mut editor, "o");
    assert_eq!(cursor(&editor), (0, 7));
}

#[test]
fn jumps_forward_across_multiple_lines() {
    let (mut editor, _) = editor();
    editor.set_text("abc\ndef\nghi");
    send(&mut editor, "\x1b[A");
    send(&mut editor, "\x1b[A");
    send(&mut editor, "\x01");
    send(&mut editor, "\x1d");
    send(&mut editor, "g");
    assert_eq!(cursor(&editor), (2, 0));
}

#[test]
fn jumps_backward_to_first_occurrence_before_cursor_on_same_line() {
    let (mut editor, _) = editor();
    editor.set_text("hello world");
    assert_eq!(cursor(&editor), (0, 11));
    send(&mut editor, "\x1b\x1d");
    send(&mut editor, "o");
    assert_eq!(cursor(&editor), (0, 7));
}

#[test]
fn jumps_backward_across_multiple_lines() {
    let (mut editor, _) = editor();
    editor.set_text("abc\ndef\nghi");
    assert_eq!(cursor(&editor), (2, 3));
    send(&mut editor, "\x1b\x1d");
    send(&mut editor, "a");
    assert_eq!(cursor(&editor), (0, 0));
}

#[test]
fn does_nothing_when_character_is_not_found_forward() {
    let (mut editor, _) = editor();
    editor.set_text("hello world");
    send(&mut editor, "\x01");
    send(&mut editor, "\x1d");
    send(&mut editor, "z");
    assert_eq!(cursor(&editor), (0, 0));
}

#[test]
fn does_nothing_when_character_is_not_found_backward() {
    let (mut editor, _) = editor();
    editor.set_text("hello world");
    assert_eq!(cursor(&editor), (0, 11));
    send(&mut editor, "\x1b\x1d");
    send(&mut editor, "z");
    assert_eq!(cursor(&editor), (0, 11));
}

#[test]
fn jump_is_case_sensitive() {
    let (mut editor, _) = editor();
    editor.set_text("Hello World");
    send(&mut editor, "\x01");
    send(&mut editor, "\x1d");
    send(&mut editor, "h");
    assert_eq!(cursor(&editor), (0, 0));
    send(&mut editor, "\x1d");
    send(&mut editor, "W");
    assert_eq!(cursor(&editor), (0, 6));
}

#[test]
fn cancels_jump_mode_when_ctrl_bracket_is_pressed_again() {
    let (mut editor, _) = editor();
    editor.set_text("hello world");
    send(&mut editor, "\x01");
    send(&mut editor, "\x1d");
    send(&mut editor, "\x1d");
    send(&mut editor, "o");
    assert_eq!(editor.get_text(), "ohello world");
}

#[test]
fn cancels_jump_mode_on_escape_and_processes_the_escape() {
    let (mut editor, _) = editor();
    editor.set_text("hello world");
    send(&mut editor, "\x01");
    send(&mut editor, "\x1d");
    send(&mut editor, "\x1b");
    assert_eq!(cursor(&editor), (0, 0));
    send(&mut editor, "o");
    assert_eq!(editor.get_text(), "ohello world");
}

#[test]
fn cancels_backward_jump_mode_when_ctrl_alt_bracket_is_pressed_again() {
    let (mut editor, _) = editor();
    editor.set_text("hello world");
    assert_eq!(cursor(&editor), (0, 11));
    send(&mut editor, "\x1b\x1d");
    send(&mut editor, "\x1b\x1d");
    send(&mut editor, "o");
    assert_eq!(editor.get_text(), "hello worldo");
}

#[test]
fn jump_searches_for_special_characters() {
    let (mut editor, _) = editor();
    editor.set_text("foo(bar) = baz;");
    send(&mut editor, "\x01");
    send(&mut editor, "\x1d");
    send(&mut editor, "(");
    assert_eq!(cursor(&editor), (0, 3));
    send(&mut editor, "\x1d");
    send(&mut editor, "=");
    assert_eq!(cursor(&editor), (0, 9));
}

#[test]
fn jump_handles_empty_text_gracefully() {
    let (mut editor, _) = editor();
    editor.set_text("");
    send(&mut editor, "\x1d");
    send(&mut editor, "x");
    assert_eq!(cursor(&editor), (0, 0));
}

#[test]
fn jump_resets_last_action() {
    let (mut editor, _) = editor();
    editor.set_text("hello world");
    send(&mut editor, "\x01");
    send(&mut editor, "x");
    assert_eq!(editor.get_text(), "xhello world");
    send(&mut editor, "\x1d");
    send(&mut editor, "o");
    send(&mut editor, "Y");
    assert_eq!(editor.get_text(), "xhellYo world");
    send(&mut editor, "\x1b[45;5u");
    assert_eq!(editor.get_text(), "xhello world");
}

#[test]
fn focusable_trait_tracks_focus() {
    let (mut editor, _) = editor();
    assert!(!editor.focused());
    editor.set_focused(true);
    assert!(editor.focused());
}

#[test]
fn custom_theme_border_color_is_used() {
    let (host, _) = editor_host::fake_host(24);
    let mut theme = EditorTheme::plain();
    theme.border_color = std::rc::Rc::new(|text| JsString::from("\x1b[35m") + text + "\x1b[39m");
    let mut editor =
        pi_tui::components::editor::Editor::new(host, theme, pi_tui::components::editor::EditorOptions::default());
    let lines = editor.render(10);
    assert!(lines[0].starts_with("\x1b[35m─\x1b[39m"));
}
