//! Ported from `packages/tui/test/editor.test.ts` (`Paste marker atomic behavior`).
use pi_js_compat::js_json::JsString;
use pi_tui::tui::Component;

use crate::shared_editor_host as editor_host;

use std::cell::RefCell;
use std::rc::Rc;

use editor_host::{cursor, editor, editor_with_rows, send};
use pi_tui::components::editor::Editor;
use pi_tui::utils::visible_width;

fn paste_with_marker(editor: &mut Editor) -> JsString {
    let big_content = "line\n".repeat(20).trim_end().to_string();
    send(editor, format!("\x1b[200~{big_content}\x1b[201~"));
    editor.get_text()
}

fn big_paste(tag: &str) -> String {
    (0..12)
        .map(|index| format!("{tag}{index}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn submitted_editor() -> (Editor, Rc<RefCell<JsString>>) {
    let (mut editor, _) = editor();
    let submitted = Rc::new(RefCell::new(JsString::default()));
    let value = Rc::clone(&submitted);
    editor.on_submit = Some(Rc::new(move |text| {
        *value.borrow_mut() = text.clone();
    }));
    (editor, submitted)
}

#[test]
fn creates_a_paste_marker_for_large_pastes() {
    let (mut editor, _) = editor();
    let text = paste_with_marker(&mut editor);
    assert!(text.contains("[paste #"));
    assert!(text.contains(" lines]"));
}

#[test]
fn treats_paste_marker_as_single_unit_for_right_arrow() {
    let (mut editor, _) = editor();
    send(&mut editor, "A");
    paste_with_marker(&mut editor);
    send(&mut editor, "B");
    send(&mut editor, "\x01");
    assert_eq!(cursor(&editor), (0, 0));
    send(&mut editor, "\x1b[C");
    assert_eq!(cursor(&editor), (0, 1));
    send(&mut editor, "\x1b[C");
    let marker = editor.get_text().slice(1, editor.get_text().find('B').unwrap());
    assert_eq!(cursor(&editor), (0, 1 + marker.len()));
    send(&mut editor, "\x1b[C");
    assert_eq!(cursor(&editor), (0, 1 + marker.len() + 1));
}

#[test]
fn treats_paste_marker_as_single_unit_for_left_arrow() {
    let (mut editor, _) = editor();
    send(&mut editor, "A");
    paste_with_marker(&mut editor);
    send(&mut editor, "B");
    send(&mut editor, "\x1b[D");
    let marker = editor.get_text().slice(1, editor.get_text().find('B').unwrap());
    assert_eq!(cursor(&editor), (0, 1 + marker.len()));
    send(&mut editor, "\x1b[D");
    assert_eq!(cursor(&editor), (0, 1));
    send(&mut editor, "\x1b[D");
    assert_eq!(cursor(&editor), (0, 0));
}

#[test]
fn treats_paste_marker_as_single_unit_for_backspace() {
    let (mut editor, _) = editor();
    send(&mut editor, "A");
    paste_with_marker(&mut editor);
    send(&mut editor, "B");
    let marker = editor.get_text().slice(1, editor.get_text().find('B').unwrap());
    send(&mut editor, "\x01");
    send(&mut editor, "\x1b[C");
    send(&mut editor, "\x1b[C");
    assert_eq!(cursor(&editor), (0, 1 + marker.len()));
    send(&mut editor, "\x7f");
    assert_eq!(editor.get_text(), "AB");
    assert_eq!(cursor(&editor), (0, 1));
}

#[test]
fn treats_paste_marker_as_single_unit_for_forward_delete() {
    let (mut editor, _) = editor();
    send(&mut editor, "A");
    paste_with_marker(&mut editor);
    send(&mut editor, "B");
    send(&mut editor, "\x01");
    send(&mut editor, "\x1b[C");
    send(&mut editor, "\x1b[3~");
    assert_eq!(editor.get_text(), "AB");
    assert_eq!(cursor(&editor), (0, 1));
}

#[test]
fn treats_paste_marker_as_single_unit_for_word_movement() {
    let (mut editor, _) = editor();
    send(&mut editor, "X");
    send(&mut editor, " ");
    paste_with_marker(&mut editor);
    send(&mut editor, " ");
    send(&mut editor, "Y");
    let text = editor.get_text();
    let marker = text.slice(2, text.find(" Y").unwrap());
    send(&mut editor, "\x01");
    send(&mut editor, "\x1b[1;5C");
    assert_eq!(cursor(&editor), (0, 1));
    send(&mut editor, "\x1b[1;5C");
    assert_eq!(cursor(&editor), (0, 2 + marker.len()));
}

#[test]
fn undo_restores_marker_after_backspace_deletion() {
    let (mut editor, _) = editor();
    send(&mut editor, "A");
    paste_with_marker(&mut editor);
    send(&mut editor, "B");
    let text_before = editor.get_text();
    send(&mut editor, "\x01");
    send(&mut editor, "\x1b[C");
    send(&mut editor, "\x1b[C");
    send(&mut editor, "\x7f");
    assert_eq!(editor.get_text(), "AB");
    send(&mut editor, "\x1b[45;5u");
    assert_eq!(editor.get_text(), text_before);
}

#[test]
fn undo_after_paste_marker_deletion_restores_the_paste_registry() {
    let (mut editor, submitted) = submitted_editor();
    let paste = big_paste("alpha");
    send(&mut editor, format!("\x1b[200~{paste}\x1b[201~"));
    send(&mut editor, "\x7f");
    send(&mut editor, "\x1b[45;5u");
    send(&mut editor, "\r");
    assert_eq!(*submitted.borrow(), paste);
}

#[test]
fn undo_after_deleting_the_first_of_two_paste_markers_restores_both_registry_entries() {
    let (mut editor, submitted) = submitted_editor();
    let paste_a = big_paste("alpha");
    let paste_b = big_paste("beta");
    send(&mut editor, format!("\x1b[200~{paste_a}\x1b[201~"));
    send(&mut editor, format!("\x1b[200~{paste_b}\x1b[201~"));
    send(&mut editor, "\x01");
    send(&mut editor, "\x1b[C");
    send(&mut editor, "\x7f");
    send(&mut editor, "\x1b[45;5u");
    send(&mut editor, "\r");
    assert_eq!(*submitted.borrow(), format!("{paste_a}{paste_b}"));
}

#[test]
fn renumbers_the_paste_registry_in_ascending_id_order_when_markers_are_out_of_order_in_text() {
    let (mut editor, submitted) = submitted_editor();
    let paste_a = big_paste("alpha");
    let paste_b = big_paste("beta");
    let paste_c = big_paste("gamma");
    send(&mut editor, format!("\x1b[200~{paste_a}\x1b[201~"));
    send(&mut editor, "\x01");
    send(&mut editor, format!("\x1b[200~{paste_b}\x1b[201~"));
    send(&mut editor, "\x01");
    send(&mut editor, format!("\x1b[200~{paste_c}\x1b[201~"));
    send(&mut editor, "\x05");
    send(&mut editor, "\x7f");
    send(&mut editor, "\r");
    assert_eq!(*submitted.borrow(), format!("{paste_c}{paste_b}"));
}

#[test]
fn undo_after_set_text_restores_paste_markers_and_registry() {
    let (mut editor, submitted) = submitted_editor();
    let paste = big_paste("alpha");
    send(&mut editor, format!("\x1b[200~{paste}\x1b[201~"));
    editor.set_text("replacement");
    send(&mut editor, "\x1b[45;5u");
    send(&mut editor, "\r");
    assert_eq!(*submitted.borrow(), paste);
}

#[test]
fn handles_multiple_paste_markers_in_same_line() {
    let (mut editor, _) = editor();
    paste_with_marker(&mut editor);
    send(&mut editor, " ");
    paste_with_marker(&mut editor);
    let text = editor.get_text();
    let first_end = text.find(" lines]").unwrap() + " lines]".len();
    let first = text.slice(0, first_end);
    let second = text.slice(first_end + 1, text.len());
    assert_eq!(first.len() + 1 + second.len(), text.len());
    send(&mut editor, "\x01");
    send(&mut editor, "\x1b[C");
    assert_eq!(cursor(&editor), (0, first.len()));
    send(&mut editor, "\x1b[C");
    assert_eq!(cursor(&editor), (0, first.len() + 1));
    send(&mut editor, "\x1b[C");
    assert_eq!(cursor(&editor), (0, first.len() + 1 + second.len()));
}

#[test]
fn does_not_treat_manually_typed_marker_like_text_as_atomic() {
    let (mut editor, _) = editor();
    let fake_marker = "[paste #99 +5 lines]";
    for ch in fake_marker.chars() {
        send(&mut editor, ch.to_string());
    }
    assert_eq!(editor.get_text(), fake_marker);
    send(&mut editor, "\x01");
    send(&mut editor, "\x1b[C");
    assert_eq!(cursor(&editor), (0, 1));
}

#[test]
fn does_not_crash_when_paste_marker_is_wider_than_terminal_width() {
    let (mut editor, _) = editor();
    let big_content = "line\n".repeat(47).trim_end().to_string();
    send(&mut editor, format!("\x1b[200~{big_content}\x1b[201~"));
    let text = editor.get_text();
    let marker = text.slice(0, text.find(" lines]").unwrap() + " lines]".len());
    assert!(visible_width(marker) > 8);
    for line in editor.render(8) {
        assert!(visible_width(&line) <= 8, "{line:?}");
    }
}

#[test]
fn does_not_crash_when_text_plus_paste_marker_exceeds_width_with_cursor_on_marker() {
    let (mut editor, _) = editor();
    for _ in 0..35 {
        send(&mut editor, "b");
    }
    let big_content = "line\n".repeat(27).trim_end().to_string();
    send(&mut editor, format!("\x1b[200~{big_content}\x1b[201~"));
    for _ in 0..4 {
        send(&mut editor, "b");
    }
    for _ in 0..5 {
        send(&mut editor, "\x1b[D");
    }
    for line in editor.render(54) {
        assert!(visible_width(&line) <= 54, "{line:?}");
    }
}

#[test]
fn word_wrap_line_rechecks_overflow_after_backtracking_to_wrap_opportunity() {
    let (mut editor, _) = editor();
    send(&mut editor, " ");
    for _ in 0..35 {
        send(&mut editor, "b");
    }
    let big_content = "line\n".repeat(27).trim_end().to_string();
    send(&mut editor, format!("\x1b[200~{big_content}\x1b[201~"));
    for _ in 0..4 {
        send(&mut editor, "b");
    }
    for line in editor.render(54) {
        assert!(visible_width(&line) <= 54, "{line:?}");
    }
}

#[test]
fn expands_large_pasted_content_literally_in_get_expanded_text() {
    let (mut editor, _) = editor();
    let pasted = (1..=10)
        .map(|index| format!("line {index}"))
        .chain(["tokens $1 $2 $& $$ $` $' end".to_string()])
        .collect::<Vec<_>>()
        .join("\n");
    send(&mut editor, format!("\x1b[200~{pasted}\x1b[201~"));
    assert!(editor.get_text().contains("[paste #"));
    assert_eq!(editor.get_expanded_text(), pasted);
}

#[test]
fn snaps_to_the_paste_marker_start_when_navigating_down_into_it() {
    let (mut editor, _) = editor();
    editor.set_text("12345678901234567890\n\nhello ");
    send(&mut editor, format!("\x1b[200~{}\x1b[201~", "x".repeat(2000)));
    editor.render(80);
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
    assert_eq!(cursor(&editor), (2, 6));
}

#[test]
fn preserves_sticky_column_when_navigating_through_paste_marker_line() {
    let (mut editor, _) = editor_with_rows(24);
    for ch in "1234567890123456".chars() {
        send(&mut editor, ch.to_string());
    }
    send(&mut editor, "\n");
    send(&mut editor, "\n");
    send(&mut editor, format!("\x1b[200~{}\x1b[201~", "x".repeat(2000)));
    send(&mut editor, "\n");
    send(&mut editor, "\n");
    for ch in "abcdefghijklmnop".chars() {
        send(&mut editor, ch.to_string());
    }
    editor.render(30);
    for _ in 0..4 {
        send(&mut editor, "\x1b[A");
    }
    send(&mut editor, "\x01");
    for _ in 0..10 {
        send(&mut editor, "\x1b[C");
    }
    assert_eq!(cursor(&editor), (0, 10));
    send(&mut editor, "\x1b[B");
    assert_eq!(cursor(&editor), (1, 0));
    send(&mut editor, "\x1b[B");
    assert_eq!(cursor(&editor), (2, 0));
    send(&mut editor, "\x1b[B");
    assert_eq!(cursor(&editor), (3, 0));
    send(&mut editor, "\x1b[B");
    assert_eq!(cursor(&editor), (4, 10));
}

#[test]
fn does_not_get_stuck_moving_down_from_a_multi_visual_line_paste_marker() {
    let (mut editor, _) = editor_with_rows(24);
    for ch in "abcdefgh".chars() {
        send(&mut editor, ch.to_string());
    }
    let big_content = "line\n".repeat(100).trim_end().to_string();
    send(&mut editor, format!("\x1b[200~{big_content}\x1b[201~"));
    for ch in "ijklmnopqr".chars() {
        send(&mut editor, ch.to_string());
    }
    send(&mut editor, "\n");
    for ch in "123456789012345678".chars() {
        send(&mut editor, ch.to_string());
    }
    editor.render(20);
    let text = editor.get_text();
    let marker = text.slice(8, text.find(" lines]").unwrap() + " lines]".len());
    let marker_start = 8;
    let marker_end = marker_start + marker.len();
    assert!(marker.len() > 20);

    send(&mut editor, "\x1b[A");
    send(&mut editor, "\x01");
    for _ in 0..6 {
        send(&mut editor, "\x1b[C");
    }
    assert_eq!(cursor(&editor), (0, 6));
    send(&mut editor, "\x1b[B");
    assert_eq!(cursor(&editor), (0, marker_start));
    send(&mut editor, "\x1b[B");
    assert_eq!(cursor(&editor), (0, marker_end));
    send(&mut editor, "\x1b[A");
    assert_eq!(cursor(&editor), (0, marker_start));
    send(&mut editor, "\x1b[A");
    assert_eq!(cursor(&editor), (0, 6));
}

#[test]
fn skips_marker_continuation_visual_lines_when_preferred_col_falls_in_marker_tail() {
    let (mut editor, _) = editor_with_rows(24);
    for ch in "abcdefgh".chars() {
        send(&mut editor, ch.to_string());
    }
    let big_content = "line\n".repeat(100).trim_end().to_string();
    send(&mut editor, format!("\x1b[200~{big_content}\x1b[201~"));
    for ch in "ijklmnopqr".chars() {
        send(&mut editor, ch.to_string());
    }
    send(&mut editor, "\n");
    for ch in "123456789012345678".chars() {
        send(&mut editor, ch.to_string());
    }
    editor.render(20);
    send(&mut editor, "\x1b[A");
    send(&mut editor, "\x01");
    for _ in 0..3 {
        send(&mut editor, "\x1b[C");
    }
    assert_eq!(cursor(&editor), (0, 3));
    send(&mut editor, "\x1b[B");
    assert_eq!(cursor(&editor), (0, 8));
    send(&mut editor, "\x1b[B");
    assert_eq!(cursor(&editor), (1, 3));
    send(&mut editor, "\x1b[A");
    assert_eq!(cursor(&editor), (0, 8));
    send(&mut editor, "\x1b[A");
    assert_eq!(cursor(&editor), (0, 3));
}

#[test]
fn submits_large_pasted_content_literally() {
    let (mut editor, submitted) = submitted_editor();
    let pasted = (1..=10)
        .map(|index| format!("line {index}"))
        .chain(["tokens $1 $2 $& $$ $` $' end".to_string()])
        .collect::<Vec<_>>()
        .join("\n");
    send(&mut editor, format!("\x1b[200~{pasted}\x1b[201~"));
    send(&mut editor, "\r");
    assert_eq!(*submitted.borrow(), pasted);
}
