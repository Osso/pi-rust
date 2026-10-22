use pi_js_compat::js_json::JsString;
use std::fmt::Debug;

use pi_coding_agent::core::tools::edit_primitives::AppliedEditsResult;

fn assert_debug<T: Debug>(_: &T) {}

#[test]
fn applied_edits_result_preserves_required_fields_and_debug() {
    let result = AppliedEditsResult {
        base_content: JsString::from("before"),
        new_content: JsString::from("after"),
    };

    assert_eq!(result.base_content, "before");
    assert_eq!(result.new_content, "after");
    assert_debug(&result);
}

#[test]
fn applied_edits_result_preserves_empty_unicode_and_control_strings() {
    let result = AppliedEditsResult {
        base_content: JsString::from("\0旧\n\r\n🙂"),
        new_content: JsString::from(""),
    };

    assert_eq!(result.base_content, "\0旧\n\r\n🙂");
    assert_eq!(result.new_content, "");
}

#[test]
fn applied_edits_result_fields_are_publicly_mutable() {
    let mut result = AppliedEditsResult {
        base_content: JsString::from("before"),
        new_content: JsString::from("after"),
    };

    result.base_content = JsString::from("updated base");
    result.new_content = JsString::from("updated new");

    assert_eq!(result.base_content, "updated base");
    assert_eq!(result.new_content, "updated new");
}
