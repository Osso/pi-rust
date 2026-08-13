use pi_agent_core::harness::compaction::ContextUsageEstimate;
use serde_json::{Value, json};

use super::{oracle::TsOracle, support::ok_response};

const FIRST_REQUEST_ID: u64 = 13_148;
const EXPECTED_FIXTURE_COUNT: usize = 8;

struct Fixture {
    request_id: u64,
    name: &'static str,
    cases: Vec<Case>,
}

struct Case {
    name: &'static str,
    tokens_bits: &'static str,
    usage_tokens_bits: &'static str,
    trailing_tokens_bits: &'static str,
    last_usage_index_bits: Option<&'static str>,
}

pub(crate) fn compare_all_fixtures(oracle: &mut TsOracle) {
    let fixtures = fixtures();
    assert_eq!(fixtures.len(), EXPECTED_FIXTURE_COUNT);
    assert_request_ids(&fixtures);

    for fixture in &fixtures {
        let expected = oracle.request(&request(fixture));
        let actual = rust_response(fixture);
        assert_eq!(actual, expected, "context usage estimate mismatch: {}", fixture.name);
    }
}

fn assert_request_ids(fixtures: &[Fixture]) {
    for (offset, fixture) in fixtures.iter().enumerate() {
        let expected = FIRST_REQUEST_ID + u64::try_from(offset).expect("fixture offset must fit u64");
        assert_eq!(fixture.request_id, expected, "fixture IDs must remain contiguous");
    }
}

fn request(fixture: &Fixture) -> Value {
    json!({
        "id": fixture.request_id,
        "operation": "context_usage_estimate",
        "cases": fixture.cases.iter().map(case_request).collect::<Vec<_>>(),
    })
}

fn case_request(case: &Case) -> Value {
    json!({
        "name": case.name,
        "tokensBits": case.tokens_bits,
        "usageTokensBits": case.usage_tokens_bits,
        "trailingTokensBits": case.trailing_tokens_bits,
        "lastUsageIndexBits": case.last_usage_index_bits,
    })
}

fn rust_response(fixture: &Fixture) -> Value {
    let results = fixture.cases.iter().map(normalize_case).collect::<Vec<_>>();
    ok_response(fixture.request_id, json!({"results": results}))
}

fn normalize_case(case: &Case) -> Value {
    let estimate = ContextUsageEstimate {
        tokens: binary64_from_bits(case.tokens_bits),
        usage_tokens: binary64_from_bits(case.usage_tokens_bits),
        trailing_tokens: binary64_from_bits(case.trailing_tokens_bits),
        last_usage_index: case.last_usage_index_bits.map(binary64_from_bits),
    };
    json!({
        "name": case.name,
        "tokensBits": binary64_bits(estimate.tokens),
        "usageTokensBits": binary64_bits(estimate.usage_tokens),
        "trailingTokensBits": binary64_bits(estimate.trailing_tokens),
        "lastUsageIndexBits": estimate.last_usage_index.map(binary64_bits),
    })
}

fn binary64_from_bits(bits: &str) -> f64 {
    let value = u64::from_str_radix(bits, 16).expect("fixture bits must be lowercase hexadecimal");
    f64::from_bits(value)
}

fn binary64_bits(value: f64) -> String {
    format!("{:016x}", value.to_bits())
}

fn fixtures() -> Vec<Fixture> {
    vec![
        ordinary_fixture(),
        negative_fractional_index_fixture(),
        signed_zero_fixture(),
        subnormal_fixture(),
        max_finite_fixture(),
        infinity_fixture(),
        nan_fixture(),
        final_record_fixture(),
    ]
}

fn ordinary_fixture() -> Fixture {
    fixture(
        13_148,
        "ordinary values and optional index states",
        vec![
            case(
                "ordinary without usage index",
                "4045400000000000",
                "4031400000000000",
                "400e000000000000",
                None,
            ),
            case(
                "ordinary with usage index",
                "3ff0000000000000",
                "4004000000000000",
                "400c000000000000",
                Some("4012000000000000"),
            ),
        ],
    )
}

