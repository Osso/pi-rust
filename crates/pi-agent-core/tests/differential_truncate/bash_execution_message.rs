use pi_agent_core::harness::{BashExecutionMessage, BashExecutionRole};
use serde_json::{Value, json};

use super::{oracle::TsOracle, support::ok_response};

const FIRST_REQUEST_ID: u64 = 13_097;
const EXPECTED_FIXTURE_COUNT: usize = 8;

#[derive(Clone, Copy)]
enum ExitCodeState {
    Undefined,
    Value,
}

impl ExitCodeState {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Undefined => "undefined",
            Self::Value => "value",
        }
    }
}

#[derive(Clone, Copy)]
enum FullOutputPathState {
    Absent,
    Undefined,
    Value,
}

impl FullOutputPathState {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Absent => "absent",
            Self::Undefined => "undefined",
            Self::Value => "value",
        }
    }
}

#[derive(Clone, Copy)]
enum ExcludeFromContextState {
    Absent,
    Undefined,
    False,
    True,
}

impl ExcludeFromContextState {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Absent => "absent",
            Self::Undefined => "undefined",
            Self::False => "false",
            Self::True => "true",
        }
    }
}

struct Fixture {
    request_id: u64,
    name: &'static str,
    cases: Vec<Case>,
}

struct Case {
    name: &'static str,
    command: &'static str,
    output: &'static str,
    exit_code_state: ExitCodeState,
    exit_code_bits: &'static str,
    cancelled: bool,
    truncated: bool,
    full_output_path_state: FullOutputPathState,
    full_output_path: &'static str,
    timestamp_bits: &'static str,
    exclude_from_context_state: ExcludeFromContextState,
}

pub(crate) fn compare_all_fixtures(oracle: &mut TsOracle) {
    let fixtures = fixtures();
    assert_eq!(fixtures.len(), EXPECTED_FIXTURE_COUNT);
    assert_request_ids(&fixtures);

    for fixture in &fixtures {
        let expected = oracle.request(&request(fixture));
        let actual = rust_response(fixture);
        assert_eq!(actual, expected, "bash execution message mismatch: {}", fixture.name);
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
        "operation": "bash_execution_message",
        "cases": fixture.cases.iter().map(case_request).collect::<Vec<_>>(),
    })
}

fn case_request(case: &Case) -> Value {
    json!({
        "name": case.name,
        "command": case.command,
        "output": case.output,
        "exitCodeState": case.exit_code_state.as_str(),
        "exitCodeBits": case.exit_code_bits,
        "cancelled": case.cancelled,
        "truncated": case.truncated,
        "fullOutputPathState": case.full_output_path_state.as_str(),
        "fullOutputPath": case.full_output_path,
        "timestampBits": case.timestamp_bits,
        "excludeFromContextState": case.exclude_from_context_state.as_str(),
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
        "command": message.command,
        "output": message.output,
        "exitCodeBits": message.exit_code.map(binary64_bits),
        "cancelled": message.cancelled,
        "truncated": message.truncated,
        "fullOutputPath": message.full_output_path,
        "timestampBits": binary64_bits(message.timestamp),
        "excludeFromContext": message.exclude_from_context,
    })
}

fn build_message(case: &Case) -> BashExecutionMessage {
    BashExecutionMessage {
        role: BashExecutionRole::BashExecution,
        command: case.command.to_owned(),
        output: case.output.to_owned(),
        exit_code: match case.exit_code_state {
            ExitCodeState::Undefined => None,
            ExitCodeState::Value => Some(binary64_from_bits(case.exit_code_bits)),
        },
        cancelled: case.cancelled,
        truncated: case.truncated,
        full_output_path: match case.full_output_path_state {
            FullOutputPathState::Absent | FullOutputPathState::Undefined => None,
            FullOutputPathState::Value => Some(case.full_output_path.to_owned()),
        },
        timestamp: binary64_from_bits(case.timestamp_bits),
        exclude_from_context: match case.exclude_from_context_state {
            ExcludeFromContextState::Absent | ExcludeFromContextState::Undefined => None,
            ExcludeFromContextState::False => Some(false),
            ExcludeFromContextState::True => Some(true),
        },
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
        absent_optional_fixture(),
        undefined_optional_fixture(),
        empty_value_fixture(),
        string_boundary_fixture(),
        signed_zero_and_infinity_fixture(),
        finite_boundary_fixture(),
        nan_fixture(),
    ]
}

fn ordinary_fixture() -> Fixture {
    fixture(
        13_097,
        "ordinary required and optional values",
        vec![Case {
            name: "ordinary",
            command: "printf ordinary",
            output: "ordinary output",
            exit_code_state: ExitCodeState::Value,
            exit_code_bits: "000000000000002a",
            cancelled: false,
            truncated: true,
            full_output_path_state: FullOutputPathState::Value,
            full_output_path: "/tmp/full-output.log",
            timestamp_bits: "3ff8000000000000",
            exclude_from_context_state: ExcludeFromContextState::True,
        }],
    )
}

