use pi_ai::utils::hash::short_hash;

#[test]
fn hashes_concrete_strings_like_the_typescript_source() {
    let long = "0123456789".repeat(20);
    let cases = [
        ("", "k4n83c7h0j2b"),
        ("a", "m8735310ae7sx"),
        ("abc", "y0biex7f9bbh"),
        ("hello world", "n7rb4n1m39uz8"),
        ("🙈", "kphsz0153ms3q"),
        ("é", "1ohi2dgmq6lzp"),
        ("a\0b", "r2cxz11lm0c19"),
        ("The quick brown fox jumps over the lazy dog", "eig47k1th3xf1"),
        (long.as_str(), "j54qqh1qj5qh"),
    ];

    for (input, expected) in cases {
        assert_eq!(short_hash(input), expected, "hash mismatch for {input:?}");
    }
}

#[test]
fn hashes_utf16_code_units_not_utf8_bytes() {
    assert_ne!(short_hash("é"), short_hash("e"));
    assert_eq!(short_hash("\u{10000}"), short_hash("\u{10000}"));
    assert_ne!(short_hash("\u{10000}"), short_hash("\u{10001}"));
}
