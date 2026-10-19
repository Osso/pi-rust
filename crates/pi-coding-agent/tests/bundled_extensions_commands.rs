//! Differential and behavioral coverage for the small command/tool bundled extensions.

#[path = "support/bundled_extension_commands.rs"]
mod command_support;
#[path = "support/bundled_extension_load.rs"]
mod load_support;
#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/bundled_extensions.rs"]
mod support;
#[path = "support/bundled_extension_tool.rs"]
mod tool_support;

use std::sync::{Arc, Mutex};

use pi_coding_agent::core::extensions::types::{BoxFuture, HandlerError, NotifyType};
use pi_coding_agent::core::session_control_db::{
    WritableSessionMetadata, read_session_metadata, write_session_metadata,
};
use pi_coding_agent::core::session_manager::{SessionManager, SetMetadataControlDbPathOptions};
use serde_json::json;
use support::{Calls, Harness, RecordingUi, calls_json};
use tempfile::TempDir;

fn oracle_for(module: &str) -> oracle::TsOracle {
    oracle::TsOracle::spawn("bundled-extensions-oracle.ts", module)
}

#[tokio::test]
async fn self_restart_matches_typescript() {
    let mut oracle = oracle_for("packages/coding-agent/extensions/self-restart/src/index.ts");
    let response = oracle.request(&json!({
        "id": 1,
        "extension": "self-restart",
        "steps": [
            {"op": "registrations"},
            {"op": "command", "name": "restart", "args": ""},
            {"op": "tool", "name": "restart_self", "params": {}}
        ]
    }));
    assert_eq!(response["ok"], true, "{response}");

    let harness = Harness::load("/tmp", Arc::new(RecordingUi::default()), |api| {
        pi_coding_agent::bundled_extensions::self_restart::self_restart_extension(api)
    })
    .await;

    assert_eq!(harness.registrations(), response["output"]["steps"][0]["result"]);
    harness.run_command("restart", "").await.unwrap();
    assert_eq!(
        calls_json(&harness.take_calls()),
        response["output"]["steps"][1]["calls"]
    );

    let tool = harness.tool("restart_self");
    let result = (tool.execute)("call-1".to_owned(), json!({}), None, None, harness.context())
        .await
        .unwrap();
    assert_eq!(json!(result), response["output"]["steps"][2]["result"]);
    assert_eq!(
        calls_json(&harness.take_calls()),
        response["output"]["steps"][2]["calls"]
    );
}

#[tokio::test]
async fn spec_validation_matches_typescript() {
    let mut oracle = oracle_for("packages/coding-agent/extensions/spec-validation/src/index.ts");
    let response = oracle.request(&json!({
        "id": 2,
        "extension": "spec-validation",
        "steps": [
            {"op": "registrations"},
            {"op": "command", "name": "spec-validation", "args": "", "idle": true},
            {"op": "command", "name": "spec-validation", "args": "", "idle": false}
        ]
    }));
    assert_eq!(response["ok"], true, "{response}");

    let harness = Harness::load("/tmp", Arc::new(RecordingUi::default()), |api| {
        pi_coding_agent::bundled_extensions::spec_validation::spec_validation_extension(api)
    })
    .await;

    assert_eq!(harness.registrations(), response["output"]["steps"][0]["result"]);
    harness.run_command("spec-validation", "").await.unwrap();
    assert_eq!(
        calls_json(&harness.take_calls()),
        response["output"]["steps"][1]["calls"]
    );

    let harness = Harness::load("/tmp", Arc::new(RecordingUi::default()), |api| {
        pi_coding_agent::bundled_extensions::spec_validation::spec_validation_extension(api)
    })
    .await;
    let mut runner = harness.runner.clone();
    let idle_actions = support::context_actions(&harness.calls, None);
    runner.bind_core(
        support::actions(&harness.calls),
        pi_coding_agent::core::extensions::types::ExtensionContextActions {
            is_idle: Arc::new(|| false),
            ..idle_actions
        },
        None,
    );
    let handler = harness.command("spec-validation").handler;
    let error = handler("".to_owned(), runner.create_command_context())
        .await
        .unwrap_err();
    assert_eq!(error.message, response["output"]["steps"][2]["error"]["message"]);
    assert_eq!(
        calls_json(&harness.take_calls()),
        response["output"]["steps"][2]["calls"]
    );
}

