use std::collections::HashMap;
use std::ffi::OsString;
use std::process::{Command, Output};

use pi_coding_agent::core::{get_missing_config_value_env_var_names, is_config_value_configured};
use serde_json::{Value, json};

pub(crate) const OUTPUT_PREFIX: &str = "PI_RUST_CONFIG_STATUS_OUTPUT:";
const MODE_ENV: &str = "PI_RUST_CONFIG_STATUS_MODE";
pub(crate) const ENV_A: &str = "PI_RUST_CONFIG_STATUS_A_7D9F";
pub(crate) const ENV_B: &str = "PI_RUST_CONFIG_STATUS_B_7D9F";
pub(crate) const ENV_C: &str = "PI_RUST_CONFIG_STATUS_C_7D9F";
pub(crate) const ENV_LOWERCASE_A: &str = "pi_rust_config_status_a_7d9f";

pub(crate) fn normalized_status(config: &str, env: Option<&HashMap<String, String>>) -> Value {
    json!({
        "missing": get_missing_config_value_env_var_names(config, env),
        "configured": is_config_value_configured(config, env),
    })
}

pub(crate) fn spawn_child_probe(mode: &str, env_pairs: &[(&str, OsString)]) -> Value {
    let output = child_probe_command(mode, env_pairs)
        .output()
        .expect("config status child probe must spawn");
    parse_child_probe_output(output)
}

fn child_probe_command(mode: &str, env_pairs: &[(&str, OsString)]) -> Command {
    let executable = std::env::current_exe().expect("config status test executable must be discoverable");
    assert!(
        executable.is_absolute(),
        "config status test executable must be absolute"
    );

    let mut command = Command::new(executable);
    command.env_clear().env(MODE_ENV, mode).args([
        "--ignored",
        "--exact",
        "config_status_process_probe_child",
        "--nocapture",
        "--test-threads=1",
    ]);
    for (name, value) in env_pairs {
        command.env(name, value);
    }
    command
}

fn parse_child_probe_output(output: Output) -> Value {
    assert!(
        output.status.success(),
        "config status child probe failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("config status child output must be UTF-8");
    let payloads = stdout
        .lines()
        .filter_map(|line| {
            line.find(OUTPUT_PREFIX)
                .map(|index| &line[index + OUTPUT_PREFIX.len()..])
        })
        .collect::<Vec<_>>();
    assert_eq!(
        payloads.len(),
        1,
        "config status child must emit one prefixed JSON value"
    );

    serde_json::from_str(payloads[0]).expect("config status child prefixed output must be JSON")
}

pub(crate) fn run_child_probe() {
    let mode = std::env::var(MODE_ENV).expect("config status child mode must be set");
    let status = match mode.as_str() {
        "missing" | "process_empty" | "process_ascii" | "process_unicode" => {
            normalized_status("$PI_RUST_CONFIG_STATUS_A_7D9F", None)
        }
        "explicit_empty_fallback" => {
            let env = map_from([(ENV_A, "")]);
            normalized_status("$PI_RUST_CONFIG_STATUS_A_7D9F", Some(&env))
        }
        "mixed" => {
            let env = map_from([(ENV_B, "explicit-b")]);
            normalized_status(
                "$PI_RUST_CONFIG_STATUS_B_7D9F-$PI_RUST_CONFIG_STATUS_A_7D9F-$PI_RUST_CONFIG_STATUS_C_7D9F-$PI_RUST_CONFIG_STATUS_A_7D9F",
                Some(&env),
            )
        }
        "explicit_case" => {
            let env = map_from([(ENV_A, "uppercase")]);
            normalized_status(
                "$PI_RUST_CONFIG_STATUS_A_7D9F-$pi_rust_config_status_a_7d9f",
                Some(&env),
            )
        }
        #[cfg(unix)]
        "process_nonunicode" => normalized_status("$PI_RUST_CONFIG_STATUS_A_7D9F", None),
        _ => panic!("unknown config status child mode: {mode}"),
    };

    println!("{OUTPUT_PREFIX}{status}");
}

fn map_from<const N: usize>(entries: [(&str, &str); N]) -> HashMap<String, String> {
    entries
        .into_iter()
        .map(|(name, value)| (name.to_owned(), value.to_owned()))
        .collect()
}
