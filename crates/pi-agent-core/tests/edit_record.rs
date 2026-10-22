use pi_js_compat::js_json::JsString;
use std::fmt::Debug;

use pi_agent_core::harness::tools::edit_diff::Edit;

fn assert_debug<T: Debug>(_: &T) {}

#[test]
fn edit_preserves_required_fields_and_debug() {
    let edit = Edit {
        old_text: JsString::from("before"),
        new_text: JsString::from("after"),
    };

    assert_eq!(edit.old_text, "before");
    assert_eq!(edit.new_text, "after");
    assert_debug(&edit);
}

#[test]
fn edit_preserves_empty_unicode_and_control_strings() {
    let edit = Edit {
        old_text: JsString::from("\0前😀\r\n"),
        new_text: JsString::from(""),
    };

    assert_eq!(edit.old_text, "\0前😀\r\n");
    assert_eq!(edit.new_text, "");
}

#[test]
fn edit_fields_are_publicly_mutable() {
    let mut edit = Edit {
        old_text: JsString::from("before"),
        new_text: JsString::from("after"),
    };

    edit.old_text = JsString::from("updated old");
    edit.new_text = JsString::from("updated new");

    assert_eq!(edit.old_text, "updated old");
    assert_eq!(edit.new_text, "updated new");
}
