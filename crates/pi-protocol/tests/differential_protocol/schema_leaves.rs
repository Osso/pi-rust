use pi_protocol::{ModelRef, SessionPhase, ThinkingLevel};
use serde_json::{Value, json};

use super::oracle::TsOracle;

const FIRST_REQUEST_ID: u64 = 2000;

struct StringFixture {
    name: &'static str,
    value: &'static str,
}

impl StringFixture {
    const fn new(name: &'static str, value: &'static str) -> Self {
        Self { name, value }
    }

    fn request(&self, id: u64, operation: &'static str) -> Value {
        json!({
            "id": id,
            "operation": operation,
            "value": self.value,
        })
    }
}

struct ModelRefFixture {
    name: &'static str,
    provider: &'static str,
    id: &'static str,
}

impl ModelRefFixture {
    const fn new(name: &'static str, provider: &'static str, id: &'static str) -> Self {
        Self { name, provider, id }
    }

    fn request(&self, id: u64) -> Value {
        json!({
            "id": id,
            "operation": "validate_model_ref",
            "value": {
                "provider": self.provider,
                "id": self.id,
            },
        })
    }
}

const THINKING_FIXTURES: &[StringFixture] = &[
    StringFixture::new("off", "off"),
    StringFixture::new("minimal", "minimal"),
    StringFixture::new("low", "low"),
    StringFixture::new("medium", "medium"),
    StringFixture::new("high", "high"),
    StringFixture::new("xhigh", "xhigh"),
    StringFixture::new("max", "max"),
    StringFixture::new("empty", ""),
    StringFixture::new("uppercase", "HIGH"),
    StringFixture::new("provider-only level", "ultra"),
    StringFixture::new("leading space", " high"),
    StringFixture::new("trailing space", "high "),
];

const PHASE_FIXTURES: &[StringFixture] = &[
    StringFixture::new("idle", "idle"),
    StringFixture::new("turn", "turn"),
    StringFixture::new("compaction", "compaction"),
    StringFixture::new("branch summary", "branch_summary"),
    StringFixture::new("retry", "retry"),
    StringFixture::new("empty", ""),
    StringFixture::new("uppercase", "Idle"),
    StringFixture::new("hyphenated", "branch-summary"),
    StringFixture::new("leading space", " retry"),
    StringFixture::new("trailing space", "retry "),
];

const MODEL_REF_FIXTURES: &[ModelRefFixture] = &[
    ModelRefFixture::new("ordinary", "openai", "gpt-5"),
    ModelRefFixture::new("unicode", "é", "モデル"),
    ModelRefFixture::new("supplementary and combining", "𐅑", "e\u{301}"),
    ModelRefFixture::new("whitespace-only", "   ", "\t\n"),
    ModelRefFixture::new("embedded NUL", "provider\0name", "model\0id"),
    ModelRefFixture::new("empty provider", "", "model"),
    ModelRefFixture::new("empty id", "provider", ""),
    ModelRefFixture::new("both empty", "", ""),
];

fn valid_string_response(id: u64, value: &str) -> Value {
    json!({
        "id": id,
        "ok": true,
        "output": {
            "valid": true,
            "value": value,
        },
    })
}

fn valid_model_ref_response(id: u64, provider: &str, model_id: &str) -> Value {
    json!({
        "id": id,
        "ok": true,
        "output": {
            "valid": true,
            "value": {
                "provider": provider,
                "id": model_id,
            },
        },
    })
}

fn invalid_response(id: u64) -> Value {
    json!({
        "id": id,
        "ok": true,
        "output": {"valid": false},
    })
}

fn compare_thinking_fixture(oracle: &mut TsOracle, fixture: &StringFixture, id: u64) {
    let expected = oracle.request(&fixture.request(id, "validate_thinking_level"));
    let actual = match fixture.value.parse::<ThinkingLevel>() {
        Ok(level) => valid_string_response(id, level.as_str()),
        Err(_) => invalid_response(id),
    };
    assert_eq!(actual, expected, "thinking-level mismatch: {}", fixture.name);
}

fn compare_phase_fixture(oracle: &mut TsOracle, fixture: &StringFixture, id: u64) {
    let expected = oracle.request(&fixture.request(id, "validate_session_phase"));
    let actual = match fixture.value.parse::<SessionPhase>() {
        Ok(phase) => valid_string_response(id, phase.as_str()),
        Err(_) => invalid_response(id),
    };
    assert_eq!(actual, expected, "session-phase mismatch: {}", fixture.name);
}

fn compare_model_ref_fixture(oracle: &mut TsOracle, fixture: &ModelRefFixture, id: u64) {
    let expected = oracle.request(&fixture.request(id));
    let actual = match ModelRef::new(fixture.provider.to_owned(), fixture.id.to_owned()) {
        Ok(model_ref) => valid_model_ref_response(id, model_ref.provider(), model_ref.id()),
        Err(_) => invalid_response(id),
    };
    assert_eq!(actual, expected, "model-ref mismatch: {}", fixture.name);
}

pub(crate) fn compare_all_fixtures(oracle: &mut TsOracle) {
    let mut id = FIRST_REQUEST_ID;
    for fixture in THINKING_FIXTURES {
        compare_thinking_fixture(oracle, fixture, id);
        id += 1;
    }
    for fixture in PHASE_FIXTURES {
        compare_phase_fixture(oracle, fixture, id);
        id += 1;
    }
    for fixture in MODEL_REF_FIXTURES {
        compare_model_ref_fixture(oracle, fixture, id);
        id += 1;
    }
}
