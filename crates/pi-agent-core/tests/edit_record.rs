use std::fmt::Debug;

use pi_agent_core::harness::tools::edit_diff::Edit;

fn assert_debug<T: Debug>(_: &T) {}

#[test]
fn edit_preserves_required_fields_and_debug() {
    let edit = Edit {
        old_text: "before".to_owned(),
        new_text: "after".to_owned(),
    };

    assert_eq!(edit.old_text, "before");
    assert_eq!(edit.new_text, "after");
    assert_debug(&edit);
}

#[test]
fn edit_preserves_empty_unicode_and_control_strings() {
    let edit = Edit {
        old_text: "\0前😀\r\n".to_owned(),
        new_text: "".to_owned(),
    };

    assert_eq!(edit.old_text, "\0前😀\r\n");
    assert_eq!(edit.new_text, "");
}

#[test]
fn edit_fields_are_publicly_mutable() {
    let mut edit = Edit {
        old_text: "before".to_owned(),
        new_text: "after".to_owned(),
    };

    edit.old_text = "updated old".to_owned();
    edit.new_text = "updated new".to_owned();

    assert_eq!(edit.old_text, "updated old");
    assert_eq!(edit.new_text, "updated new");
}
