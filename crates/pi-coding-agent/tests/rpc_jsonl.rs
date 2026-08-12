use pi_coding_agent::modes::rpc::{JsonlLineDecoder, serialize_json_line};
use serde::Serialize;
use serde_json::json;

struct FailingValue;

impl Serialize for FailingValue {
    fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        Err(serde::ser::Error::custom("intentional serialization failure"))
    }
}

fn collect_string_chunks(chunks: &[&str]) -> Vec<String> {
    let mut decoder = JsonlLineDecoder::new();
    let mut lines = Vec::new();
    for chunk in chunks {
        lines.extend(decoder.push_str(chunk));
    }
    lines.extend(decoder.end());
    lines
}

#[test]
fn serialize_json_line_is_compact_and_adds_one_lf() {
    let null = serialize_json_line(&serde_json::Value::Null).expect("null serialization must succeed");
    let boolean = serialize_json_line(&true).expect("boolean serialization must succeed");
    let number = serialize_json_line(&9).expect("number serialization must succeed");
    let object = serialize_json_line(&json!({"text": "line\nbreak\u{2028}\u{2029}"}))
        .expect("object serialization must succeed");

    assert_eq!(null, "null\n");
    assert_eq!(boolean, "true\n");
    assert_eq!(number, "9\n");
    assert_eq!(object, "{\"text\":\"line\\nbreak\u{2028}\u{2029}\"}\n");
    assert_eq!(object.matches('\n').count(), 1);
}

#[test]
fn decoder_splits_only_on_lf_and_strips_one_cr_from_crlf() {
    let lines = collect_string_chunks(&["first\nsecond\r\n\r\r\n\nthird\rinside\n"]);

    assert_eq!(lines, ["first", "second", "\r", "", "third\rinside"]);
}

#[test]
fn decoder_handles_arbitrary_ascii_chunking() {
    let input = (0..32).map(|index| format!("line-{index}\n")).collect::<String>();
    let chunks = input.chars().map(|character| character.to_string()).collect::<Vec<_>>();
    let chunk_refs = chunks.iter().map(String::as_str).collect::<Vec<_>>();

    let lines = collect_string_chunks(&chunk_refs);
    let expected = (0..32).map(|index| format!("line-{index}")).collect::<Vec<_>>();

    assert_eq!(lines, expected);
}

#[test]
fn decoder_completes_split_two_and_four_byte_utf8_scalars() {
    let mut decoder = JsonlLineDecoder::new();

    assert_eq!(decoder.push_bytes(&[0xc3]), Vec::<String>::new());
    assert_eq!(decoder.push_bytes(&[0xa9, b'\n']), ["é"]);
    assert_eq!(decoder.push_bytes(&[0xf0]), Vec::<String>::new());
    assert_eq!(decoder.push_bytes(&[0x90]), Vec::<String>::new());
    assert_eq!(decoder.push_bytes(&[0x80]), Vec::<String>::new());
    assert_eq!(decoder.push_bytes(&[0x80, b'\n']), ["𐀀"]);
}

#[test]
fn decoder_replaces_malformed_utf8_and_incomplete_end_sequences() {
    let cases = [
        (vec![0xff, b'\n'], "�"),
        (vec![0xe2, 0x28, 0xa1, b'\n'], "�(�"),
        (vec![0xc0, 0xaf, b'\n'], "��"),
        (vec![0xed, 0xa0, 0x80, b'\n'], "���"),
    ];

    for (bytes, expected) in cases {
        let mut decoder = JsonlLineDecoder::new();
        assert_eq!(decoder.push_bytes(&bytes), [expected]);
        assert!(decoder.end().is_empty());
    }

    let mut decoder = JsonlLineDecoder::new();
    assert!(decoder.push_bytes(&[0xe2, 0x82]).is_empty());
    assert_eq!(decoder.end(), ["�"]);
}

#[test]
fn decoder_preserves_bom_empty_chunks_and_unterminated_lines() {
    let mut decoder = JsonlLineDecoder::new();

    assert!(decoder.push_bytes(&[]).is_empty());
    assert_eq!(decoder.push_bytes(&[0xef, 0xbb, 0xbf, b'x', b'\n']), ["\u{feff}x"]);
    assert!(decoder.push_str("").is_empty());
    assert_eq!(decoder.push_str("unterminated"), Vec::<String>::new());
    assert_eq!(decoder.end(), ["unterminated"]);

    let empty = JsonlLineDecoder::new();
    assert!(empty.end().is_empty());

    let mut trailing_cr = JsonlLineDecoder::new();
    assert!(trailing_cr.push_str("tail\r").is_empty());
    assert_eq!(trailing_cr.end(), ["tail"]);
}

#[test]
fn decoder_preserves_consecutive_empty_records_and_terminal_lf_has_no_extra_record() {
    let mut decoder = JsonlLineDecoder::new();

    assert_eq!(decoder.push_str("\n\nvalue\n"), ["", "", "value"]);
    assert!(decoder.end().is_empty());
}

#[test]
fn string_chunks_do_not_consume_pending_byte_decoder_sequences() {
    let mut decoder = JsonlLineDecoder::new();

    assert!(decoder.push_bytes(&[0xf0]).is_empty());
    assert_eq!(decoder.push_str("string line\n"), ["string line"]);
    assert_eq!(decoder.push_bytes(&[0x9f, 0x92, 0xa9, b'\n']), ["💩"]);
}

#[test]
fn serializer_propagates_json_errors() {
    let error = serialize_json_line(&FailingValue).expect_err("serialization failure must propagate");

    assert!(error.to_string().contains("intentional serialization failure"));
}

#[test]
fn serializer_matches_javascript_for_non_finite_numbers() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            serialize_json_line(&value).expect("non-finite float must serialize"),
            "null\n"
        );
    }
}

#[test]
fn serializer_uses_the_approved_serde_number_spelling() {
    assert_eq!(
        serialize_json_line(&1.0f64).expect("finite float must serialize"),
        "1.0\n"
    );
    assert_eq!(
        serialize_json_line(&-0.0f64).expect("negative zero must serialize"),
        "-0.0\n"
    );
    assert_eq!(
        serialize_json_line(&1e20f64).expect("large float must serialize"),
        "1e+20\n"
    );
}

#[test]
fn json_value_serialization_remains_compact() {
    let value = json!({"number": 9, "text": "value"});

    assert_eq!(
        serialize_json_line(&value).expect("JSON value must serialize"),
        "{\"number\":9,\"text\":\"value\"}\n"
    );
}
