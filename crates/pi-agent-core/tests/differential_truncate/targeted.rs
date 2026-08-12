use serde_json::json;

use super::{
    oracle::TsOracle,
    support::{Direction, TruncationCase, ok_response, options},
};
use pi_agent_core::harness::utils::truncate_head;

const FIRST_REQUEST_ID: u64 = 10_008;
const FALLBACK_REQUEST_ID: u64 = 10_032;

pub(crate) fn compare_all_fixtures(oracle: &mut TsOracle) {
    for (offset, fixture) in fixtures().into_iter().enumerate() {
        let id = FIRST_REQUEST_ID + u64::try_from(offset).expect("fixture offset must fit u64");
        let expected = oracle.request(&fixture.request(id));
        let actual = fixture.rust_response(id);

        assert_eq!(actual, expected, "targeted truncation mismatch: {}", fixture.name);
    }
}

pub(crate) fn compare_utf8_fallback(oracle: &mut TsOracle) {
    let content = "aé🙂\nb";
    let selected_options = options(10, 100);
    let expected = oracle.request(&json!({
        "id": FALLBACK_REQUEST_ID,
        "operation": "truncate_head_without_buffer",
        "content": content,
        "options": {"maxLines": 10, "maxBytes": 100},
    }));
    let actual = ok_response(
        FALLBACK_REQUEST_ID,
        super::support::normalize_result(&truncate_head(content, selected_options)),
    );

    assert_eq!(actual, expected, "fallback UTF-8 byte counting mismatch");
}

fn fixtures() -> Vec<TruncationCase> {
    let mut fixtures = head_fixtures();
    fixtures.extend(tail_fixtures());
    fixtures
}

fn head_fixtures() -> Vec<TruncationCase> {
    let mut fixtures = head_content_fixtures();
    fixtures.extend(head_limit_fixtures());
    fixtures
}

fn head_content_fixtures() -> Vec<TruncationCase> {
    vec![
        TruncationCase::new("head defaults", Direction::Head, "alpha\nβ\n", None),
        TruncationCase::new("head empty zero", Direction::Head, "", Some(options(0, 0))),
        TruncationCase::new(
            "head CRLF Unicode trailing LF",
            Direction::Head,
            "é\r\n🙂\n",
            Some(options(2, 9)),
        ),
        TruncationCase::new(
            "head complete byte lines",
            Direction::Head,
            "aa\nbbb\nc",
            Some(options(10, 6)),
        ),
        TruncationCase::new(
            "head first line oversized",
            Direction::Head,
            "éé\nabc",
            Some(options(10, 3)),
        ),
    ]
}

fn head_limit_fixtures() -> Vec<TruncationCase> {
    vec![
        TruncationCase::new("head line limit", Direction::Head, "a\nb\nc", Some(options(2, 100))),
        TruncationCase::new("head line-byte tie", Direction::Head, "a\nb\nc", Some(options(2, 3))),
        TruncationCase::new("head zero lines", Direction::Head, "a\nb", Some(options(0, 100))),
        TruncationCase::new(
            "head empty lines NUL supplementary",
            Direction::Head,
            "a\n\n\0🙂\nz",
            Some(options(10, 8)),
        ),
    ]
}

fn tail_fixtures() -> Vec<TruncationCase> {
    let mut fixtures = tail_limit_fixtures();
    fixtures.extend(tail_text_fixtures());
    fixtures
}

fn tail_limit_fixtures() -> Vec<TruncationCase> {
    vec![
        TruncationCase::new("tail defaults", Direction::Tail, "alpha\nβ\n", None),
        TruncationCase::new("tail line limit", Direction::Tail, "a\nb\nc", Some(options(2, 100))),
        TruncationCase::new(
            "tail complete byte lines",
            Direction::Tail,
            "a\nbb\nccc",
            Some(options(10, 6)),
        ),
        TruncationCase::new(
            "tail partial Unicode suffix",
            Direction::Tail,
            "aé🙂b",
            Some(options(10, 5)),
        ),
        TruncationCase::new("tail oversized scalar", Direction::Tail, "abc🙂", Some(options(10, 3))),
    ]
}

fn tail_text_fixtures() -> Vec<TruncationCase> {
    vec![
        TruncationCase::new("tail zero-byte tie", Direction::Tail, "abc🙂", Some(options(1, 0))),
        TruncationCase::new(
            "tail combining NUL",
            Direction::Tail,
            "\u{301}\0🙂",
            Some(options(10, 4)),
        ),
        TruncationCase::new(
            "tail trailing LF truncation",
            Direction::Tail,
            "a\nbb\nccc\n",
            Some(options(2, 100)),
        ),
        TruncationCase::new(
            "tail CRLF preservation",
            Direction::Tail,
            "a\r\nbb\r\nc",
            Some(options(2, 100)),
        ),
    ]
}
