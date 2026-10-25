//! Ported from `packages/tui/test/editor.test.ts` (`Sticky column`).
use pi_tui::tui::Component;

use crate::shared_editor_host as editor_host;

use editor_host::{cursor, editor, editor_with_rows, send};
use pi_tui::components::editor::Editor;

fn position_cursor(editor: &mut Editor, line: usize, col: usize) {
    for _ in 0..20 {
        send(editor, "\x1b[A");
    }
    for _ in 0..line {
        send(editor, "\x1b[B");
    }
    send(editor, "\x01");
    for _ in 0..col {
        send(editor, "\x1b[C");
    }
}

#[test]
fn preserves_target_column_when_moving_up_through_a_shorter_line() {
    let (mut editor, _) = editor();
    editor.set_text("2222222222x222\n\n1111111111_111111111111");
    assert_eq!(cursor(&editor), (2, 23));
    send(&mut editor, "\x01");
    for _ in 0..10 {
        send(&mut editor, "\x1b[C");
    }
    assert_eq!(cursor(&editor), (2, 10));
    send(&mut editor, "\x1b[A");
    assert_eq!(cursor(&editor), (1, 0));
    send(&mut editor, "\x1b[A");
    assert_eq!(cursor(&editor), (0, 10));
}

#[test]
fn preserves_target_column_when_moving_down_through_a_shorter_line() {
    let (mut editor, _) = editor();
    editor.set_text("1111111111_111\n\n2222222222x222222222222");
    send(&mut editor, "\x1b[A");
    send(&mut editor, "\x1b[A");
    send(&mut editor, "\x01");
    for _ in 0..10 {
        send(&mut editor, "\x1b[C");
    }
    assert_eq!(cursor(&editor), (0, 10));
    send(&mut editor, "\x1b[B");
    assert_eq!(cursor(&editor), (1, 0));
    send(&mut editor, "\x1b[B");
    assert_eq!(cursor(&editor), (2, 10));
}

#[test]
fn resets_sticky_column_on_horizontal_movement_left_arrow() {
    let (mut editor, _) = editor();
    editor.set_text("1234567890\n\n1234567890");
    send(&mut editor, "\x01");
    for _ in 0..5 {
        send(&mut editor, "\x1b[C");
    }
    assert_eq!(cursor(&editor), (2, 5));
    send(&mut editor, "\x1b[A");
    send(&mut editor, "\x1b[A");
    assert_eq!(cursor(&editor), (0, 5));
    send(&mut editor, "\x1b[D");
    assert_eq!(cursor(&editor), (0, 4));
    send(&mut editor, "\x1b[B");
    send(&mut editor, "\x1b[B");
    assert_eq!(cursor(&editor), (2, 4));
}

#[test]
fn resets_sticky_column_on_horizontal_movement_right_arrow() {
    let (mut editor, _) = editor();
    editor.set_text("1234567890\n\n1234567890");
    send(&mut editor, "\x1b[A");
    send(&mut editor, "\x1b[A");
    send(&mut editor, "\x01");
    for _ in 0..5 {
        send(&mut editor, "\x1b[C");
    }
    assert_eq!(cursor(&editor), (0, 5));
    send(&mut editor, "\x1b[B");
    send(&mut editor, "\x1b[B");
    assert_eq!(cursor(&editor), (2, 5));
    send(&mut editor, "\x1b[C");
    assert_eq!(cursor(&editor), (2, 6));
    send(&mut editor, "\x1b[A");
    send(&mut editor, "\x1b[A");
    assert_eq!(cursor(&editor), (0, 6));
}

#[test]
fn resets_sticky_column_on_typing() {
    let (mut editor, _) = editor();
    editor.set_text("1234567890\n\n1234567890");
    send(&mut editor, "\x01");
    for _ in 0..8 {
        send(&mut editor, "\x1b[C");
    }
    send(&mut editor, "\x1b[A");
    send(&mut editor, "\x1b[A");
    assert_eq!(cursor(&editor), (0, 8));
    send(&mut editor, "X");
    assert_eq!(cursor(&editor), (0, 9));
    send(&mut editor, "\x1b[B");
    send(&mut editor, "\x1b[B");
    assert_eq!(cursor(&editor), (2, 9));
}

#[test]
fn resets_sticky_column_on_backspace() {
    let (mut editor, _) = editor();
    editor.set_text("1234567890\n\n1234567890");
    send(&mut editor, "\x01");
    for _ in 0..8 {
        send(&mut editor, "\x1b[C");
    }
    send(&mut editor, "\x1b[A");
    send(&mut editor, "\x1b[A");
    assert_eq!(cursor(&editor), (0, 8));
    send(&mut editor, "\x7f");
    assert_eq!(cursor(&editor), (0, 7));
    send(&mut editor, "\x1b[B");
    send(&mut editor, "\x1b[B");
    assert_eq!(cursor(&editor), (2, 7));
}

