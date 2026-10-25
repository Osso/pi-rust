//! Native public client against the actual unmodified frozen RPC server.
#[path = "support/mode_harness.rs"]
mod mode_harness;

use pi_coding_agent::modes::rpc::{RpcClient, RpcClientOptions, RpcCommandBody};
use pi_js_compat::js_json::{self, JsValue};
use serde::Serialize;
use std::path::{Path, PathBuf};

fn check_wire<T: Serialize>(
    log: &Path,
    command: &str,
    field: Option<&str>,
    result: Result<T, impl std::fmt::Display>,
) -> Result<(), String> {
    let result = result.map_err(|error| format!("{command}: {error}"))?;
    let text = std::fs::read_to_string(log).map_err(|error| error.to_string())?;
    let wire = text
        .lines()
        .rev()
        .filter_map(|line| js_json::parse_lossless(line).ok())
        .find(|value| {
            value.get("type").and_then(JsValue::as_string) == Some(&"response".into())
                && value.get("command").and_then(JsValue::as_string) == Some(&command.into())
        })
        .ok_or_else(|| format!("{command}: no wire response"))?;
    let data = wire
        .get("data")
        .ok_or_else(|| format!("{command}: missing wire data"))?;
    let expected = match field {
        Some(field) => data.get(field).ok_or_else(|| format!("{command}: missing {field}"))?,
        None => data,
    };
    let actual = js_json::to_value(&result).map_err(|error| error.to_string())?;
    if &actual != expected {
        return Err(format!("{command}: decoded {actual:?}, wire {expected:?}"));
    }
    Ok(())
}

fn create_server_options(root: &Path, source: &Path) -> RpcClientOptions {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    for name in ["cwd", "agent", "sessions", "state"] {
        std::fs::create_dir(root.join(name)).unwrap();
    }
    std::fs::write(
        root.join("agent/settings.json"),
        r#"{"compaction":{"enabled":false,"keepRecentTokens":1,"reserveTokens":100},"retry":{"enabled":false}}"#,
    )
    .unwrap();
    let mut options = RpcClientOptions {
        executable_path: Some(source.join("node_modules/.bin/tsx").to_str().unwrap().into()),
        cli_path: Some(repo.join("oracle/rpc-client-server.mts").to_str().unwrap().into()),
        cwd: Some(root.join("cwd").to_str().unwrap().into()),
        node_args: vec![
            "--tsconfig".into(),
            source.join("tsconfig.json").to_str().unwrap().into(),
        ],
        ..Default::default()
    };
    for (name, value) in [
        ("PI_RPC_FROZEN_MODULE",source.join("packages/coding-agent/src/modes/rpc/rpc-mode.ts").display().to_string()),
        ("PI_RPC_WIRE_LOG",root.join("wire.jsonl").display().to_string()),
        ("PI_HARNESS_MODE","rpc".into()),
        ("PI_HARNESS_CWD",root.join("cwd").display().to_string()),
        ("PI_HARNESS_AGENT_DIR",root.join("agent").display().to_string()),
        ("PI_HARNESS_SESSION_DIR",root.join("sessions").display().to_string()),
        ("PI_CODING_AGENT_STATE_DIR",root.join("state").display().to_string()),
        ("PI_HARNESS_RPC_ROOT",root.join("state").display().to_string()),
        ("PI_HARNESS_RESPONSES",r#"[{"text":"first","endTurn":"done","toolId":"first"},{"text":"second","endTurn":"done","toolId":"second"},{"text":"summary"},{"text":"turn prefix summary"}]"#.into()),
        ("PI_HARNESS_RPC_FIXTURE",r#"{"uiProbe":true}"#.into()),
    ] { options.env.insert(name.into(),value.into()); }
    options
}

async fn start_server() -> (tempfile::TempDir, RpcClient, PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let lock = mode_harness::read_source_lock();
    mode_harness::verify_source_head(&lock);
    let client = RpcClient::new(create_server_options(root.path(), &lock.worktree));
    client.start().await.unwrap();
    let log = root.path().join("wire.jsonl");
    (root, client, log)
}

#[tokio::test]
async fn export_path_from_the_actual_frozen_server_retains_unpaired_units() {
    let (root, client, log) = start_server().await;
    client.prompt_and_wait("persist", None, 10_000.0).await.unwrap();
    let path = pi_js_compat::js_json::JsString::from(root.path().to_str().unwrap())
        + "/out-"
        + pi_js_compat::js_json::JsString::from_utf16(vec![0xd800])
        + ".html";
    let result = check_wire(&log, "export_html", None, client.export_html(Some(path)).await);
    client.stop().await.unwrap();
    result.unwrap();
}

#[tokio::test]
async fn every_typed_getter_decodes_actual_frozen_server_responses_without_loss() {
    let mut failures = Vec::new();
    // Main-approved isolation: destructive operations each start from a fresh persisted conversation.
    for scenario in ["read", "new", "compact", "switch", "fork", "clone"] {
        let (root, client, log) = start_server().await;
        macro_rules! check {
            ($command:literal,$field:expr,$call:expr) => {
                if let Err(error) = check_wire(&log, $command, $field, $call.await) {
                    failures.push(format!("{scenario}: {error}"));
                }
            };
        }
        if scenario != "new" {
            client.prompt_and_wait("x".repeat(2000), None, 10_000.0).await.unwrap();
            client.prompt_and_wait("second", None, 10_000.0).await.unwrap();
        }
        match scenario {
            "new" => {
                check!("new_session", None, client.new_session(None));
            }
            "compact" => {
                client.set_auto_compaction(true).await.unwrap();
                check!("compact", None, client.compact(None));
            }
            "switch" => {
                let state = client.send(RpcCommandBody::GetState).await.unwrap();
                let path = state["data"]["sessionFile"].as_string().unwrap().clone();
                check!("switch_session", None, client.switch_session(path));
            }
            "fork" => {
                let entries = client.send(RpcCommandBody::GetEntries { since: None }).await.unwrap();
                let id = entries["data"]["entries"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|entry| entry["message"]["role"].as_string() == Some(&"user".into()))
                    .unwrap()["id"]
                    .as_string()
                    .unwrap()
                    .clone();
                check!("fork", None, client.fork(id));
            }
            "clone" => {
                check!("clone", None, client.clone_session());
            }
            "read" => {
                check!("get_state", None, client.get_state());
                check!("set_model", None, client.set_model("faux", "faux-1"));
                check!("cycle_model", None, client.cycle_model());
                check!("get_available_models", Some("models"), client.get_available_models());
                check!("cycle_thinking_level", None, client.cycle_thinking_level());
                check!("get_session_stats", None, client.get_session_stats());
                check!("bash", None, client.bash("printf private"));
                check!("get_messages", Some("messages"), client.get_messages());
                check!("get_fork_messages", Some("messages"), client.get_fork_messages());
                check!("get_entries", None, client.get_entries(None));
                check!("get_tree", None, client.get_tree());
                check!(
                    "get_last_assistant_text",
                    Some("text"),
                    client.get_last_assistant_text()
                );
                check!("get_commands", Some("commands"), client.get_commands());
                check!(
                    "export_html",
                    None,
                    client.export_html(Some(root.path().join("out.html").to_str().unwrap().into()))
                );
            }
            _ => unreachable!(),
        }
        client.stop().await.unwrap();
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
