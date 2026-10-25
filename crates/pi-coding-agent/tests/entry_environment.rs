#[path = "support/env_probe.rs"]
mod env_probe;
use crate::shared_oracle as oracle;

use serde_json::json;
use std::ffi::OsString;

#[test]
fn entrypoints_preserve_inherited_ai_agent_like_frozen() {
    for (mode, source) in [("cli", "cli.ts"), ("rpc", "rpc-entry.ts")] {
        for inherited in [None, Some(""), Some("other-agent")] {
            let mut environment = vec![("PI_ENTRY_MODE", OsString::from(mode))];
            if let Some(value) = inherited {
                environment.push(("AI_AGENT", OsString::from(value)));
            }
            let mut source = oracle::TsOracle::spawn_with_environment(
                "entry-environment-oracle.ts",
                &format!("packages/coding-agent/src/{source}"),
                &environment,
            );
            let expected = source.request(&json!({"id": 1}));
            assert_eq!(expected["ok"], true, "{expected}");
            let child = env_probe::run_probe("entry_environment_child", &environment);
            let stdout = String::from_utf8(child.stdout).unwrap();
            let actual = stdout
                .lines()
                .find_map(|line| line.split_once("ENTRY_ENV:").map(|(_, value)| value))
                .unwrap();
            let actual: serde_json::Value = serde_json::from_str(actual).unwrap();
            assert_eq!(actual, expected["output"], "{mode}, AI_AGENT={inherited:?}");
        }
    }
}

#[tokio::test]
#[ignore = "child process probe"]
async fn entry_environment_child() {
    if !env_probe::in_probe() {
        return;
    }
    let mode = std::env::var("PI_ENTRY_MODE").unwrap();
    if mode == "rpc" {
        let main: pi_coding_agent::rpc_entry::RpcMain<'_> = Box::new(|_| Box::pin(async { Ok(()) }));
        pi_coding_agent::rpc_entry::run_rpc_entry(&[], main).await.unwrap();
    } else {
        let main: pi_coding_agent::cli_entry::CliMain<'_> = Box::new(|_| Box::pin(async { Ok(0) }));
        pi_coding_agent::cli_entry::run_cli_main(&[], main).await.unwrap();
    }
    println!(
        "ENTRY_ENV:{}",
        json!([
            pi_js_compat::process_env::get("PI_CODING_AGENT"),
            pi_js_compat::process_env::get("AI_AGENT"),
        ])
    );
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