#[test]
fn resets_sticky_column_on_ctrl_a() {
    let (mut editor, _) = editor();
    editor.set_text("1234567890\n\n1234567890");
    send(&mut editor, "\x01");
    for _ in 0..8 {
        send(&mut editor, "\x1b[C");
    }
    send(&mut editor, "\x1b[A");
    send(&mut editor, "\x01");
    assert_eq!(cursor(&editor), (1, 0));
    send(&mut editor, "\x1b[A");
    assert_eq!(cursor(&editor), (0, 0));
}

#[test]
fn resets_sticky_column_on_ctrl_e() {
    let (mut editor, _) = editor();
    editor.set_text("12345\n\n1234567890");
    send(&mut editor, "\x01");
    for _ in 0..3 {
        send(&mut editor, "\x1b[C");
    }
    send(&mut editor, "\x1b[A");
    send(&mut editor, "\x1b[A");
    assert_eq!(cursor(&editor), (0, 3));
    send(&mut editor, "\x05");
    assert_eq!(cursor(&editor), (0, 5));
    send(&mut editor, "\x1b[B");
    send(&mut editor, "\x1b[B");
    assert_eq!(cursor(&editor), (2, 5));
}

#[test]
fn resets_sticky_column_on_word_movement_ctrl_left() {
    let (mut editor, _) = editor();
    editor.set_text("hello world\n\nhello world");
    assert_eq!(cursor(&editor), (2, 11));
    send(&mut editor, "\x1b[A");
    send(&mut editor, "\x1b[A");
    assert_eq!(cursor(&editor), (0, 11));
    send(&mut editor, "\x1b[1;5D");
    assert_eq!(cursor(&editor), (0, 6));
    send(&mut editor, "\x1b[B");
    send(&mut editor, "\x1b[B");
    assert_eq!(cursor(&editor), (2, 6));
}

#[test]
fn resets_sticky_column_on_word_movement_ctrl_right() {
    let (mut editor, _) = editor();
    editor.set_text("hello world\n\nhello world");
    send(&mut editor, "\x1b[A");
    send(&mut editor, "\x1b[A");
    send(&mut editor, "\x01");
    assert_eq!(cursor(&editor), (0, 0));
    send(&mut editor, "\x1b[B");
    send(&mut editor, "\x1b[B");
    assert_eq!(cursor(&editor), (2, 0));
    send(&mut editor, "\x1b[1;5C");
    assert_eq!(cursor(&editor), (2, 5));
    send(&mut editor, "\x1b[A");
    send(&mut editor, "\x1b[A");
    assert_eq!(cursor(&editor), (0, 5));
}

#[test]
fn resets_sticky_column_on_undo() {
    let (mut editor, _) = editor();
    editor.set_text("1234567890\n\n1234567890");
    send(&mut editor, "\x1b[A");
    send(&mut editor, "\x1b[A");
    send(&mut editor, "\x01");
    for _ in 0..8 {
        send(&mut editor, "\x1b[C");
    }
    assert_eq!(cursor(&editor), (0, 8));
    send(&mut editor, "\x1b[B");
    send(&mut editor, "\x1b[B");
    assert_eq!(cursor(&editor), (2, 8));
    send(&mut editor, "X");
    assert_eq!(editor.get_text(), "1234567890\n\n12345678X90");
    assert_eq!(cursor(&editor), (2, 9));
    send(&mut editor, "\x1b[A");
    send(&mut editor, "\x1b[A");
    assert_eq!(cursor(&editor), (0, 9));
    send(&mut editor, "\x1b[45;5u");
    assert_eq!(editor.get_text(), "1234567890\n\n1234567890");
    assert_eq!(cursor(&editor), (2, 8));
    send(&mut editor, "\x1b[A");
    send(&mut editor, "\x1b[A");
    assert_eq!(cursor(&editor), (0, 8));
}

#[test]
fn handles_multiple_consecutive_up_down_movements() {
    let (mut editor, _) = editor();
    editor.set_text("1234567890\nab\ncd\nef\n1234567890");
    send(&mut editor, "\x01");
    for _ in 0..7 {
        send(&mut editor, "\x1b[C");
    }
    assert_eq!(cursor(&editor), (4, 7));
    send(&mut editor, "\x1b[A");
    assert_eq!(cursor(&editor), (3, 2));
    send(&mut editor, "\x1b[A");
    assert_eq!(cursor(&editor), (2, 2));
    send(&mut editor, "\x1b[A");
    assert_eq!(cursor(&editor), (1, 2));
    send(&mut editor, "\x1b[A");
    assert_eq!(cursor(&editor), (0, 7));
    send(&mut editor, "\x1b[B");
    assert_eq!(cursor(&editor), (1, 2));
    send(&mut editor, "\x1b[B");
    assert_eq!(cursor(&editor), (2, 2));
    send(&mut editor, "\x1b[B");
    assert_eq!(cursor(&editor), (3, 2));
    send(&mut editor, "\x1b[B");
    assert_eq!(cursor(&editor), (4, 7));
}

