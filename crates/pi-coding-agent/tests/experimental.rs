#[path = "support/env_probe.rs"]
mod env_probe;
use crate::shared_oracle as oracle;

use std::ffi::OsString;

use pi_coding_agent::core::experimental::are_experimental_features_enabled;
use serde_json::json;

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
fn experimental_gate_matches_frozen_source() {
    for value in [
        None,
        Some(""),
        Some("0"),
        Some("true"),
        Some("1"),
        Some(" 1"),
        Some("01"),
    ] {
        let environment = [("PI_EXPERIMENTAL", OsString::from(value.unwrap_or_default()))];
        let mut source = oracle::TsOracle::spawn_with_environment(
            "experimental-oracle.mts",
            "packages/coding-agent/src/core/experimental.ts",
            &environment,
        );
        let response = source.request(&json!({ "id": 1, "cwd": "/tmp" }));
        assert_eq!(response["ok"], true, "{response}");
        let expected = response["output"].as_bool().unwrap().to_string();
        assert_eq!(probe(value), expected, "PI_EXPERIMENTAL={value:?}");
    }
}

#[test]
#[ignore = "child process probe"]
fn experimental_probe_child() {
    if !env_probe::in_probe() {
        return;
    }
    println!("{PREFIX}{}", are_experimental_features_enabled());
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
