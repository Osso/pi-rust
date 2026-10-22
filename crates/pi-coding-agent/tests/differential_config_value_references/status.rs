use std::collections::HashMap;
use std::ffi::OsString;

use serde_json::{Map, Value, json};

use super::fixtures::Fixture;
use super::oracle::TsOracle;
use crate::config_value_status_probe::{ENV_A, ENV_B, ENV_C, ENV_LOWERCASE_A, normalized_status, spawn_child_probe};

const FIRST_STATUS_ID: u64 = 12_069;
const LAST_STATUS_ID: u64 = 12_082;
const NON_UNICODE_STATUS_ID: u64 = 12_083;
const SINGLE_A_CONFIG: &str = "$PI_RUST_CONFIG_STATUS_A_7D9F";
const ALL_NAMES_CONFIG: &str =
    "$PI_RUST_CONFIG_STATUS_A_7D9F-$PI_RUST_CONFIG_STATUS_B_7D9F-$PI_RUST_CONFIG_STATUS_C_7D9F";

pub(crate) fn run(oracle: &mut TsOracle) {
    let fixtures = all();
    assert_status_fixture_ids(&fixtures);

    for fixture in fixtures {
        let expected = oracle.request(&fixture.request);
        assert_eq!(
            expected, fixture.expected,
            "config value status mismatch: {}",
            fixture.name
        );
    }
}

#[cfg(unix)]
pub(crate) fn run_non_unicode_status() {
    use std::os::unix::ffi::OsStringExt;

    let invalid_value = OsString::from_vec(vec![0xff]);
    let env_pairs = vec![(ENV_A, invalid_value.clone())];
    let mut oracle = TsOracle::spawn_with_environment(
        "config-value-reference-oracle.ts",
        "packages/coding-agent/src/core/resolve-config-value.ts",
        &env_pairs,
    );
    let request = json!({
        "id": NON_UNICODE_STATUS_ID,
        "operation": "status",
        "config": SINGLE_A_CONFIG,
        "env": Value::Null,
    });
    let expected_status = spawn_child_probe("process_nonunicode", &env_pairs);
    let expected = status_response(NON_UNICODE_STATUS_ID, expected_status);

    assert_eq!(oracle.request(&request), expected, "non-Unicode process environment");
}

