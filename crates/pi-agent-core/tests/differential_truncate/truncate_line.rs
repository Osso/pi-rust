use pi_agent_core::harness::utils::{GREP_MAX_LINE_LENGTH, truncate_line};
use serde_json::{Value, json};

use super::{oracle::TsOracle, support::ok_response};

const FIRST_REQUEST_ID: u64 = 10_026;
const DISCREPANCY_REQUEST_ID: u64 = 10_031;
const MARKER: &str = "... [truncated]";
const HIGH_SURROGATE: u16 = 0xd83d;

pub(crate) fn compare_all_fixtures(oracle: &mut TsOracle) {
    compare_shared_fixtures(oracle);
    compare_approved_unicode_discrepancy(oracle);
}

fn compare_shared_fixtures(oracle: &mut TsOracle) {
    let exact = "x".repeat(GREP_MAX_LINE_LENGTH);
    let over = "x".repeat(GREP_MAX_LINE_LENGTH + 1);
    let fixtures = [
        ("default exact", exact, None),
        ("default over", over, None),
        ("zero", "abc".to_owned(), Some(0)),
        ("custom ASCII", "abc".to_owned(), Some(2)),
        ("BMP combining", "é\u{301}x".to_owned(), Some(2)),
    ];

    for (offset, (name, line, max_chars)) in fixtures.into_iter().enumerate() {
        let id = FIRST_REQUEST_ID + u64::try_from(offset).expect("fixture offset must fit u64");
        let expected = oracle.request(&line_request(id, &line, max_chars));
        let result = truncate_line(&line, max_chars);
        let actual = line_response(id, &result.text, result.was_truncated);

        assert_eq!(actual, expected, "shared truncate-line mismatch: {name}");
    }
}

fn compare_approved_unicode_discrepancy(oracle: &mut TsOracle) {
    let expected = oracle.request(&line_request(DISCREPANCY_REQUEST_ID, "🙂x", Some(1)));
    let mut source_units = vec![HIGH_SURROGATE];
    source_units.extend(MARKER.encode_utf16());
    assert_eq!(
        expected,
        ok_response(
            DISCREPANCY_REQUEST_ID,
            json!({"textUtf16": source_units, "wasTruncated": true}),
        ),
        "pinned UTF-16 source discrepancy changed"
    );

    let rust = truncate_line("🙂x", Some(1));
    assert_eq!(rust.text, format!("🙂{MARKER}"));
    assert!(rust.was_truncated);
    assert_ne!(rust.text.encode_utf16().collect::<Vec<_>>(), source_units);
}

fn line_request(id: u64, line: &str, max_chars: Option<usize>) -> Value {
    let mut request = json!({"id": id, "operation": "truncate_line", "line": line});
    if let Some(max_chars) = max_chars {
        request["maxChars"] = json!(max_chars);
    }
    request
}

fn line_response(id: u64, text: &str, was_truncated: bool) -> Value {
    ok_response(
        id,
        json!({
            "textUtf16": text.encode_utf16().collect::<Vec<_>>(),
            "wasTruncated": was_truncated,
        }),
    )
}
