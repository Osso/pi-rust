//! Base64 parity for `atob`/`btoa` and Node `Buffer`.

use pi_js_compat::base64::{Base64Error, atob, btoa, buffer_decoded_len, buffer_from_base64, buffer_to_base64};

#[test]
fn atob_matches_web_api_leniency_and_errors() {
    assert_eq!(atob("").unwrap(), Vec::<u8>::new());
    assert_eq!(atob("AA").unwrap(), vec![0]);
    assert_eq!(atob("AAA").unwrap(), vec![0, 0]);
    assert_eq!(atob("AAAA").unwrap(), vec![0, 0, 0]);
    assert_eq!(atob("QQ==").unwrap(), b"A");
    assert_eq!(atob("QUI=").unwrap(), b"AB");
    assert_eq!(atob("QUJD").unwrap(), b"ABC");
    assert_eq!(atob("A A A A").unwrap(), vec![0, 0, 0]);
    assert_eq!(atob("QUJD\nREVG").unwrap(), b"ABCDEF");
    assert_eq!(atob("QUJD\r\nREVG").unwrap(), b"ABCDEF");
    for invalid in [
        "A", "AAAAA", "AA==AAAA", "AAA=AAAA", "!!!!", "A-_A", "-_+/AQ==", "=", "AA=", "AA=A", "A===", "A=", "=AAA",
    ] {
        assert_eq!(atob(invalid), Err(Base64Error), "{invalid:?}");
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
