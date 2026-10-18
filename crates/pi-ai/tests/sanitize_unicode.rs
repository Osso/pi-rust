use pi_ai::utils::sanitize_unicode::{sanitize_surrogate_units, sanitize_surrogates};

#[test]
fn preserves_paired_surrogates_and_plain_text() {
    let emoji = [0x48, 0x65, 0x6c, 0x6c, 0x6f, 0x20, 0xd83d, 0xde48, 0x20, 0x57];
    assert_eq!(sanitize_surrogate_units(&emoji), emoji);
    assert_eq!(sanitize_surrogates("Hello 🙈 World"), "Hello 🙈 World");
}

#[test]
fn removes_unpaired_high_and_low_surrogates() {
    assert_eq!(
        sanitize_surrogate_units(&[0x54, 0x65, 0x78, 0x74, 0x20, 0xd83d, 0x20, 0x68]),
        vec![0x54, 0x65, 0x78, 0x74, 0x20, 0x20, 0x68]
    );
    assert_eq!(sanitize_surrogate_units(&[0xd83d]), Vec::<u16>::new());
    assert_eq!(sanitize_surrogate_units(&[0xde48]), Vec::<u16>::new());
    assert_eq!(
        sanitize_surrogate_units(&[0x41, 0xde48, 0x42, 0xd83d, 0x43]),
        vec![0x41, 0x42, 0x43]
    );
}

#[test]
fn repairs_overlapping_surrogate_sequences() {
    assert_eq!(
        sanitize_surrogate_units(&[0xd83d, 0xd83d, 0xde48]),
        vec![0xd83d, 0xde48]
    );
    assert_eq!(
        sanitize_surrogate_units(&[0xd83d, 0xde48, 0xde48]),
        vec![0xd83d, 0xde48]
    );
    assert_eq!(sanitize_surrogate_units(&[0xde48, 0xd83d]), Vec::<u16>::new());
    assert_eq!(
        sanitize_surrogate_units(&[0xd83d, 0xde48, 0xd83d, 0xde48]),
        vec![0xd83d, 0xde48, 0xd83d, 0xde48]
    );
    assert_eq!(
        sanitize_surrogate_units(&[0xdc00, 0xd800, 0xdc00]),
        vec![0xd800, 0xdc00]
    );
}

#[test]
fn string_api_is_identity_for_valid_rust_text() {
    let text = "Text with emoji 🙈 and combining é";
    assert_eq!(sanitize_surrogates(text), text);
}