fn absent_optional_fixture() -> Fixture {
    fixture(
        13_098,
        "undefined exit and absent optionals",
        vec![Case {
            name: "absent-optionals",
            command: "true",
            output: "",
            exit_code_state: ExitCodeState::Undefined,
            exit_code_bits: "0000000000000000",
            cancelled: true,
            truncated: false,
            full_output_path_state: FullOutputPathState::Absent,
            full_output_path: "ignored",
            timestamp_bits: "0000000000000000",
            exclude_from_context_state: ExcludeFromContextState::Absent,
        }],
    )
}

fn undefined_optional_fixture() -> Fixture {
    fixture(
        13_099,
        "explicitly undefined optionals",
        vec![Case {
            name: "undefined-optionals",
            command: "echo undefined",
            output: "undefined output",
            exit_code_state: ExitCodeState::Undefined,
            exit_code_bits: "7ff8000000000001",
            cancelled: false,
            truncated: true,
            full_output_path_state: FullOutputPathState::Undefined,
            full_output_path: "ignored",
            timestamp_bits: "bff0000000000000",
            exclude_from_context_state: ExcludeFromContextState::Undefined,
        }],
    )
}

fn empty_value_fixture() -> Fixture {
    fixture(
        13_100,
        "empty strings empty path and false",
        vec![Case {
            name: "empty-values",
            command: "",
            output: "",
            exit_code_state: ExitCodeState::Value,
            exit_code_bits: "0000000000000000",
            cancelled: false,
            truncated: false,
            full_output_path_state: FullOutputPathState::Value,
            full_output_path: "",
            timestamp_bits: "8000000000000000",
            exclude_from_context_state: ExcludeFromContextState::False,
        }],
    )
}

fn string_boundary_fixture() -> Fixture {
    fixture(
        13_101,
        "unicode controls and XML-sensitive strings",
        vec![Case {
            name: "string-boundaries",
            command: "😀e\u{301}\0<&>\"'",
            output: "前\r\n中\n後\0<&>\"'",
            exit_code_state: ExitCodeState::Value,
            exit_code_bits: "3ff0000000000000",
            cancelled: true,
            truncated: true,
            full_output_path_state: FullOutputPathState::Value,
            full_output_path: "路径/😀/e\u{301}\0<&>\"'",
            timestamp_bits: "400921fb54442d18",
            exclude_from_context_state: ExcludeFromContextState::False,
        }],
    )
}

fn signed_zero_and_infinity_fixture() -> Fixture {
    fixture(
        13_102,
        "signed zero and infinities",
        vec![
            positive_zero_case(),
            negative_zero_case(),
            positive_infinity_case(),
            negative_infinity_case(),
        ],
    )
}

fn positive_zero_case() -> Case {
    Case {
        name: "positive-zero-exit-negative-zero-time",
        command: "zero-plus",
        output: "",
        exit_code_state: ExitCodeState::Value,
        exit_code_bits: "0000000000000000",
        cancelled: false,
        truncated: false,
        full_output_path_state: FullOutputPathState::Value,
        full_output_path: "/tmp/zero-plus",
        timestamp_bits: "8000000000000000",
        exclude_from_context_state: ExcludeFromContextState::False,
    }
}

fn negative_zero_case() -> Case {
    Case {
        name: "negative-zero-exit-positive-zero-time",
        command: "zero-minus",
        output: "zero",
        exit_code_state: ExitCodeState::Value,
        exit_code_bits: "8000000000000000",
        cancelled: true,
        truncated: false,
        full_output_path_state: FullOutputPathState::Value,
        full_output_path: "/tmp/zero-minus",
        timestamp_bits: "0000000000000000",
        exclude_from_context_state: ExcludeFromContextState::True,
    }
}

fn positive_infinity_case() -> Case {
    Case {
        name: "positive-infinity-exit-negative-infinity-time",
        command: "infinity-plus",
        output: "infinity",
        exit_code_state: ExitCodeState::Value,
        exit_code_bits: "7ff0000000000000",
        cancelled: false,
        truncated: true,
        full_output_path_state: FullOutputPathState::Value,
        full_output_path: "/tmp/infinity-plus",
        timestamp_bits: "fff0000000000000",
        exclude_from_context_state: ExcludeFromContextState::False,
    }
}

fn negative_infinity_case() -> Case {
    Case {
        name: "negative-infinity-exit-positive-infinity-time",
        command: "infinity-minus",
        output: "-infinity",
        exit_code_state: ExitCodeState::Value,
        exit_code_bits: "fff0000000000000",
        cancelled: true,
        truncated: true,
        full_output_path_state: FullOutputPathState::Value,
        full_output_path: "/tmp/infinity-minus",
        timestamp_bits: "7ff0000000000000",
        exclude_from_context_state: ExcludeFromContextState::True,
    }
}

