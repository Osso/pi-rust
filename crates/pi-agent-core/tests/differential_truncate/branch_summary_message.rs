use pi_agent_core::harness::{BranchSummaryMessage, BranchSummaryRole};
use serde_json::{Value, json};

use super::{oracle::TsOracle, support::ok_response};

const FIRST_REQUEST_ID: u64 = 13_105;
const EXPECTED_FIXTURE_COUNT: usize = 8;

struct Fixture {
    request_id: u64,
    name: &'static str,
    cases: Vec<Case>,
}

struct Case {
    name: &'static str,
    summary: &'static str,
    from_id: &'static str,
    timestamp_bits: &'static str,
}

pub(crate) fn compare_all_fixtures(oracle: &mut TsOracle) {
    let fixtures = fixtures();
    assert_eq!(fixtures.len(), EXPECTED_FIXTURE_COUNT);
    assert_request_ids(&fixtures);

    for fixture in &fixtures {
        let expected = oracle.request(&request(fixture));
        let actual = rust_response(fixture);
        assert_eq!(actual, expected, "branch summary message mismatch: {}", fixture.name);
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
        "operation": "branch_summary_message",
        "cases": fixture.cases.iter().map(case_request).collect::<Vec<_>>(),
    })
}

fn case_request(case: &Case) -> Value {
    json!({
        "name": case.name,
        "summary": case.summary,
        "fromId": case.from_id,
        "timestampBits": case.timestamp_bits,
    })
}

fn rust_response(fixture: &Fixture) -> Value {
    let results = fixture.cases.iter().map(normalize_case).collect::<Vec<_>>();
    ok_response(fixture.request_id, json!({"results": results}))
}

fn normalize_case(case: &Case) -> Value {
    let message = build_message(case);
    json!({
        "name": case.name,
        "role": message.role.as_str(),
        "summary": message.summary,
        "fromId": message.from_id,
        "timestampBits": binary64_bits(message.timestamp),
    })
}

fn build_message(case: &Case) -> BranchSummaryMessage {
    BranchSummaryMessage {
        role: BranchSummaryRole::BranchSummary,
        summary: case.summary.to_owned(),
        from_id: case.from_id.to_owned(),
        timestamp: binary64_from_bits(case.timestamp_bits),
    }
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
        empty_strings_fixture(),
        unicode_fixture(),
        control_and_punctuation_fixture(),
        signed_zero_fixture(),
        infinity_fixture(),
        finite_boundary_fixture(),
        nan_fixture(),
    ]
}

fn ordinary_fixture() -> Fixture {
    fixture(
        13_105,
        "ordinary values",
        vec![Case {
            name: "ordinary",
            summary: "A concise branch summary.",
            from_id: "branch-42",
            timestamp_bits: "3ff8000000000000",
        }],
    )
}

fn empty_strings_fixture() -> Fixture {
    fixture(
        13_106,
        "empty strings",
        vec![Case {
            name: "empty-values",
            summary: "",
            from_id: "",
            timestamp_bits: "0000000000000000",
        }],
    )
}

fn unicode_fixture() -> Fixture {
    fixture(
        13_107,
        "unicode supplementary combining and nul",
        vec![Case {
            name: "unicode-boundaries",
            summary: "😀e\u{301}\0前",
            from_id: "路径/😀/e\u{301}\0/branch",
            timestamp_bits: "400921fb54442d18",
        }],
    )
}

fn control_and_punctuation_fixture() -> Fixture {
    fixture(
        13_108,
        "line endings xml punctuation and mutation-shaped values",
        vec![
            Case {
                name: "raw-strings",
                summary: "before\r\n<summary>\n<&>\"'",
                from_id: "id\r\n<&>\"'",
                timestamp_bits: "bff0000000000000",
            },
            Case {
                name: "mutation-shaped",
                summary: "updated summary",
                from_id: "updated-id",
                timestamp_bits: "3fe0000000000000",
            },
        ],
    )
}

fn signed_zero_fixture() -> Fixture {
    fixture(
        13_109,
        "positive and negative zero",
        vec![
            Case {
                name: "positive-zero",
                summary: "plus zero",
                from_id: "zero-plus",
                timestamp_bits: "0000000000000000",
            },
            Case {
                name: "negative-zero",
                summary: "minus zero",
                from_id: "zero-minus",
                timestamp_bits: "8000000000000000",
            },
        ],
    )
}

fn infinity_fixture() -> Fixture {
    fixture(
        13_110,
        "positive and negative infinity",
        vec![
            Case {
                name: "positive-infinity",
                summary: "plus infinity",
                from_id: "infinity-plus",
                timestamp_bits: "7ff0000000000000",
            },
            Case {
                name: "negative-infinity",
                summary: "minus infinity",
                from_id: "infinity-minus",
                timestamp_bits: "fff0000000000000",
            },
        ],
    )
}

fn finite_boundary_fixture() -> Fixture {
    fixture(
        13_111,
        "finite binary64 boundaries and unsafe integer boundary",
        vec![
            Case {
                name: "positive-subnormal",
                summary: "small positive",
                from_id: "subnormal-plus",
                timestamp_bits: "0000000000000001",
            },
            Case {
                name: "negative-subnormal",
                summary: "small negative",
                from_id: "subnormal-minus",
                timestamp_bits: "8000000000000001",
            },
            Case {
                name: "max-finite",
                summary: "largest finite",
                from_id: "max-finite",
                timestamp_bits: "7fefffffffffffff",
            },
            Case {
                name: "ordinary-fraction",
                summary: "fractional timestamp",
                from_id: "fraction",
                timestamp_bits: "3fd5555555555555",
            },
            Case {
                name: "unsafe-integer-boundary",
                summary: "large integer boundary",
                from_id: "unsafe-boundary",
                timestamp_bits: "4340000000000000",
            },
        ],
    )
}

fn nan_fixture() -> Fixture {
    fixture(
        13_112,
        "nan payloads and deterministic mixed values",
        vec![
            Case {
                name: "positive-nan-one",
                summary: "nan one",
                from_id: "nan-positive-one",
                timestamp_bits: "7ff8000000000001",
            },
            Case {
                name: "positive-nan-payload",
                summary: "nan payload",
                from_id: "nan-positive-payload",
                timestamp_bits: "7ff8000000000042",
            },
            Case {
                name: "negative-nan-one",
                summary: "negative nan one",
                from_id: "nan-negative-one",
                timestamp_bits: "fff8000000000001",
            },
            Case {
                name: "negative-nan-payload",
                summary: "negative nan payload",
                from_id: "nan-negative-payload",
                timestamp_bits: "fff8000000000042",
            },
            Case {
                name: "mixed-ordinary",
                summary: "deterministic ordinary",
                from_id: "mixed-ordinary",
                timestamp_bits: "4024000000000000",
            },
            Case {
                name: "mixed-negative",
                summary: "deterministic negative",
                from_id: "mixed-negative",
                timestamp_bits: "c024000000000000",
            },
        ],
    )
}

fn fixture(request_id: u64, name: &'static str, cases: Vec<Case>) -> Fixture {
    Fixture {
        request_id,
        name,
        cases,
    }
}
