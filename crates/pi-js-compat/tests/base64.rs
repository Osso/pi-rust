//! Base64 parity for `atob`/`btoa` and Node `Buffer`.

use pi_js_compat::base64::{atob, btoa, buffer_decoded_len, buffer_from_base64, buffer_to_base64};

#[test]
fn atob_matches_web_api_leniency_and_errors() {
    assert_eq!(atob(&"".into()).unwrap(), Vec::<u8>::new());
    assert_eq!(atob(&"AA".into()).unwrap(), vec![0]);
    assert_eq!(atob(&"AAA".into()).unwrap(), vec![0, 0]);
    assert_eq!(atob(&"AAAA".into()).unwrap(), vec![0, 0, 0]);
    assert_eq!(atob(&"QQ==".into()).unwrap(), b"A");
    assert_eq!(atob(&"QUI=".into()).unwrap(), b"AB");
    assert_eq!(atob(&"QUJD".into()).unwrap(), b"ABC");
    assert_eq!(atob(&"A A A A".into()).unwrap(), vec![0, 0, 0]);
    assert_eq!(atob(&"QUJD\nREVG".into()).unwrap(), b"ABCDEF");
    assert_eq!(atob(&"QUJD\r\nREVG".into()).unwrap(), b"ABCDEF");
    for invalid in [
        "A", "AAAAA", "AA==AAAA", "AAA=AAAA", "!!!!", "A-_A", "-_+/AQ==", "=", "AA=", "AA=A", "A===", "A=", "=AAA",
    ] {
        assert!(atob(&invalid.into()).is_err(), "{invalid:?}");
    }
}

#[test]
fn btoa_matches_standard_padded_encoding() {
    assert_eq!(btoa(b""), "");
    assert_eq!(btoa(b"A"), "QQ==");
    assert_eq!(btoa(b"AB"), "QUI=");
    assert_eq!(btoa(b"ABC"), "QUJD");
    assert_eq!(btoa(b"AAAA"), "QUFBQQ==");
}

#[test]
fn buffer_decoding_accepts_both_alphabets_and_stops_at_padding() {
    assert_eq!(buffer_from_base64(""), Vec::<u8>::new());
    assert_eq!(buffer_from_base64("A"), Vec::<u8>::new());
    assert_eq!(buffer_from_base64("AA"), vec![0]);
    assert_eq!(buffer_from_base64("AAA"), vec![0, 0]);
    assert_eq!(buffer_from_base64("AAAA"), vec![0, 0, 0]);
    assert_eq!(buffer_from_base64("AAAAA"), vec![0, 0, 0]);
    assert_eq!(buffer_from_base64("AA==AAAA"), vec![0]);
    assert_eq!(buffer_from_base64("AAA=AAAA"), vec![0, 0]);
    assert_eq!(buffer_from_base64("A A A A"), vec![0, 0, 0]);
    assert_eq!(buffer_from_base64("!!!!"), Vec::<u8>::new());
    assert_eq!(buffer_from_base64("A-_A"), vec![0x03, 0xef, 0xc0]);
    assert_eq!(buffer_from_base64("-_+/AQ=="), vec![0xfb, 0xff, 0xbf, 0x01]);
    assert_eq!(buffer_from_base64("QUJDRA"), b"ABCD");
}

#[test]
fn buffer_encoding_matches_standard_padded_encoding() {
    assert_eq!(buffer_to_base64(b""), "");
    assert_eq!(buffer_to_base64(b"A"), "QQ==");
    assert_eq!(buffer_to_base64(b"AB"), "QUI=");
    assert_eq!(buffer_to_base64(b"ABC"), "QUJD");
    assert_eq!(buffer_to_base64(b"AAAA"), "QUFBQQ==");
}

#[test]
fn buffer_decoded_len_counts_standard_symbols_before_padding() {
    for (value, expected) in [
        ("AAAA", 3),
        ("AAA=", 2),
        ("AA==", 1),
        ("A", 0),
        ("AAAAA", 3),
        ("AA==AAAA", 1),
        ("", 0),
        ("A A A A", 3),
        ("!!!!", 0),
    ] {
        assert_eq!(buffer_decoded_len(value), expected, "{value:?}");
    }
}