fn finite_boundary_fixture() -> Fixture {
    fixture(
        13_103,
        "subnormal finite fractional and unsafe-boundary bits",
        vec![
            minimum_subnormal_case(),
            maximum_finite_case(),
            fractional_case(),
            unsafe_integer_boundary_case(),
        ],
    )
}

fn minimum_subnormal_case() -> Case {
    Case {
        name: "minimum-subnormal",
        command: "subnormal",
        output: "min",
        exit_code_state: ExitCodeState::Value,
        exit_code_bits: "0000000000000001",
        cancelled: false,
        truncated: false,
        full_output_path_state: FullOutputPathState::Undefined,
        full_output_path: "ignored",
        timestamp_bits: "8000000000000001",
        exclude_from_context_state: ExcludeFromContextState::Undefined,
    }
}

fn maximum_finite_case() -> Case {
    Case {
        name: "maximum-finite",
        command: "max-finite",
        output: "max",
        exit_code_state: ExitCodeState::Value,
        exit_code_bits: "7fefffffffffffff",
        cancelled: true,
        truncated: false,
        full_output_path_state: FullOutputPathState::Value,
        full_output_path: "/tmp/max-finite",
        timestamp_bits: "ffefffffffffffff",
        exclude_from_context_state: ExcludeFromContextState::False,
    }
}

fn fractional_case() -> Case {
    Case {
        name: "fractional",
        command: "fractional",
        output: "1.5 and pi",
        exit_code_state: ExitCodeState::Value,
        exit_code_bits: "3ff8000000000000",
        cancelled: false,
        truncated: true,
        full_output_path_state: FullOutputPathState::Absent,
        full_output_path: "ignored",
        timestamp_bits: "400921fb54442d18",
        exclude_from_context_state: ExcludeFromContextState::True,
    }
}

fn unsafe_integer_boundary_case() -> Case {
    Case {
        name: "unsafe-integer-boundary",
        command: "unsafe-boundary",
        output: "2^53 boundary",
        exit_code_state: ExitCodeState::Value,
        exit_code_bits: "4340000000000001",
        cancelled: true,
        truncated: true,
        full_output_path_state: FullOutputPathState::Value,
        full_output_path: "/tmp/unsafe-boundary",
        timestamp_bits: "c340000000000000",
        exclude_from_context_state: ExcludeFromContextState::False,
    }
}

fn nan_fixture() -> Fixture {
    fixture(
        13_104,
        "positive and negative NaN payloads",
        vec![
            positive_quiet_nan_case(),
            positive_payload_nan_case(),
            negative_payload_nan_case(),
            negative_signaling_nan_case(),
        ],
    )
}

fn positive_quiet_nan_case() -> Case {
    Case {
        name: "positive-quiet-nan-one",
        command: "nan-positive-one",
        output: "nan 1",
        exit_code_state: ExitCodeState::Value,
        exit_code_bits: "7ff8000000000001",
        cancelled: false,
        truncated: false,
        full_output_path_state: FullOutputPathState::Absent,
        full_output_path: "ignored",
        timestamp_bits: "fff0000000000001",
        exclude_from_context_state: ExcludeFromContextState::False,
    }
}

fn positive_payload_nan_case() -> Case {
    Case {
        name: "positive-quiet-nan-payload",
        command: "nan-positive-payload",
        output: "nan 2",
        exit_code_state: ExitCodeState::Value,
        exit_code_bits: "7ff8000000000042",
        cancelled: true,
        truncated: true,
        full_output_path_state: FullOutputPathState::Undefined,
        full_output_path: "ignored",
        timestamp_bits: "7ff0000000000002",
        exclude_from_context_state: ExcludeFromContextState::True,
    }
}

fn negative_payload_nan_case() -> Case {
    Case {
        name: "negative-quiet-nan-payload",
        command: "nan-negative-payload",
        output: "nan 3",
        exit_code_state: ExitCodeState::Value,
        exit_code_bits: "fff8000000000042",
        cancelled: false,
        truncated: true,
        full_output_path_state: FullOutputPathState::Value,
        full_output_path: "",
        timestamp_bits: "fff8000000000001",
        exclude_from_context_state: ExcludeFromContextState::False,
    }
}

fn negative_signaling_nan_case() -> Case {
    Case {
        name: "negative-signaling-nan",
        command: "nan-negative-signaling",
        output: "nan 4",
        exit_code_state: ExitCodeState::Value,
        exit_code_bits: "fff0000000000002",
        cancelled: true,
        truncated: false,
        full_output_path_state: FullOutputPathState::Value,
        full_output_path: "/tmp/nan",
        timestamp_bits: "7ff8000000000042",
        exclude_from_context_state: ExcludeFromContextState::True,
    }
}

fn fixture(request_id: u64, name: &'static str, cases: Vec<Case>) -> Fixture {
    Fixture {
        request_id,
        name,
        cases,
    }
}
