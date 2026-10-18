#[path = "support/env_probe.rs"]
mod env_probe;

use std::ffi::OsString;

use pi_coding_agent::core::experimental::{are_experimental_features_enabled, get_experimental_tool_sampling};

const PREFIX: &str = "EXPERIMENTAL_PROBE:";

fn probe(value: Option<&str>) -> String {
    let env: Vec<(&str, OsString)> = value
        .map(|value| vec![("PI_EXPERIMENTAL", OsString::from(value))])
        .unwrap_or_default();
    let output = env_probe::run_probe("experimental_probe_child", &env);
    let stdout = String::from_utf8(output.stdout).unwrap();
    let lines: Vec<&str> = stdout
        .lines()
        .filter_map(|line| line.split_once(PREFIX).map(|(_, rest)| rest))
        .collect();
    assert_eq!(lines.len(), 1, "probe must print one line: {stdout}");
    lines[0].to_owned()
}

#[test]
fn returns_false_when_pi_experimental_is_unset() {
    assert_eq!(probe(None), "false null");
}

#[test]
fn returns_false_when_pi_experimental_is_empty() {
    assert_eq!(probe(Some("")), "false null");
}

#[test]
fn returns_true_when_pi_experimental_is_set_to_1() {
    assert_eq!(probe(Some("1")), r#"true {"type":"json_schema","strict":"prefer"}"#);
}

#[test]
fn returns_false_when_pi_experimental_is_set_to_0() {
    assert_eq!(probe(Some("0")), "false null");
}

#[test]
fn returns_false_when_pi_experimental_is_set_to_a_non_1_value() {
    assert_eq!(probe(Some("true")), "false null");
}

#[test]
#[ignore = "child process probe"]
fn experimental_probe_child() {
    if !env_probe::in_probe() {
        return;
    }
    let sampling = serde_json::to_string(&get_experimental_tool_sampling()).unwrap();
    println!("{PREFIX}{} {sampling}", are_experimental_features_enabled());
}
