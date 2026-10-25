#[path = "support/env_probe.rs"]
mod env_probe;
#[path = "support/oracle.rs"]
mod oracle;

use std::ffi::OsString;

use pi_agent_core::AgentTool;
use pi_coding_agent::core::experimental::are_experimental_features_enabled;
use pi_coding_agent::core::tools::{bash::BashTool, edit::EditTool, write::WriteTool};
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
fn experimental_gate_and_tool_sampling_match_frozen_source() {
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
            "coding-agent-tools-index-oracle.ts",
            "packages/coding-agent/src/core/tools/index.ts",
            &environment,
        );
        let response = source.request(&json!({ "id": 1, "cwd": "/tmp" }));
        assert_eq!(response["ok"], true, "{response}");
        let definitions = response["output"]["definitions"].as_array().unwrap();
        let sampling = ["bash", "edit", "write"].map(|name| {
            definitions
                .iter()
                .find(|definition| definition["name"] == name)
                .unwrap()["constrainedSampling"]
                .clone()
        });
        assert_eq!(sampling, [json!(null), json!(null), json!(null)]);
        let expected = format!("{} {}", value == Some("1"), serde_json::to_string(&sampling).unwrap());
        assert_eq!(probe(value), expected, "PI_EXPERIMENTAL={value:?}");
    }
}

#[test]
#[ignore = "child process probe"]
fn experimental_probe_child() {
    if !env_probe::in_probe() {
        return;
    }
    let sampling = serde_json::to_string(&[
        BashTool::new("/tmp").constrained_sampling(),
        EditTool::new("/tmp").constrained_sampling(),
        WriteTool::new("/tmp").constrained_sampling(),
    ])
    .unwrap();
    println!("{PREFIX}{} {sampling}", are_experimental_features_enabled());
}
