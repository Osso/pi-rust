use pi_protocol::ProtocolErrorCode;
use serde_json::{Value, json};

use super::oracle::TsOracle;

const FIRST_REQUEST_ID: u64 = 7000;
const FIXTURE_COUNT: u64 = 14;

struct Fixture {
    name: &'static str,
    value: &'static str,
}

impl Fixture {
    const fn new(name: &'static str, value: &'static str) -> Self {
        Self { name, value }
    }

    fn request(&self, id: u64) -> Value {
        json!({
            "id": id,
            "operation": "validate_protocol_error_code",
            "value": self.value,
        })
    }

    fn rust_response(&self, id: u64) -> Value {
        match self.value.parse::<ProtocolErrorCode>() {
            Ok(code) => valid_response(id, code.as_str()),
            Err(_) => invalid_response(id),
        }
    }
}

const FIXTURES: &[Fixture] = &[
    Fixture::new("version", "version"),
    Fixture::new("busy", "busy"),
    Fixture::new("session locked", "session_locked"),
    Fixture::new("not found", "not_found"),
    Fixture::new("invalid request", "invalid_request"),
    Fixture::new("not implemented", "not_implemented"),
    Fixture::new("internal error", "internal_error"),
    Fixture::new("empty", ""),
    Fixture::new("uppercase", "VERSION"),
    Fixture::new("hyphenated", "session-locked"),
    Fixture::new("unknown", "unknown"),
    Fixture::new("auth", "auth"),
    Fixture::new("leading space", " version"),
    Fixture::new("trailing space", "internal_error "),
];

fn valid_response(id: u64, value: &str) -> Value {
    json!({
        "id": id,
        "ok": true,
        "output": {
            "valid": true,
            "value": value,
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

pub(crate) fn compare_all_fixtures(oracle: &mut TsOracle) {
    let mut next_id = FIRST_REQUEST_ID;
    for fixture in FIXTURES {
        let expected = oracle.request(&fixture.request(next_id));
        let actual = fixture.rust_response(next_id);
        assert_eq!(actual, expected, "protocol error-code mismatch: {}", fixture.name);
        next_id += 1;
    }
    assert_eq!(next_id, FIRST_REQUEST_ID + FIXTURE_COUNT);
}