#[tokio::test]
async fn session_id_matches_typescript() {
    let manager = Arc::new(std::sync::Mutex::new(
        SessionManager::in_memory(Some("/tmp"), None).unwrap(),
    ));
    let harness = Harness::load_full(
        "/tmp",
        Arc::clone(&manager),
        Arc::new(RecordingUi::default()),
        None,
        pi_coding_agent::bundled_extensions::session_id::session_id_extension,
    )
    .await;
    let session_id = manager.lock().unwrap().get_session_id().to_owned();

    let mut oracle = oracle_for("packages/coding-agent/extensions/session-id/src/index.ts");
    let response = oracle.request(&json!({
        "id": 3,
        "extension": "session-id",
        "steps": [
            {"op": "registrations"},
            {"op": "command", "name": "session-id", "args": "", "sessionId": session_id}
        ]
    }));
    assert_eq!(response["ok"], true, "{response}");

    assert_eq!(harness.registrations(), response["output"]["steps"][0]["result"]);
    harness.run_command("session-id", "").await.unwrap();
    assert_eq!(
        calls_json(&harness.take_calls()),
        response["output"]["steps"][1]["calls"]
    );
}

#[tokio::test]
async fn session_archive_matches_typescript() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("control.sqlite");
    let db_path = db_path.to_string_lossy().into_owned();
    let session_file = temp.path().join("current.jsonl");
    let session_file = session_file.to_string_lossy().into_owned();
    write_session_metadata(
        &db_path,
        &WritableSessionMetadata {
            session_path: session_file.clone(),
            id: "current".to_owned(),
            cwd: temp.path().to_string_lossy().into_owned(),
            name: None,
            parent_session_path: None,
            archived_at: None,
            is_archived: None,
            is_subagent: None,
            subagent_name: None,
            model_provider: None,
            model_id: None,
            thinking_level: None,
            created_at: "2026-07-11T00:00:00.000Z".to_owned(),
            modified_at: "2026-07-11T00:00:00.000Z".to_owned(),
            message_count: 1,
            first_message: "hello".to_owned(),
            all_messages_text: "hello".to_owned(),
            index_message_text: None,
        },
    )
    .unwrap();

    let mut manager = SessionManager::create(
        &temp.path().to_string_lossy(),
        Some(&temp.path().to_string_lossy()),
        None,
    )
    .unwrap();
    manager
        .set_metadata_control_db_path(Some(&db_path), SetMetadataControlDbPathOptions::default())
        .unwrap();
    let manager = Arc::new(std::sync::Mutex::new(manager));

    let harness = Harness::load_full(
        &temp.path().to_string_lossy(),
        Arc::clone(&manager),
        Arc::new(RecordingUi::default()),
        Some(db_path.clone()),
        pi_coding_agent::bundled_extensions::session_archive::session_archive_extension,
    )
    .await;

    let mut oracle = oracle_for("packages/coding-agent/extensions/session-archive/src/index.ts");
    let response = oracle.request(&json!({
        "id": 4,
        "extension": "session-archive",
        "steps": [
            {"op": "registrations"},
            {"op": "command", "name": "archive", "args": "", "controlDbPath": db_path, "sessionFile": session_file},
            {"op": "command", "name": "archive", "args": "now", "controlDbPath": db_path, "sessionFile": session_file}
        ]
    }));
    assert_eq!(response["ok"], true, "{response}");

    assert_eq!(harness.registrations(), response["output"]["steps"][0]["result"]);
    harness.run_command("archive", "").await.unwrap();
    assert_eq!(
        calls_json(&harness.take_calls()),
        response["output"]["steps"][1]["calls"]
    );
    assert_eq!(
        read_session_metadata(&db_path, &session_file)
            .unwrap()
            .unwrap()
            .is_archived,
        Some(true)
    );

    harness.run_command("archive", "now").await.unwrap();
    assert_eq!(
        calls_json(&harness.take_calls()),
        response["output"]["steps"][2]["calls"]
    );
}

