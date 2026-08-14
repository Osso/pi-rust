use std::fmt::Debug;

use pi_agent_core::harness::tools::edit_diff::AppliedEditsResult;

fn assert_debug<T: Debug>(_: &T) {}

#[test]
fn applied_edits_result_preserves_required_fields_and_debug() {
    let result = AppliedEditsResult {
        base_content: "before".to_owned(),
        new_content: "after".to_owned(),
    };

    assert_eq!(result.base_content, "before");
    assert_eq!(result.new_content, "after");
    assert_debug(&result);
}

#[test]
fn applied_edits_result_preserves_empty_unicode_and_control_strings() {
    let result = AppliedEditsResult {
        base_content: "\0旧\n\r\n🙂".to_owned(),
        new_content: "".to_owned(),
    };

    assert_eq!(result.base_content, "\0旧\n\r\n🙂");
    assert_eq!(result.new_content, "");
}

#[test]
fn applied_edits_result_fields_are_publicly_mutable() {
    let mut result = AppliedEditsResult {
        base_content: "before".to_owned(),
        new_content: "after".to_owned(),
    };

    result.base_content = "updated base".to_owned();
    result.new_content = "updated new".to_owned();

    assert_eq!(result.base_content, "updated base");
    assert_eq!(result.new_content, "updated new");
}