#[test]
fn moves_correctly_through_wrapped_visual_lines_without_getting_stuck() {
    let (mut editor, _) = editor_with_rows(24);
    editor.set_text("short\n123456789012345678901234567890");
    editor.render(15);
    assert_eq!(cursor(&editor), (1, 30));
    send(&mut editor, "\x1b[A");
    assert_eq!(cursor(&editor).0, 1);
    send(&mut editor, "\x1b[A");
    assert_eq!(cursor(&editor).0, 1);
    send(&mut editor, "\x1b[A");
    assert_eq!(cursor(&editor).0, 0);
}

#[test]
fn handles_set_text_resetting_sticky_column() {
    let (mut editor, _) = editor();
    editor.set_text("1234567890\n\n1234567890");
    send(&mut editor, "\x01");
    for _ in 0..8 {
        send(&mut editor, "\x1b[C");
    }
    send(&mut editor, "\x1b[A");
    editor.set_text("abcdefghij\n\nabcdefghij");
    assert_eq!(cursor(&editor), (2, 10));
    send(&mut editor, "\x1b[A");
    send(&mut editor, "\x1b[A");
    assert_eq!(cursor(&editor), (0, 10));
}

#[test]
fn sets_preferred_visual_col_when_pressing_right_at_end_of_prompt() {
    let (mut editor, _) = editor();
    editor.set_text("111111111x1111111111\n\n333333333_");
    send(&mut editor, "\x1b[A");
    send(&mut editor, "\x1b[A");
    send(&mut editor, "\x05");
    assert_eq!(cursor(&editor), (0, 20));
    send(&mut editor, "\x1b[B");
    send(&mut editor, "\x1b[B");
    assert_eq!(cursor(&editor), (2, 10));
    send(&mut editor, "\x1b[C");
    assert_eq!(cursor(&editor), (2, 10));
    send(&mut editor, "\x1b[A");
    send(&mut editor, "\x1b[A");
    assert_eq!(cursor(&editor), (0, 10));
}

#[test]
fn handles_editor_resizes_when_preferred_visual_col_is_on_the_same_line() {
    let (mut editor, _) = editor_with_rows(24);
    editor.set_text("12345678901234567890\n\n12345678901234567890");
    send(&mut editor, "\x01");
    for _ in 0..15 {
        send(&mut editor, "\x1b[C");
    }
    send(&mut editor, "\x1b[A");
    send(&mut editor, "\x1b[A");
    assert_eq!(cursor(&editor), (0, 15));
    editor.render(12);
    send(&mut editor, "\x1b[B");
    send(&mut editor, "\x1b[B");
    assert_eq!(cursor(&editor).1, 4);
}

#[test]
fn handles_editor_resizes_when_preferred_visual_col_is_on_a_different_line() {
    let (mut editor, _) = editor_with_rows(24);
    editor.set_text("short\n12345678901234567890");
    send(&mut editor, "\x01");
    for _ in 0..15 {
        send(&mut editor, "\x1b[C");
    }
    assert_eq!(cursor(&editor), (1, 15));
    send(&mut editor, "\x1b[A");
    assert_eq!(cursor(&editor), (0, 5));
    editor.render(10);
    send(&mut editor, "\x1b[B");
    assert_eq!(cursor(&editor), (1, 8));
    send(&mut editor, "\x1b[A");
    assert_eq!(cursor(&editor), (0, 5));
    editor.render(80);
    send(&mut editor, "\x1b[B");
    assert_eq!(cursor(&editor), (1, 15));
}

#[test]
fn rewrapped_lines_target_fits_current_visual_column() {
    let (mut editor, _) = editor_with_rows(24);
    editor.set_text("abcdefghijklmnopqr\n123456789012345678");
    position_cursor(&mut editor, 0, 18);
    assert_eq!(cursor(&editor), (0, 18));
    editor.render(10);
    send(&mut editor, "\x1b[B");
    assert_eq!(cursor(&editor), (1, 8));
    editor.render(80);
    send(&mut editor, "\x1b[A");
    assert_eq!(cursor(&editor), (0, 8));
    send(&mut editor, "\x1b[B");
    assert_eq!(cursor(&editor), (1, 8));
}

#[test]
fn rewrapped_lines_target_shorter_than_current_visual_column() {
    let (mut editor, _) = editor_with_rows(24);
    editor.set_text("abcdefghijklmnopqr\n123456789012345678\nab");
    position_cursor(&mut editor, 0, 18);
    assert_eq!(cursor(&editor), (0, 18));
    editor.render(10);
    send(&mut editor, "\x1b[B");
    assert_eq!(cursor(&editor), (1, 8));
    editor.render(80);
    send(&mut editor, "\x1b[B");
    assert_eq!(cursor(&editor), (2, 2));
    send(&mut editor, "\x1b[A");
    assert_eq!(cursor(&editor), (1, 8));
}