#[tokio::test]
async fn debug_matches_typescript() {
    let ui = Arc::new(RecordingUi::default());
    let controller_calls: Arc<Mutex<Calls>> = ui.calls();
    let manager = Arc::new(std::sync::Mutex::new(
        SessionManager::in_memory(Some("/tmp"), None).unwrap(),
    ));
    let harness = Harness::load_full("/tmp", Arc::clone(&manager), Arc::clone(&ui), None, move |api| {
        pi_coding_agent::bundled_extensions::debug::debug_extension(
            api,
            Arc::new(move || Arc::new(RecordingDebugRepl(Arc::clone(&controller_calls)))),
        )
    })
    .await;
    let session_id = manager.lock().unwrap().get_session_id().to_owned();

    let mut oracle = oracle_for("packages/coding-agent/extensions/debug/src/index.ts");
    let response = oracle.request(&json!({
        "id": 5,
        "extension": "debug",
        "steps": [
            {"op": "registrations"},
            {"op": "command", "name": "debug", "args": "", "sessionId": session_id},
            {"op": "command", "name": "debug", "args": "off"},
            {"op": "command", "name": "debug", "args": "attach"}
        ]
    }));
    assert_eq!(response["ok"], true, "{response}");

    assert_eq!(harness.registrations(), response["output"]["steps"][0]["result"]);
    for (index, args) in ["", "off", "attach"].iter().enumerate() {
        harness.run_command("debug", args).await.unwrap();
        assert_eq!(
            calls_json(&harness.take_calls()),
            response["output"]["steps"][index + 1]["calls"],
            "step {index}"
        );
    }
}

#[tokio::test]
async fn registers_all_small_extensions_under_source_ids() {
    let mut registry = pi_coding_agent::core::extensions::loader::ExtensionRegistry::new();
    pi_coding_agent::bundled_extensions::register_small_extensions(
        &mut registry,
        &pi_coding_agent::bundled_extensions::SmallExtensionsOptions {
            get_debug_repl: Arc::new(|| Arc::new(RecordingDebugRepl(Arc::new(Mutex::new(Calls::default()))))),
        },
    );
    assert_eq!(
        registry.ids(),
        vec![
            "<first-party:claude-memory-enrich>",
            "<first-party:claude-memory-session-end>",
            "<first-party:docs-tree-context>",
            "<first-party:ask-secret>",
            "<first-party:browser-cli>",
            "<first-party:approval-controls>",
            "<first-party:debug>",
            "<first-party:run-plan>",
            "<first-party:safe>",
            "<first-party:self-restart>",
            "<first-party:session-archive>",
            "<first-party:session-id>",
            "<first-party:spec-validation>",
        ]
    );
    let (extensions, errors) = registry
        .load_registered(
            &[],
            "/tmp",
            Arc::new(pi_coding_agent::core::event_bus::EventBusController::new()),
            pi_coding_agent::core::extensions::create_extension_runtime(),
        )
        .await;
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(extensions.len(), 13);
    let tool_names: Vec<&str> = extensions
        .iter()
        .flat_map(|extension| extension.tools.keys().map(String::as_str))
        .collect();
    assert_eq!(tool_names, vec!["ask_secret", "browser-cli", "restart_self"]);
    let command_names: Vec<&str> = extensions
        .iter()
        .flat_map(|extension| extension.commands.keys().map(String::as_str))
        .collect();
    assert_eq!(
        command_names,
        vec![
            "approvals",
            "sandbox",
            "debug",
            "run-plan",
            "safe",
            "restart",
            "archive",
            "session-id",
            "spec-validation",
        ]
    );
}

struct RecordingDebugRepl(Arc<Mutex<Calls>>);

impl pi_coding_agent::bundled_extensions::debug::DebugReplController for RecordingDebugRepl {
    fn enable(&self, session_id: &str) -> BoxFuture<'static, Result<String, HandlerError>> {
        let session_id = session_id.to_owned();
        let calls = Arc::clone(&self.0);
        Box::pin(async move {
            calls
                .lock()
                .unwrap()
                .debug
                .push(("enable".to_owned(), Some(session_id.clone())));
            Ok(format!("debug {session_id}"))
        })
    }

    fn disable(&self) -> BoxFuture<'static, Result<(), HandlerError>> {
        let calls = Arc::clone(&self.0);
        Box::pin(async move {
            calls.lock().unwrap().debug.push(("disable".to_owned(), None));
            Ok(())
        })
    }
}

#[test]
fn archive_command_rejects_missing_control_db() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let harness = Harness::load("/tmp", Arc::new(RecordingUi::default()), |api| {
            pi_coding_agent::bundled_extensions::session_archive::session_archive_extension(api)
        })
        .await;
        harness.run_command("archive", "").await.unwrap();
        let calls = harness.take_calls();
        assert_eq!(
            calls.notify,
            vec![(
                "Session archive requires a control database.".to_owned(),
                Some(NotifyType::Error)
            )]
        );
    });
}
