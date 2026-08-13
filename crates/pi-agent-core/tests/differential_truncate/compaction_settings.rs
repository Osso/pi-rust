use pi_agent_core::harness::{CompactionSettings, should_compact};
use serde_json::{Value, json};

use super::{oracle::TsOracle, support::ok_response};

const FIRST_REQUEST_ID: u64 = 13_113;
const EXPECTED_FIXTURE_COUNT: usize = 8;

struct Fixture {
    request_id: u64,
    name: &'static str,
    cases: Vec<Case>,
}

struct Case {
    name: &'static str,
    context_tokens_bits: &'static str,
    context_window_bits: &'static str,
    enabled: bool,
    reserve_tokens_bits: &'static str,
    keep_recent_tokens_bits: &'static str,
}

pub(crate) fn compare_all_fixtures(oracle: &mut TsOracle) {
    let fixtures = fixtures();
    assert_eq!(fixtures.len(), EXPECTED_FIXTURE_COUNT);
    assert_request_ids(&fixtures);

    for fixture in &fixtures {
        let expected = oracle.request(&request(fixture));
        let actual = rust_response(fixture);
        assert_eq!(actual, expected, "compaction settings mismatch: {}", fixture.name);
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
        "operation": "should_compact",
        "cases": fixture.cases.iter().map(case_request).collect::<Vec<_>>(),
    })
}

fn case_request(case: &Case) -> Value {
    json!({
        "name": case.name,
        "contextTokensBits": case.context_tokens_bits,
        "contextWindowBits": case.context_window_bits,
        "enabled": case.enabled,
        "reserveTokensBits": case.reserve_tokens_bits,
        "keepRecentTokensBits": case.keep_recent_tokens_bits,
    })
}

fn rust_response(fixture: &Fixture) -> Value {
    let results = fixture.cases.iter().map(normalize_case).collect::<Vec<_>>();
    ok_response(fixture.request_id, json!({"results": results}))
}

fn normalize_case(case: &Case) -> Value {
    let settings = CompactionSettings {
        enabled: case.enabled,
        reserve_tokens: binary64_from_bits(case.reserve_tokens_bits),
        keep_recent_tokens: binary64_from_bits(case.keep_recent_tokens_bits),
    };
    json!({
        "name": case.name,
        "shouldCompact": should_compact(
            binary64_from_bits(case.context_tokens_bits),
            binary64_from_bits(case.context_window_bits),
            &settings,
        ),
    })
}

fn binary64_from_bits(bits: &str) -> f64 {
    let value = u64::from_str_radix(bits, 16).expect("fixture bits must be lowercase hexadecimal");
    f64::from_bits(value)
}

fn fixtures() -> Vec<Fixture> {
    vec![
        ordinary_fixture(),
        disabled_fixture(),
        keep_recent_fixture(),
        signed_zero_and_subnormal_fixture(),
        negative_values_fixture(),
        infinity_and_overflow_fixture(),
        nan_fixture(),
        finite_boundary_fixture(),
    ]
}

fn ordinary_fixture() -> Fixture {
    fixture(13_113, "ordinary threshold boundaries", ordinary_cases())
}

fn ordinary_cases() -> Vec<Case> {
    vec![
        case(
            "exceeded",
            "4057c00000000000",
            "4059000000000000",
            true,
            "4024000000000000",
            "4034000000000000",
        ),
        case(
            "equal",
            "4056800000000000",
            "4059000000000000",
            true,
            "4024000000000000",
            "4034000000000000",
        ),
        case(
            "below",
            "4056400000000000",
            "4059000000000000",
            true,
            "4024000000000000",
            "4034000000000000",
        ),
    ]
}

fn disabled_fixture() -> Fixture {
    fixture(
        13_114,
        "disabled short circuit",
        vec![
            case(
                "disabled ordinary",
                "7ff0000000000000",
                "fff0000000000000",
                false,
                "7ff8000000000042",
                "fff8000000000042",
            ),
            case(
                "disabled nan",
                "7ff8000000000001",
                "7ff8000000000042",
                false,
                "0000000000000001",
                "8000000000000001",
            ),
        ],
    )
}

fn keep_recent_fixture() -> Fixture {
    fixture(13_115, "keep recent tokens is unobserved", keep_recent_cases())
}

fn keep_recent_cases() -> Vec<Case> {
    vec![
        case(
            "keep recent zero",
            "4058c00000000000",
            "4059000000000000",
            true,
            "4024000000000000",
            "0000000000000000",
        ),
        case(
            "keep recent nan",
            "4058c00000000000",
            "4059000000000000",
            true,
            "4024000000000000",
            "7ff8000000000042",
        ),
        case(
            "keep recent infinity",
            "4058c00000000000",
            "4059000000000000",
            true,
            "4024000000000000",
            "7ff0000000000000",
        ),
    ]
}

fn signed_zero_and_subnormal_fixture() -> Fixture {
    fixture(
        13_116,
        "signed zero and subnormal values",
        signed_zero_and_subnormal_cases(),
    )
}

fn signed_zero_and_subnormal_cases() -> Vec<Case> {
    vec![
        positive_subnormal_case(),
        negative_subnormal_case(),
        negative_zero_reserve_case(),
        negative_zero_context_case(),
    ]
}

fn positive_subnormal_case() -> Case {
    case(
        "positive subnormal",
        "0000000000000001",
        "0000000000000000",
        true,
        "0000000000000000",
        "0000000000000000",
    )
}

