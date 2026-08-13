use pi_agent_core::harness::AgentToolExecutionContext;
use serde_json::{Value, json};

use super::{oracle::TsOracle, support::ok_response};

const FIRST_REQUEST_ID: u64 = 13_168;
const EXPECTED_FIXTURE_COUNT: usize = 8;

struct Fixture {
    request_id: u64,
    name: &'static str,
    cases: Vec<Case>,
}

struct Case {
    name: &'static str,
    started_at_bits: &'static str,
    agent_id_state: AgentIdState,
    agent_id: &'static str,
}

#[derive(Clone, Copy)]
enum AgentIdState {
    Absent,
    Present,
}

impl AgentIdState {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Absent => "absent",
            Self::Present => "present",
        }
    }
}

pub(crate) fn compare_all_fixtures(oracle: &mut TsOracle) {
    let fixtures = fixtures();
    assert_eq!(fixtures.len(), EXPECTED_FIXTURE_COUNT);
    assert_request_ids(&fixtures);

    for fixture in &fixtures {
        let expected = oracle.request(&request(fixture));
        let actual = rust_response(fixture);
        assert_eq!(
            actual, expected,
            "agent tool execution context mismatch: {}",
            fixture.name
        );
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
        "operation": "agent_tool_execution_context",
        "cases": fixture.cases.iter().map(case_request).collect::<Vec<_>>(),
    })
}

fn case_request(case: &Case) -> Value {
    json!({
        "name": case.name,
        "startedAtBits": case.started_at_bits,
        "agentIdState": case.agent_id_state.as_str(),
        "agentId": case.agent_id,
    })
}

fn rust_response(fixture: &Fixture) -> Value {
    let results = fixture.cases.iter().map(normalize_case).collect::<Vec<_>>();
    ok_response(fixture.request_id, json!({"results": results}))
}

fn normalize_case(case: &Case) -> Value {
    let context = AgentToolExecutionContext {
        started_at: binary64_from_bits(case.started_at_bits),
        agent_id: agent_id(case),
    };
    json!({
        "name": case.name,
        "startedAtBits": binary64_bits(context.started_at),
        "agentId": context.agent_id,
    })
}

fn agent_id(case: &Case) -> Option<String> {
    match case.agent_id_state {
        AgentIdState::Absent => None,
        AgentIdState::Present => Some(case.agent_id.to_owned()),
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
        absent_id_fixture(),
        empty_id_and_zero_fixture(),
        string_boundary_fixture(),
        signed_zero_and_subnormal_fixture(),
        finite_extremes_fixture(),
        non_finite_fixture(),
        mutation_shaped_fixture(),
    ]
}

fn ordinary_fixture() -> Fixture {
    fixture(
        13_168,
        "ordinary present identity",
        vec![
            case("ordinary", "3ff8000000000000", AgentIdState::Present, "agent-42"),
            case(
                "fractional timestamp",
                "400921fb54442d18",
                AgentIdState::Present,
                "worker/7",
            ),
        ],
    )
}

fn absent_id_fixture() -> Fixture {
    fixture(
        13_169,
        "absent optional identity",
        vec![
            case("absent ordinary", "3ff0000000000000", AgentIdState::Absent, ""),
            case("absent negative", "bff0000000000000", AgentIdState::Absent, "ignored"),
        ],
    )
}

fn empty_id_and_zero_fixture() -> Fixture {
    fixture(
        13_170,
        "empty identity and zero timestamp",
        vec![
            case("empty identity", "0000000000000000", AgentIdState::Present, ""),
            case(
                "negative zero identity",
                "8000000000000000",
                AgentIdState::Present,
                "zero-agent",
            ),
        ],
    )
}

fn string_boundary_fixture() -> Fixture {
    fixture(
        13_171,
        "unicode combining nul and newline identity",
        vec![
            case(
                "unicode combining",
                "4000000000000000",
                AgentIdState::Present,
                "代理🙂e\u{301}",
            ),
            case(
                "nul and newline",
                "c000000000000000",
                AgentIdState::Present,
                "before\0after\nnext\r",
            ),
        ],
    )
}

fn signed_zero_and_subnormal_fixture() -> Fixture {
    fixture(
        13_172,
        "signed zero and subnormal timestamps",
        vec![
            case(
                "positive zero",
                "0000000000000000",
                AgentIdState::Present,
                "positive-zero",
            ),
            case("negative zero", "8000000000000000", AgentIdState::Absent, ""),
            case(
                "positive subnormal",
                "0000000000000001",
                AgentIdState::Present,
                "subnormal-plus",
            ),
            case("negative subnormal", "8000000000000001", AgentIdState::Absent, ""),
        ],
    )
}

fn finite_extremes_fixture() -> Fixture {
    fixture(
        13_173,
        "finite binary64 extremes",
        vec![
            case("maximum finite", "7fefffffffffffff", AgentIdState::Present, "max"),
            case(
                "negative maximum finite",
                "ffefffffffffffff",
                AgentIdState::Present,
                "negative-max",
            ),
            case("minimum normal", "0010000000000000", AgentIdState::Absent, ""),
            case(
                "negative minimum normal",
                "8010000000000000",
                AgentIdState::Present,
                "minimum",
            ),
        ],
    )
}

fn non_finite_fixture() -> Fixture {
    fixture(
        13_174,
        "infinities and distinct nan payloads",
        vec![
            case(
                "positive infinity",
                "7ff0000000000000",
                AgentIdState::Present,
                "infinity-plus",
            ),
            case("negative infinity", "fff0000000000000", AgentIdState::Absent, ""),
            case(
                "positive nan payload",
                "7ff8000000000001",
                AgentIdState::Present,
                "nan-one",
            ),
            case(
                "distinct nan payload",
                "7ff8000000000042",
                AgentIdState::Present,
                "nan-payload",
            ),
            case("negative nan payload", "fff8000000000001", AgentIdState::Absent, ""),
        ],
    )
}

fn mutation_shaped_fixture() -> Fixture {
    fixture(
        13_175,
        "final mutation-shaped records",
        vec![
            case(
                "updated present identity",
                "4024000000000000",
                AgentIdState::Present,
                "updated-agent",
            ),
            case("updated absent identity", "c024000000000000", AgentIdState::Absent, ""),
            case("updated empty identity", "3fd0000000000000", AgentIdState::Present, ""),
        ],
    )
}

fn case(
    name: &'static str,
    started_at_bits: &'static str,
    agent_id_state: AgentIdState,
    agent_id: &'static str,
) -> Case {
    Case {
        name,
        started_at_bits,
        agent_id_state,
        agent_id,
    }
}

fn fixture(request_id: u64, name: &'static str, cases: Vec<Case>) -> Fixture {
    Fixture {
        request_id,
        name,
        cases,
    }
}