fn negative_fractional_index_fixture() -> Fixture {
    fixture(
        13_149,
        "negative and fractional usage indexes",
        vec![
            case(
                "negative fractional index",
                "bfe0000000000000",
                "c002000000000000",
                "3fc0000000000000",
                Some("c004000000000000"),
            ),
            case(
                "positive fractional index",
                "3fd0000000000000",
                "3ff4000000000000",
                "bffc000000000000",
                Some("3fd0000000000000"),
            ),
        ],
    )
}

fn signed_zero_fixture() -> Fixture {
    fixture(
        13_150,
        "signed zeros in every numeric position",
        vec![
            case(
                "positive zeros",
                "0000000000000000",
                "0000000000000000",
                "0000000000000000",
                Some("0000000000000000"),
            ),
            case(
                "negative zeros",
                "8000000000000000",
                "8000000000000000",
                "8000000000000000",
                Some("8000000000000000"),
            ),
        ],
    )
}

fn subnormal_fixture() -> Fixture {
    fixture(
        13_151,
        "positive and negative subnormals in every numeric position",
        vec![
            case(
                "positive subnormals",
                "0000000000000001",
                "0000000000000002",
                "0000000000000003",
                Some("0000000000000004"),
            ),
            case(
                "negative subnormals",
                "8000000000000001",
                "8000000000000002",
                "8000000000000003",
                Some("8000000000000004"),
            ),
        ],
    )
}

fn max_finite_fixture() -> Fixture {
    fixture(
        13_152,
        "positive and negative maximum finite values",
        vec![
            case(
                "positive maximum finite",
                "7fefffffffffffff",
                "7fefffffffffffff",
                "7fefffffffffffff",
                Some("7fefffffffffffff"),
            ),
            case(
                "negative maximum finite",
                "ffefffffffffffff",
                "ffefffffffffffff",
                "ffefffffffffffff",
                Some("ffefffffffffffff"),
            ),
        ],
    )
}

fn infinity_fixture() -> Fixture {
    fixture(
        13_153,
        "positive and negative infinities in every numeric position",
        vec![
            case(
                "positive infinities",
                "7ff0000000000000",
                "7ff0000000000000",
                "7ff0000000000000",
                Some("7ff0000000000000"),
            ),
            case(
                "negative infinities",
                "fff0000000000000",
                "fff0000000000000",
                "fff0000000000000",
                Some("fff0000000000000"),
            ),
        ],
    )
}

fn nan_fixture() -> Fixture {
    fixture(
        13_154,
        "distinct positive and negative NaN payloads in every numeric position",
        vec![
            case(
                "distinct positive NaN payloads",
                "7ff8000000000001",
                "7ff8000000000042",
                "7ff8000000000100",
                Some("7ff8000000000400"),
            ),
            case(
                "distinct negative NaN payloads",
                "fff8000000000001",
                "fff8000000000042",
                "fff8000000000100",
                Some("fff8000000000400"),
            ),
        ],
    )
}

fn final_record_fixture() -> Fixture {
    fixture(
        13_155,
        "final source-shaped records after mutation",
        vec![
            case(
                "final record without usage index",
                "3fe8000000000000",
                "7fefffffffffffff",
                "8000000000000000",
                None,
            ),
            case(
                "final record with usage index",
                "fff0000000000000",
                "0000000000000001",
                "7ff8000000000042",
                Some("c004000000000000"),
            ),
        ],
    )
}

fn case(
    name: &'static str,
    tokens_bits: &'static str,
    usage_tokens_bits: &'static str,
    trailing_tokens_bits: &'static str,
    last_usage_index_bits: Option<&'static str>,
) -> Case {
    Case {
        name,
        tokens_bits,
        usage_tokens_bits,
        trailing_tokens_bits,
        last_usage_index_bits,
    }
}

fn fixture(request_id: u64, name: &'static str, cases: Vec<Case>) -> Fixture {
    Fixture {
        request_id,
        name,
        cases,
    }
}
