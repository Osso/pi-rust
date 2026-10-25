//! Native public client against the actual unmodified frozen RPC server.
#[path = "support/mode_harness.rs"]
mod mode_harness;

use pi_coding_agent::modes::rpc::{RpcClient, RpcClientOptions, RpcCommandBody};
use pi_js_compat::js_json::{self, JsValue};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::Duration;

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

#[tokio::test]
async fn every_typed_getter_decodes_actual_frozen_server_responses_without_loss() {
    let root = tempfile::tempdir().unwrap();
    let lock = mode_harness::read_source_lock();
    mode_harness::verify_source_head(&lock);
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    for name in ["cwd", "agent", "sessions", "state"] {
        std::fs::create_dir(root.path().join(name)).unwrap();
    }
    std::fs::write(
        root.path().join("agent/settings.json"),
        r#"{"compaction":{"enabled":false,"keepRecentTokens":1,"reserveTokens":100},"retry":{"enabled":false}}"#,
    )
    .unwrap();
    let log = root.path().join("wire.jsonl");
    let mut options = RpcClientOptions {
        executable_path: Some(lock.worktree.join("node_modules/.bin/tsx").to_str().unwrap().into()),
        cli_path: Some(repo.join("oracle/rpc-client-server.mts")),
        cwd: Some(root.path().join("cwd")),
        node_args: vec![
            "--tsconfig".into(),
            lock.worktree.join("tsconfig.json").to_str().unwrap().into(),
        ],
        ..Default::default()
    };
    for (name, value) in [
        ("PI_RPC_FROZEN_MODULE", lock.worktree.join("packages/coding-agent/src/modes/rpc/rpc-mode.ts").display().to_string()),
        ("PI_RPC_WIRE_LOG", log.display().to_string()),
        ("PI_HARNESS_MODE", "rpc".into()),
        ("PI_HARNESS_CWD", root.path().join("cwd").display().to_string()),
        ("PI_HARNESS_AGENT_DIR", root.path().join("agent").display().to_string()),
        ("PI_HARNESS_SESSION_DIR", root.path().join("sessions").display().to_string()),
        ("PI_CODING_AGENT_STATE_DIR", root.path().join("state").display().to_string()),
        ("PI_HARNESS_RPC_ROOT", root.path().join("state").display().to_string()),
        ("PI_HARNESS_RESPONSES", r#"[{"text":"first","endTurn":"done","toolId":"first"},{"text":"second","endTurn":"done","toolId":"second"},{"text":"summary"}]"#.into()),
        ("PI_HARNESS_RPC_FIXTURE", r#"{"uiProbe":true}"#.into()),
    ] { options.env.insert(name.into(),value); }
    let client = RpcClient::new(options);
    client.start().await.unwrap();
    let mut failures = Vec::new();
    macro_rules! check {
        ($command:literal, $field:expr, $call:expr) => {
            if let Err(error) = check_wire(&log, $command, $field, $call.await) {
                failures.push(error);
            }
        };
    }
    check!("new_session", None, client.new_session(None));
    check!("get_state", None, client.get_state());
    check!("set_model", None, client.set_model("faux", "faux-1"));
    check!("cycle_model", None, client.cycle_model());
    check!("get_available_models", Some("models"), client.get_available_models());
    check!("cycle_thinking_level", None, client.cycle_thinking_level());
    client
        .prompt_and_wait("x".repeat(2000), None, Duration::from_secs(10))
        .await
        .unwrap();
    client
        .prompt_and_wait("second", None, Duration::from_secs(10))
        .await
        .unwrap();
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
    client.set_auto_compaction(true).await.unwrap();
    check!("compact", None, client.compact(None));
    let state = client.send(RpcCommandBody::GetState).await.unwrap();
    let session_path = state["data"]["sessionFile"].as_string().unwrap().clone();
    check!("switch_session", None, client.switch_session(session_path));
    check!("clone", None, client.clone_session());
    let entries = client.send(RpcCommandBody::GetEntries { since: None }).await.unwrap();
    let user_id = entries["data"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["message"]["role"].as_string() == Some(&"user".into()))
        .unwrap()["id"]
        .as_string()
        .unwrap()
        .clone();
    check!("fork", None, client.fork(user_id));
    client.stop().await.unwrap();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
