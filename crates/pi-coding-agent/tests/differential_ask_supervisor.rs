#[path = "support/advisory_fixture.rs"]
mod advisory_fixture;
use crate::shared_oracle as oracle;
use crate::shared_tool_context as tool_context;

#[test]
fn durable_advisory_roundtrip_uses_only_private_kb_and_resident_fixtures() {
    advisory_fixture::run_isolated();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "private KB namespace probe invoked by the outer behavioral test"]
async fn isolated_advisory_probe() {
    advisory_fixture::run_probe().await;
}

use pi_coding_agent::core::extensions::types::ContextTool;
use pi_coding_agent::core::tools::ask_supervisor::AskSupervisorTool;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::JsValue;

#[tokio::test]
async fn loaded_empty_session_id_preserves_control_path_then_sender_guard_order() {
    use pi_coding_agent::core::session_manager::SessionManager;
    use std::sync::{Arc, Mutex};
    let root = tempfile::tempdir().unwrap();
    let cwd = pi_js_compat::node_fs::decode_node_path(root.path().as_os_str());
    let file = cwd.clone() + "/empty-id.jsonl";
    let header = json!({"type":"session","version":3,"id":"","timestamp":"2026-01-01T00:00:00.000Z","cwd":cwd});
    std::fs::write(
        pi_js_compat::js_json::encode_node_utf8(&file),
        pi_js_compat::js_json::stringify_lossless(&header) + "\n",
    )
    .unwrap();
    let mut oracle = oracle::TsOracle::spawn("coding-agent-session-tools-oracle.ts", "packages/coding-agent/src");
    let mut failures = Vec::new();
    for (path, message) in [
        (
            cwd.clone() + "/unused.sqlite",
            "ask_supervisor requires a persisted session id",
        ),
        ("".into(), "ask_supervisor requires a control database path"),
    ] {
        // Frozen session-manager.ts:1376 loads id verbatim; ask-supervisor.ts:42-45 orders these guards.
        let expected = oracle.request(&json!({"id":1,"operation":"tool:execute","module":"core/tools/ask-supervisor.ts","factory":"createAskSupervisorToolDefinition","params":{"question":"ignored"},"cwd":cwd,"sessionFile":file,"sessionDir":cwd,"controlDbPath":path}));
        assert_eq!(expected["output"]["error"], json!(message));
        let manager = SessionManager::open(&file, Some(&cwd), None).unwrap();
        assert!(
            manager.get_session_id().is_empty(),
            "loaded native header must reach the sender guard"
        );
        let (_runner, context) = tool_context::TestContextBuilder::new(cwd.clone(), Arc::new(Mutex::new(manager)))
            .control_db_path(path)
            .build()
            .await;
        let error = AskSupervisorTool::new()
            .execute(
                "guard".into(),
                json!({"question":"ignored"}),
                None,
                None,
                Some(context),
                None,
            )
            .await
            .unwrap_err();
        let actual = json!({"ok":false,"error":error.message});
        if actual != expected["output"] {
            failures.push(format!(
                "{} != {}",
                pi_js_compat::js_json::stringify_lossless(&actual),
                pi_js_compat::js_json::stringify_lossless(&expected["output"])
            ));
        }
    }
    assert!(!root.path().join("unused.sqlite").exists());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn early_guards_match_frozen_without_project_or_service_access() {
    let root = tempfile::tempdir().unwrap();
    let cases = [
        (
            "standalone",
            true,
            Some("worker"),
            false,
            "Supervisor advisory is unavailable in standalone worker mode",
        ),
        (
            "child",
            false,
            Some("worker"),
            false,
            "ask_supervisor is only available from main sessions",
        ),
        (
            "required",
            false,
            None,
            true,
            "ask_supervisor is only available from main sessions",
        ),
        (
            "empty-agent",
            false,
            Some(""),
            false,
            "ask_supervisor requires a control database path",
        ),
        (
            "missing-db",
            false,
            None,
            false,
            "ask_supervisor requires a control database path",
        ),
    ];
    let mut oracle = oracle::TsOracle::spawn("coding-agent-session-tools-oracle.ts", "packages/coding-agent/src");
    let mut failures = Vec::new();
    for (index, (name, disabled, agent, required, message)) in cases.into_iter().enumerate() {
        // Frozen ask-supervisor.ts:40-46: every case stops before hardcoded KB/project resolution.
        let expected = oracle.request(&json!({
            "id": index, "operation": "tool:execute", "module": "core/tools/ask-supervisor.ts",
            "factory": "createAskSupervisorToolDefinition", "params": {"question": "ignored"},
            "cwd": "/repo", "noSupervisor": disabled, "multiAgentAgentId": agent,
            "multiAgentRequiresAgentId": required, "sessionId": "main",
        }));
        assert_eq!(expected["output"]["error"], json!(message));
        let session = tool_context::in_memory_session("/repo", "main");
        let mut builder = tool_context::TestContextBuilder::new("/repo", session)
            .no_supervisor(disabled)
            .requires_agent_id(required);
        if let Some(agent) = agent {
            builder = builder.agent_id(agent);
        }
        let (_runner, context) = builder.build().await;
        let result = AskSupervisorTool::new()
            .execute(
                "guard".into(),
                json!({"question": "ignored"}),
                None,
                None,
                Some(context),
                None,
            )
            .await;
        let actual: JsValue = match result {
            Ok(result) => json!({"ok": true, "result": result}),
            Err(error) => json!({"ok": false, "error": error.message}),
        };
        if actual != expected["output"] {
            failures.push(format!(
                "{name}: {} != {}",
                pi_js_compat::js_json::stringify_lossless(&actual),
                pi_js_compat::js_json::stringify_lossless(&expected["output"])
            ));
        }
    }
    assert!(
        !root.path().join("unused.sqlite").exists(),
        "guard failures must not open the DB"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
#[path = "support/tool_context.rs"]
pub mod shared_tool_context;