fn negative_subnormal_case() -> Case {
    case(
        "negative subnormal",
        "8000000000000001",
        "0000000000000000",
        true,
        "0000000000000000",
        "0000000000000000",
    )
}

fn negative_zero_reserve_case() -> Case {
    case(
        "negative zero reserve",
        "0000000000000000",
        "0000000000000000",
        true,
        "8000000000000000",
        "0000000000000000",
    )
}

fn negative_zero_context_case() -> Case {
    case(
        "negative zero context",
        "8000000000000000",
        "0000000000000000",
        true,
        "0000000000000000",
        "0000000000000000",
    )
}

fn negative_values_fixture() -> Fixture {
    fixture(13_117, "negative reserve and context window", negative_value_cases())
}

fn negative_value_cases() -> Vec<Case> {
    vec![
        case(
            "negative reserve",
            "0000000000000000",
            "0000000000000000",
            true,
            "c024000000000000",
            "0000000000000000",
        ),
        case(
            "negative window",
            "bff0000000000000",
            "c024000000000000",
            true,
            "4024000000000000",
            "0000000000000000",
        ),
        case(
            "negative threshold",
            "c024000000000000",
            "bff0000000000000",
            true,
            "bff0000000000000",
            "0000000000000000",
        ),
    ]
}

fn infinity_and_overflow_fixture() -> Fixture {
    fixture(
        13_118,
        "infinities and subtraction overflow",
        infinity_and_overflow_cases(),
    )
}

fn infinity_and_overflow_cases() -> Vec<Case> {
    vec![
        positive_infinity_reserve_case(),
        negative_infinity_reserve_case(),
        positive_overflow_case(),
        infinite_equality_case(),
    ]
}

fn positive_infinity_reserve_case() -> Case {
    case(
        "positive infinity reserve",
        "7fefffffffffffff",
        "0000000000000000",
        true,
        "7ff0000000000000",
        "0000000000000000",
    )
}

fn negative_infinity_reserve_case() -> Case {
    case(
        "negative infinity reserve",
        "0000000000000000",
        "0000000000000000",
        true,
        "fff0000000000000",
        "0000000000000000",
    )
}

fn positive_overflow_case() -> Case {
    case(
        "positive overflow",
        "7fefffffffffffff",
        "ffefffffffffffff",
        true,
        "7fefffffffffffff",
        "0000000000000000",
    )
}

fn infinite_equality_case() -> Case {
    case(
        "infinite equality",
        "7ff0000000000000",
        "7ff0000000000000",
        true,
        "0000000000000000",
        "0000000000000000",
    )
}

fn nan_fixture() -> Fixture {
    fixture(13_119, "nan operands", nan_cases())
}

fn nan_cases() -> Vec<Case> {
    vec![
        nan_context_case(),
        nan_window_case(),
        nan_reserve_case(),
        negative_nan_reserve_case(),
    ]
}

fn nan_context_case() -> Case {
    case(
        "nan context",
        "7ff8000000000001",
        "0000000000000000",
        true,
        "0000000000000000",
        "0000000000000000",
    )
}

fn nan_window_case() -> Case {
    case(
        "nan window",
        "0000000000000000",
        "7ff8000000000042",
        true,
        "0000000000000000",
        "0000000000000000",
    )
}

fn nan_reserve_case() -> Case {
    case(
        "nan reserve",
        "0000000000000000",
        "0000000000000000",
        true,
        "fff8000000000042",
        "0000000000000000",
    )
}

fn negative_nan_reserve_case() -> Case {
    case(
        "negative nan reserve",
        "0000000000000000",
        "0000000000000000",
        true,
        "fff8000000000001",
        "7ff8000000000042",
    )
}

fn finite_boundary_fixture() -> Fixture {
    fixture(13_120, "finite boundaries and cancellation", finite_boundary_cases())
}

fn finite_boundary_cases() -> Vec<Case> {
    vec![
        max_finite_cancellation_case(),
        max_finite_overflow_case(),
        minimum_normal_case(),
        negative_minimum_normal_case(),
    ]
}

fn max_finite_cancellation_case() -> Case {
    case(
        "max finite cancellation",
        "0000000000000000",
        "7fefffffffffffff",
        true,
        "7fefffffffffffff",
        "0000000000000000",
    )
}

fn max_finite_overflow_case() -> Case {
    case(
        "max finite overflow",
        "7fefffffffffffff",
        "ffefffffffffffff",
        true,
        "7fefffffffffffff",
        "0000000000000000",
    )
}

fn minimum_normal_case() -> Case {
    case(
        "minimum normal",
        "0010000000000000",
        "0000000000000000",
        true,
        "0000000000000000",
        "0010000000000000",
    )
}

fn negative_minimum_normal_case() -> Case {
    case(
        "negative minimum normal",
        "8010000000000000",
        "0000000000000000",
        true,
        "0000000000000000",
        "8010000000000000",
    )
}

fn case(
    name: &'static str,
    context_tokens_bits: &'static str,
    context_window_bits: &'static str,
    enabled: bool,
    reserve_tokens_bits: &'static str,
    keep_recent_tokens_bits: &'static str,
) -> Case {
    Case {
        name,
        context_tokens_bits,
        context_window_bits,
        enabled,
        reserve_tokens_bits,
        keep_recent_tokens_bits,
    }
}

fn fixture(request_id: u64, name: &'static str, cases: Vec<Case>) -> Fixture {
    Fixture {
        request_id,
        name,
        cases,
    }
}