fn all() -> Vec<Fixture> {
    [
        basic_status_fixtures(),
        fallback_status_fixtures(),
        ordering_status_fixtures(),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn basic_status_fixtures() -> Vec<Fixture> {
    vec![
        literal_fixture(),
        command_fixture(),
        missing_fixture(),
        explicit_nonempty_fixture(),
    ]
}

fn fallback_status_fixtures() -> Vec<Fixture> {
    vec![
        explicit_empty_missing_fixture(),
        explicit_empty_fallback_fixture(),
        process_empty_fixture(),
        process_ascii_fixture(),
        process_unicode_fixture(),
    ]
}

fn ordering_status_fixtures() -> Vec<Fixture> {
    vec![
        mixed_fixture(),
        explicit_case_fixture(),
        duplicate_missing_fixture(),
        escaped_literals_fixture(),
        leading_space_command_fixture(),
    ]
}

fn literal_fixture() -> Fixture {
    direct_fixture(
        12_069,
        "literal",
        "literal",
        Value::Null,
        cleared_process_environment(),
        normalized_status("literal", None),
    )
}

fn command_fixture() -> Fixture {
    direct_fixture(
        12_070,
        "command",
        "!echo $A",
        Value::Null,
        cleared_process_environment(),
        normalized_status("!echo $A", None),
    )
}

fn missing_fixture() -> Fixture {
    child_fixture(
        12_071,
        "missing",
        SINGLE_A_CONFIG,
        Value::Null,
        cleared_process_environment(),
        "missing",
        Vec::new(),
    )
}

fn explicit_nonempty_fixture() -> Fixture {
    let entries = [(ENV_A, "0"), (ENV_B, " \t"), (ENV_C, "é世界🙂\0")];
    direct_fixture(
        12_072,
        "explicit nonempty values",
        ALL_NAMES_CONFIG,
        string_environment(&entries),
        cleared_process_environment(),
        explicit_status(ALL_NAMES_CONFIG, &entries),
    )
}

fn explicit_empty_missing_fixture() -> Fixture {
    child_fixture(
        12_073,
        "explicit empty missing",
        SINGLE_A_CONFIG,
        string_environment(&[(ENV_A, "")]),
        cleared_process_environment(),
        "explicit_empty_missing",
        Vec::new(),
    )
}

fn explicit_empty_fallback_fixture() -> Fixture {
    child_fixture(
        12_074,
        "explicit empty fallback",
        SINGLE_A_CONFIG,
        string_environment(&[(ENV_A, "")]),
        process_environment(Some("process-value"), None, None, None),
        "explicit_empty_fallback",
        vec![(ENV_A, OsString::from("process-value"))],
    )
}

fn process_empty_fixture() -> Fixture {
    process_value_fixture(12_075, "process empty", "process_empty", "")
}

fn process_ascii_fixture() -> Fixture {
    process_value_fixture(12_076, "process ASCII", "process_ascii", "value")
}

fn process_unicode_fixture() -> Fixture {
    process_value_fixture(12_077, "process Unicode", "process_unicode", "é世界🙂")
}

fn process_value_fixture(id: u64, name: &'static str, mode: &str, value: &str) -> Fixture {
    child_fixture(
        id,
        name,
        SINGLE_A_CONFIG,
        Value::Null,
        process_environment(Some(value), None, None, None),
        mode,
        vec![(ENV_A, OsString::from(value))],
    )
}

fn mixed_fixture() -> Fixture {
    child_fixture(
        12_078,
        "mixed explicit and process",
        "$PI_RUST_CONFIG_STATUS_B_7D9F-$PI_RUST_CONFIG_STATUS_A_7D9F-$PI_RUST_CONFIG_STATUS_C_7D9F-$PI_RUST_CONFIG_STATUS_A_7D9F",
        string_environment(&[(ENV_B, "explicit-b")]),
        process_environment(Some("process-a"), None, None, None),
        "mixed",
        vec![(ENV_A, OsString::from("process-a"))],
    )
}

fn explicit_case_fixture() -> Fixture {
    child_fixture(
        12_079,
        "explicit case",
        "$PI_RUST_CONFIG_STATUS_A_7D9F-$pi_rust_config_status_a_7d9f",
        string_environment(&[(ENV_A, "uppercase")]),
        cleared_process_environment(),
        "explicit_case",
        Vec::new(),
    )
}

fn duplicate_missing_fixture() -> Fixture {
    child_fixture(
        12_080,
        "duplicate missing",
        "$PI_RUST_CONFIG_STATUS_B_7D9F-$PI_RUST_CONFIG_STATUS_A_7D9F-$PI_RUST_CONFIG_STATUS_B_7D9F-${PI_RUST_CONFIG_STATUS_A_7D9F}",
        Value::Null,
        cleared_process_environment(),
        "duplicate_missing",
        Vec::new(),
    )
}

fn escaped_literals_fixture() -> Fixture {
    direct_fixture(
        12_081,
        "escaped literals",
        "$$A-$!B",
        Value::Null,
        cleared_process_environment(),
        normalized_status("$$A-$!B", None),
    )
}

fn leading_space_command_fixture() -> Fixture {
    child_fixture(
        12_082,
        "leading-space command",
        " !echo $A",
        Value::Null,
        cleared_process_environment(),
        "leading_space_command",
        Vec::new(),
    )
}

fn assert_status_fixture_ids(fixtures: &[Fixture]) {
    assert_eq!(fixtures.len(), 14, "status fixture count must remain stable");
    let ids = fixtures
        .iter()
        .map(|fixture| fixture.request["id"].as_u64().expect("status fixture ID must be u64"))
        .collect::<Vec<_>>();
    let expected = (FIRST_STATUS_ID..=LAST_STATUS_ID).collect::<Vec<_>>();
    assert_eq!(ids, expected, "status fixture IDs must be contiguous");
}

fn direct_fixture(
    id: u64,
    name: &'static str,
    config: &'static str,
    env: Value,
    process_env: Value,
    expected_status: Value,
) -> Fixture {
    Fixture {
        name,
        request: status_request(id, config, env, process_env),
        expected: status_response(id, expected_status),
    }
}

fn child_fixture(
    id: u64,
    name: &'static str,
    config: &'static str,
    env: Value,
    process_env: Value,
    mode: &str,
    env_pairs: Vec<(&str, OsString)>,
) -> Fixture {
    let expected_status = spawn_child_probe(mode, &env_pairs);
    direct_fixture(id, name, config, env, process_env, expected_status)
}

fn status_request(id: u64, config: &str, env: Value, process_env: Value) -> Value {
    json!({
        "id": id,
        "operation": "status",
        "config": config,
        "env": env,
        "processEnv": process_env,
    })
}

fn status_response(id: u64, status: Value) -> Value {
    json!({"id": id, "ok": true, "output": status})
}

fn explicit_status(config: &str, entries: &[(&str, &str)]) -> Value {
    let values = entries
        .iter()
        .map(|(name, value)| ((*name).to_owned(), pi_js_compat::js_json::JsString::from(*value)))
        .collect::<HashMap<_, _>>();
    normalized_status(config, Some(&values))
}

fn string_environment(entries: &[(&str, &str)]) -> Value {
    let mut values = Map::new();
    for (name, value) in entries {
        values.insert((*name).to_owned(), Value::String((*value).to_owned()));
    }
    Value::Object(values)
}

fn cleared_process_environment() -> Value {
    process_environment(None, None, None, None)
}

fn process_environment(
    value_a: Option<&str>,
    value_b: Option<&str>,
    value_c: Option<&str>,
    value_lowercase_a: Option<&str>,
) -> Value {
    let mut values = Map::new();
    insert_optional(&mut values, ENV_A, value_a);
    insert_optional(&mut values, ENV_B, value_b);
    insert_optional(&mut values, ENV_C, value_c);
    insert_optional(&mut values, ENV_LOWERCASE_A, value_lowercase_a);
    Value::Object(values)
}

fn insert_optional(values: &mut Map<String, Value>, name: &str, value: Option<&str>) {
    let value = value.map_or(Value::Null, |value| Value::String(value.to_owned()));
    values.insert(name.to_owned(), value);
}
