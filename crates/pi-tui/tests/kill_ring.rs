use pi_tui::kill_ring::KillRing;

#[test]
fn empty_text_is_ignored() {
    let mut ring = KillRing::new();
    ring.push("", false, false);
    assert_eq!(ring.len(), 0);
    assert_eq!(ring.peek(), None);
}

#[test]
fn push_adds_new_entries_and_peek_returns_most_recent() {
    let mut ring = KillRing::new();
    ring.push("first", false, false);
    ring.push("second", false, false);
    assert_eq!(ring.len(), 2);
    assert_eq!(ring.peek(), Some("second"));
}

#[test]
fn accumulate_appends_forward_deletions() {
    let mut ring = KillRing::new();
    ring.push("abc", false, false);
    ring.push("def", false, true);
    assert_eq!(ring.len(), 1);
    assert_eq!(ring.peek(), Some("abcdef"));
}

#[test]
fn accumulate_prepends_backward_deletions() {
    let mut ring = KillRing::new();
    ring.push("def", false, false);
    ring.push("abc", true, true);
    assert_eq!(ring.len(), 1);
    assert_eq!(ring.peek(), Some("abcdef"));
}

#[test]
fn accumulate_with_empty_ring_creates_entry() {
    let mut ring = KillRing::new();
    ring.push("only", true, true);
    assert_eq!(ring.peek(), Some("only"));
}

#[test]
fn rotate_moves_last_entry_to_front() {
    let mut ring = KillRing::new();
    ring.push("one", false, false);
    ring.push("two", false, false);
    ring.push("three", false, false);

    ring.rotate();
    assert_eq!(ring.peek(), Some("two"));
    ring.rotate();
    assert_eq!(ring.peek(), Some("one"));
    ring.rotate();
    assert_eq!(ring.peek(), Some("three"));
}

#[test]
fn rotate_is_a_no_op_for_zero_or_one_entry() {
    let mut ring = KillRing::new();
    ring.rotate();
    assert_eq!(ring.len(), 0);
    ring.push("only", false, false);
    ring.rotate();
    assert_eq!(ring.peek(), Some("only"));
}
