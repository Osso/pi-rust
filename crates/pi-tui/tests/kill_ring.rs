use pi_js_compat::js_json::JsString;
use pi_tui::kill_ring::KillRing;

#[test]
fn empty_text_is_ignored() {
    let mut ring = KillRing::new();
    ring.push(&JsString::from(""), false, false);
    assert_eq!(ring.len(), 0);
    assert_eq!(ring.peek(), None);
}

#[test]
fn push_adds_new_entries_and_peek_returns_most_recent() {
    let mut ring = KillRing::new();
    ring.push(&JsString::from("first"), false, false);
    ring.push(&JsString::from("second"), false, false);
    assert_eq!(ring.len(), 2);
    assert_eq!(ring.peek(), Some(&JsString::from("second")));
}

#[test]
fn accumulate_appends_forward_deletions() {
    let mut ring = KillRing::new();
    ring.push(&JsString::from("abc"), false, false);
    ring.push(&JsString::from("def"), false, true);
    assert_eq!(ring.len(), 1);
    assert_eq!(ring.peek(), Some(&JsString::from("abcdef")));
}

#[test]
fn accumulate_prepends_backward_deletions() {
    let mut ring = KillRing::new();
    ring.push(&JsString::from("def"), false, false);
    ring.push(&JsString::from("abc"), true, true);
    assert_eq!(ring.len(), 1);
    assert_eq!(ring.peek(), Some(&JsString::from("abcdef")));
}

#[test]
fn accumulate_with_empty_ring_creates_entry() {
    let mut ring = KillRing::new();
    ring.push(&JsString::from("only"), true, true);
    assert_eq!(ring.peek(), Some(&JsString::from("only")));
}

#[test]
fn rotate_moves_last_entry_to_front() {
    let mut ring = KillRing::new();
    ring.push(&JsString::from("one"), false, false);
    ring.push(&JsString::from("two"), false, false);
    ring.push(&JsString::from("three"), false, false);

    ring.rotate();
    assert_eq!(ring.peek(), Some(&JsString::from("two")));
    ring.rotate();
    assert_eq!(ring.peek(), Some(&JsString::from("one")));
    ring.rotate();
    assert_eq!(ring.peek(), Some(&JsString::from("three")));
}

#[test]
fn rotate_is_a_no_op_for_zero_or_one_entry() {
    let mut ring = KillRing::new();
    ring.rotate();
    assert_eq!(ring.len(), 0);
    ring.push(&JsString::from("only"), false, false);
    ring.rotate();
    assert_eq!(ring.peek(), Some(&JsString::from("only")));
}
