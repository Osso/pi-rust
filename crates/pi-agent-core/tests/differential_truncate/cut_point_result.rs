use pi_agent_core::harness::compaction::CutPointResult;
use serde_json::{Value, json};

use super::{oracle::TsOracle, support::ok_response};

const FIRST_REQUEST_ID: u64 = 13_130;
const EXPECTED_FIXTURE_COUNT: usize = 8;

struct Fixture {
    request_id: u64,
    name: &'static str,
    cases: Vec<Case>,
}

struct Case {
    name: &'static str,
    first_kept_entry_index_bits: &'static str,
    turn_start_index_bits: &'static str,
    is_split_turn: bool,
}

pub(crate) fn compare_all_fixtures(oracle: &mut TsOracle) {
    let fixtures = fixtures();
    assert_eq!(fixtures.len(), EXPECTED_FIXTURE_COUNT);
    assert_request_ids(&fixtures);

    for fixture in &fixtures {
        let expected = oracle.request(&request(fixture));
        let actual = rust_response(fixture);
        assert_eq!(actual, expected, "cut-point result mismatch: {}", fixture.name);
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
        "operation": "cut_point_result",
        "cases": fixture.cases.iter().map(case_request).collect::<Vec<_>>(),
    })
}

fn case_request(case: &Case) -> Value {
    json!({
        "name": case.name,
        "firstKeptEntryIndexBits": case.first_kept_entry_index_bits,
        "turnStartIndexBits": case.turn_start_index_bits,
        "isSplitTurn": case.is_split_turn,
    })
}

fn rust_response(fixture: &Fixture) -> Value {
    let results = fixture.cases.iter().map(normalize_case).collect::<Vec<_>>();
    ok_response(fixture.request_id, json!({"results": results}))
}

fn normalize_case(case: &Case) -> Value {
    let result = CutPointResult {
        first_kept_entry_index: binary64_from_bits(case.first_kept_entry_index_bits),
        turn_start_index: binary64_from_bits(case.turn_start_index_bits),
        is_split_turn: case.is_split_turn,
    };
    json!({
        "name": case.name,
        "firstKeptEntryIndexBits": binary64_bits(result.first_kept_entry_index),
        "turnStartIndexBits": binary64_bits(result.turn_start_index),
        "isSplitTurn": result.is_split_turn,
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
        sentinel_fixture(),
        mutation_shaped_fixture(),
        signed_zero_fixture(),
        finite_extremes_fixture(),
        infinity_fixture(),
        nan_fixture(),
        mixed_boundary_fixture(),
    ]
}

fn ordinary_fixture() -> Fixture {
    fixture(
        13_130,
        "ordinary default and split values",
        vec![
            case("default", "0000000000000000", "bff0000000000000", false),
            case("kept prefix", "4014000000000000", "bff0000000000000", false),
            case("split turn", "4020000000000000", "4008000000000000", true),
        ],
    )
}

fn sentinel_fixture() -> Fixture {
    fixture(
        13_131,
        "sentinel and boolean combinations",
        vec![
            case("sentinel false", "bff0000000000000", "bff0000000000000", false),
            case("sentinel true", "bff0000000000000", "bff0000000000000", true),
            case("zero split", "0000000000000000", "0000000000000000", true),
            case("positive unsplit", "4010000000000000", "0000000000000000", false),
        ],
    )
}

fn mutation_shaped_fixture() -> Fixture {
    fixture(
        13_132,
        "final values from mutable source-shaped records",
        vec![
            case("updated indexes", "4045000000000000", "403c000000000000", true),
            case("updated sentinel", "4022000000000000", "bff0000000000000", false),
            case("updated boolean", "0000000000000000", "4014000000000000", true),
        ],
    )
}

fn signed_zero_fixture() -> Fixture {
    fixture(
        13_133,
        "signed zero and subnormal values",
        vec![
            case("negative zero first", "8000000000000000", "0000000000000000", false),
            case("negative zero turn", "0000000000000000", "8000000000000000", true),
            case("positive subnormal", "0000000000000001", "8000000000000001", false),
            case("negative subnormal", "8000000000000001", "0000000000000001", true),
        ],
    )
}

fn finite_extremes_fixture() -> Fixture {
    fixture(
        13_134,
        "finite extremes",
        vec![
            case("minimum normal", "0010000000000000", "8010000000000000", false),
            case("negative maximum", "ffefffffffffffff", "7fefffffffffffff", true),
            case("maximum finite", "7fefffffffffffff", "ffefffffffffffff", false),
            case("minimum subnormal", "0000000000000001", "8000000000000001", true),
        ],
    )
}

fn infinity_fixture() -> Fixture {
    fixture(
        13_135,
        "infinite values",
        vec![
            case("positive infinity", "7ff0000000000000", "fff0000000000000", false),
            case("negative infinity", "fff0000000000000", "7ff0000000000000", true),
            case("both positive infinity", "7ff0000000000000", "7ff0000000000000", false),
            case("both negative infinity", "fff0000000000000", "fff0000000000000", true),
        ],
    )
}

fn nan_fixture() -> Fixture {
    fixture(
        13_136,
        "distinct NaN payloads and signs",
        vec![
            case("positive payload", "7ff8000000000001", "7ff8000000000042", false),
            case("negative payload", "fff8000000000001", "fff8000000000042", true),
            case("mixed payload signs", "7ff8000000000042", "fff8000000000001", false),
        ],
    )
}

fn mixed_boundary_fixture() -> Fixture {
    fixture(
        13_137,
        "mixed boundary corpus",
        vec![
            case("one and two", "3ff0000000000000", "4000000000000000", false),
            case("three and five", "4008000000000000", "4014000000000000", true),
            case("max and minimum", "7fefffffffffffff", "0000000000000001", false),
            case("NaN and signed zero", "7ff8000000000001", "8000000000000000", true),
        ],
    )
}

fn case(
    name: &'static str,
    first_kept_entry_index_bits: &'static str,
    turn_start_index_bits: &'static str,
    is_split_turn: bool,
) -> Case {
    Case {
        name,
        first_kept_entry_index_bits,
        turn_start_index_bits,
        is_split_turn,
    }
}

fn fixture(request_id: u64, name: &'static str, cases: Vec<Case>) -> Fixture {
    Fixture {
        request_id,
        name,
        cases,
    }
}
