use std::collections::BTreeMap;

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
    let statuses = BTreeMap::from([
        ("b".to_owned(), "  B  ".to_owned()),
        ("goal".to_owned(), "Ship\tit".to_owned()),
        ("a".to_owned(), "A".to_owned()),
    ]);

    assert_eq!(
        format_extension_status_text_lines(&statuses),
        vec!["A B".to_owned(), "Ship it".to_owned()]
    );
}

#[test]
fn emits_only_the_goal_when_it_is_the_only_status() {
    let statuses = BTreeMap::from([("goal".to_owned(), "G".to_owned())]);
    assert_eq!(format_extension_status_text_lines(&statuses), vec!["G".to_owned()]);
}

#[test]
fn emits_only_other_statuses_when_goal_is_absent() {
    let statuses = BTreeMap::from([("z".to_owned(), "Z".to_owned()), ("a".to_owned(), "A".to_owned())]);
    assert_eq!(format_extension_status_text_lines(&statuses), vec!["A Z".to_owned()]);
}

#[test]
fn emits_nothing_for_an_empty_map() {
    assert!(format_extension_status_text_lines(&BTreeMap::new()).is_empty());
}

#[test]
fn keeps_an_empty_goal_status_line() {
    let statuses = BTreeMap::from([("goal".to_owned(), "  ".to_owned())]);
    assert_eq!(format_extension_status_text_lines(&statuses), vec![String::new()]);
}

#[test]
fn keeps_an_empty_other_status_line() {
    let statuses = BTreeMap::from([("a".to_owned(), "  ".to_owned())]);
    assert_eq!(format_extension_status_text_lines(&statuses), vec![String::new()]);
}
