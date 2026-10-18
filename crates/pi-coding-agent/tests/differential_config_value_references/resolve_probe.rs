//! Child-process probe for the resolve cases that read the process environment.

use std::collections::HashMap;
use std::ffi::OsString;
use std::process::Command;

use pi_coding_agent::core::resolve_config_value;
use serde_json::{Value, json};

pub(crate) const OUTPUT_PREFIX: &str = "PI_RUST_RESOLVE_OUTPUT:";
const MODE_ENV: &str = "PI_RUST_RESOLVE_MODE";
const PROCESS: &str = "PI_DIFF_RESOLVE_PROCESS";

pub(crate) fn spawn(mode: &str, env_pairs: &[(&str, OsString)]) -> Value {
    let executable = std::env::current_exe().expect("resolve test executable must be discoverable");
    let mut command = Command::new(executable);
    command.env_clear().env(MODE_ENV, mode).args([
        "--ignored",
        "--exact",
        "config_value_reference_oracle::resolve_process_probe_child",
        "--nocapture",
        "--test-threads=1",
    ]);
    for (name, value) in env_pairs {
        command.env(name, value);
    }
    let output = command.output().expect("resolve child probe must spawn");
    assert!(
        output.status.success(),
        "resolve child probe failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("child output must be UTF-8");
    let payload = stdout
        .lines()
        .find_map(|line| {
            line.find(OUTPUT_PREFIX)
                .map(|index| &line[index + OUTPUT_PREFIX.len()..])
        })
        .expect("resolve child must emit a prefixed JSON value");
    serde_json::from_str(payload).expect("resolve child payload must be JSON")
}

pub(crate) fn run_child() {
    let mode = std::env::var(MODE_ENV).expect("resolve child mode must be set");
    let value = match mode.as_str() {
        "process_only" => resolve_config_value(&format!("${PROCESS}"), None),
        "explicit_beats_process" => resolve_config_value(&format!("${PROCESS}"), Some(&map(&[(PROCESS, "explicit")]))),
        "empty_explicit_falls_back" => resolve_config_value(&format!("${PROCESS}"), Some(&map(&[(PROCESS, "")]))),
        other => panic!("unknown resolve probe mode {other}"),
    };
    println!("{OUTPUT_PREFIX}{}", json!({"value": value}));
}

fn map(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect()
}
