use pi_protocol::{SessionMetadata, SessionMetadataInput};
use serde_json::{Value, json};

use super::oracle::TsOracle;

const FIRST_REQUEST_ID: u64 = 6000;
const FIXTURE_COUNT: u64 = 13;
const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

struct Fixture {
    name: &'static str,
    id_value: &'static str,
    created_at: u64,
    updated_at: Option<u64>,
    parent_session_id: Option<&'static str>,
    session_name: Option<&'static str>,
    cwd: Option<&'static str>,
}

impl Fixture {
    const fn new(
        name: &'static str,
        id_value: &'static str,
        created_at: u64,
        updated_at: Option<u64>,
        parent_session_id: Option<&'static str>,
        session_name: Option<&'static str>,
        cwd: Option<&'static str>,
    ) -> Self {
        Self {
            name,
            id_value,
            created_at,
            updated_at,
            parent_session_id,
            session_name,
            cwd,
        }
    }

    fn request(&self, id: u64) -> Value {
        let mut value = json!({
            "id": self.id_value,
            "createdAt": self.created_at,
        });
        add_optional(&mut value, "updatedAt", self.updated_at);
        add_optional_string(&mut value, "parentSessionId", self.parent_session_id);
        add_optional_string(&mut value, "sessionName", self.session_name);
        add_optional_string(&mut value, "cwd", self.cwd);
        json!({
            "id": id,
            "operation": "validate_session_metadata",
            "value": value,
        })
    }

    fn rust_response(&self, id: u64) -> Value {
        let input = SessionMetadataInput {
            id: self.id_value.to_owned(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            parent_session_id: self.parent_session_id.map(str::to_owned),
            session_name: self.session_name.map(str::to_owned),
            cwd: self.cwd.map(str::to_owned),
        };
        let Ok(metadata) = SessionMetadata::new(input) else {
            return invalid_response(id);
        };
        valid_response(id, &metadata)
    }
}

fn add_optional(value: &mut Value, key: &str, number: Option<u64>) {
    if let Some(number) = number {
        value[key] = json!(number);
    }
}

fn add_optional_string(value: &mut Value, key: &str, text: Option<&str>) {
    if let Some(text) = text {
        value[key] = json!(text);
    }
}

fn valid_response(id: u64, metadata: &SessionMetadata) -> Value {
    let mut value = json!({
        "id": metadata.id(),
        "createdAt": metadata.created_at(),
    });
    add_optional(&mut value, "updatedAt", metadata.updated_at());
    add_optional_string(&mut value, "parentSessionId", metadata.parent_session_id());
    add_optional_string(&mut value, "sessionName", metadata.session_name());
    add_optional_string(&mut value, "cwd", metadata.cwd());
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

const VALID_FIXTURES: &[Fixture] = &[
    Fixture::new("required only", "session-1", 0, None, None, None, None),
    Fixture::new(
        "all optionals",
        "session-2",
        10,
        Some(20),
        Some("parent-1"),
        Some("Named session"),
        Some("/tmp/project"),
    ),
    Fixture::new("updated at zero", "session-3", 1, Some(0), None, None, None),
    Fixture::new("both timestamps one", "session-4", 1, Some(1), None, None, None),
    Fixture::new(
        "maximum safe timestamps",
        "session-5",
        MAX_SAFE_INTEGER,
        Some(MAX_SAFE_INTEGER),
        None,
        None,
        None,
    ),
    Fixture::new("parent only", "session-6", 2, None, Some("parent-6"), None, None),
    Fixture::new(
        "explicit empty session name",
        "session-7",
        3,
        None,
        None,
        Some(""),
        None,
    ),
    Fixture::new("cwd only", "session-8", 4, None, None, None, Some("/workspace")),
    Fixture::new(
        "Unicode combining supplementary NUL",
        "e\u{301}\0𐅑",
        5,
        Some(6),
        Some("parent\0e\u{301}𐅑"),
        Some("name\0e\u{301}𐅑"),
        Some("cwd\0e\u{301}𐅑"),
    ),
    Fixture::new(
        "whitespace non-empty fields",
        " \t",
        7,
        Some(8),
        Some(" \n"),
        Some(" \t"),
        Some(" \r"),
    ),
];

const INVALID_FIXTURES: &[Fixture] = &[
    Fixture::new("empty id", "", 1, None, None, None, None),
    Fixture::new("empty parent", "session-11", 1, None, Some(""), None, None),
    Fixture::new("empty cwd", "session-12", 1, None, None, None, Some("")),
];

pub(crate) fn compare_all_fixtures(oracle: &mut TsOracle) {
    let fixtures = VALID_FIXTURES.iter().chain(INVALID_FIXTURES);
    let mut next_id = FIRST_REQUEST_ID;
    for fixture in fixtures {
        let expected = oracle.request(&fixture.request(next_id));
        let actual = fixture.rust_response(next_id);
        assert_eq!(actual, expected, "session-metadata mismatch: {}", fixture.name);
        next_id += 1;
    }
    assert_eq!(next_id, FIRST_REQUEST_ID + FIXTURE_COUNT);
}
