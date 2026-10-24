use pi_js_compat::node_readline::LineBuffer;

#[test]
fn utf8_and_crlf_survive_separate_input_chunks() {
    let mut lines = LineBuffer::default();
    assert!(!lines.feed(&[0xe2], None));
    assert!(!lines.feed(&[0x82, 0xac, b'\r'], None));
    assert!(!lines.feed(b"\ntail", None));
    assert_eq!(lines.finish(), ["€", "tail"]);
}

#[test]
fn taking_completed_lines_retains_partial_data_and_crlf_state() {
    let mut lines = LineBuffer::default();
    assert!(!lines.feed(b"first\npart", None));
    assert_eq!(lines.take_completed_lines(), ["first"]);
    assert!(lines.take_completed_lines().is_empty());
    assert!(!lines.feed(b"ial\r", None));
    assert_eq!(lines.take_completed_lines(), ["partial"]);
    assert!(!lines.feed(b"\nlast", None));
    assert!(lines.take_completed_lines().is_empty());
    assert_eq!(lines.finish(), ["last"]);
}

#[test]
fn malformed_bytes_and_empty_cr_lines_keep_node_repair_semantics() {
    let mut lines = LineBuffer::default();
    assert!(!lines.feed(&[b'a', 255, b'\r', b'\r'], None));
    assert_eq!(lines.finish(), ["a�", ""]);
}
