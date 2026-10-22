use indexmap::IndexMap;
use pi_js_compat::js_json::JsString;

use pi_coding_agent::utils::footer_status::{format_extension_status_text_lines, sanitize_footer_status_text};

#[test]
fn sanitizes_control_whitespace_and_collapses_spaces() {
    assert_eq!(sanitize_footer_status_text("a\r\n\tb"), "a b");
    assert_eq!(sanitize_footer_status_text("  hello   world  "), "hello world");
    assert_eq!(sanitize_footer_status_text("\r\n"), "");
    assert_eq!(sanitize_footer_status_text("a\u{a0}\u{a0}b"), "a\u{a0}\u{a0}b");
    assert_eq!(sanitize_footer_status_text("a\t\tb\nc"), "a b c");
}

#[test]
fn joins_non_goal_statuses_in_key_order_before_the_goal() {
    let statuses = IndexMap::from([
        ("b".into(), "  B  ".into()),
        ("goal".into(), "Ship\tit".into()),
        ("a".into(), "A".into()),
    ]);

    assert_eq!(
        format_extension_status_text_lines(&statuses),
        vec![JsString::from("A B"), JsString::from("Ship it")]
    );
}

#[test]
fn emits_only_the_goal_when_it_is_the_only_status() {
    let statuses = IndexMap::from([("goal".into(), "G".into())]);
    assert_eq!(format_extension_status_text_lines(&statuses), vec![JsString::from("G")]);
}

#[test]
fn emits_only_other_statuses_when_goal_is_absent() {
    let statuses = IndexMap::from([("z".into(), "Z".into()), ("a".into(), "A".into())]);
    assert_eq!(
        format_extension_status_text_lines(&statuses),
        vec![JsString::from("A Z")]
    );
}

#[test]
fn emits_nothing_for_an_empty_map() {
    assert!(format_extension_status_text_lines(&IndexMap::new()).is_empty());
}

#[test]
fn keeps_an_empty_goal_status_line() {
    let statuses = IndexMap::from([("goal".into(), "  ".into())]);
    assert_eq!(format_extension_status_text_lines(&statuses), vec![JsString::default()]);
}

#[test]
fn keeps_an_empty_other_status_line() {
    let statuses = IndexMap::from([("a".into(), "  ".into())]);
    assert_eq!(format_extension_status_text_lines(&statuses), vec![JsString::default()]);
}
