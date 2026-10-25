//! Differential and behavioral coverage for the small command/tool bundled extensions.

use pi_js_compat::js_json::JsString;
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
use pi_js_compat::js_json as json;
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
    assert_eq!(
        response["ok"],
        pi_js_compat::js_json::JsValue::Bool(true),
        "{response:?}"
    );

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
    let result = (tool.execute)("call-1".to_owned(), json!({}), None, None, Some(harness.context()))
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
    assert_eq!(
        response["ok"],
        pi_js_compat::js_json::JsValue::Bool(true),
        "{response:?}"
    );

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
    runner
        .bind_core(
            support::actions(&harness.calls),
            pi_coding_agent::core::extensions::types::ExtensionContextActions {
                is_idle: Arc::new(|| false),
                ..idle_actions
            },
            None,
        )
        .unwrap();
    let handler = harness.command("spec-validation").handler;
    let error = handler("".to_owned().into(), runner.create_command_context())
        .await
        .unwrap_err();
    assert_eq!(
        pi_js_compat::js_json::JsValue::String(error.message),
        response["output"]["steps"][2]["error"]["message"]
    );
    assert_eq!(
        calls_json(&harness.take_calls()),
        response["output"]["steps"][2]["calls"]
    );
}

#[tokio::test]
async fn session_id_matches_typescript() {
    let manager = Arc::new(std::sync::Mutex::new(
        SessionManager::in_memory(Some(&JsString::from("/tmp")), None).unwrap(),
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
    assert_eq!(
        response["ok"],
        pi_js_compat::js_json::JsValue::Bool(true),
        "{response:?}"
    );

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
    let db_path = db_path.to_str().expect("utf-8 test path").to_owned();
    let mut manager = SessionManager::create(
        &JsString::from(temp.path().to_str().expect("utf-8 test path")),
        Some(&JsString::from(temp.path().to_str().expect("utf-8 test path"))),
        None,
    )
    .unwrap();
    manager.persist_for_recovery().unwrap();
    let session_file = manager.get_session_file().unwrap().to_owned();
    write_session_metadata(
        &db_path,
        &WritableSessionMetadata {
            session_path: session_file.clone(),
            id: manager.get_session_id().to_owned(),
            cwd: temp.path().to_str().expect("utf-8 test path").to_owned().into(),
            name: None,
            parent_session_path: None,
            archived_at: None,
            is_archived: None,
            is_subagent: None,
            subagent_name: None,
            model_provider: None,
            model_id: None,
            thinking_level: None,
            created_at: "2026-07-11T00:00:00.000Z".to_owned().into(),
            modified_at: "2026-07-11T00:00:00.000Z".to_owned().into(),
            message_count: 1,
            first_message: "hello".to_owned().into(),
        },
    )
    .unwrap();

    manager
        .set_metadata_control_db_path(
            Some(&JsString::from(&db_path)),
            SetMetadataControlDbPathOptions::default(),
        )
        .unwrap();
    let manager = Arc::new(std::sync::Mutex::new(manager));

    let harness = Harness::load_full(
        &temp.path().to_str().expect("utf-8 test path"),
        Arc::clone(&manager),
        Arc::new(RecordingUi::default()),
        Some(db_path.clone()),
        pi_coding_agent::bundled_extensions::session_archive::session_archive_extension,
    )
    .await;

    let mut oracle = oracle_for("session-names:packages/coding-agent/extensions/session-archive/src/index.ts");
    let response = oracle.request(&json!({
        "id": 4,
        "extension": "session-archive",
        "steps": [
            {"op": "registrations"},
            {"op": "command", "name": "archive", "args": "", "controlDbPath": db_path, "sessionFile": session_file},
            {"op": "command", "name": "archive", "args": "now", "controlDbPath": db_path, "sessionFile": session_file}
        ]
    }));
    assert_eq!(
        response["ok"],
        pi_js_compat::js_json::JsValue::Bool(true),
        "{response:?}"
    );
    assert_eq!(harness.registrations(), response["output"]["steps"][0]["result"]);
    for step in response["output"]["steps"].as_array().unwrap() {
        assert_eq!(step["error"], pi_js_compat::js_json::JsValue::Null, "{response:?}");
    }
    harness.run_command("archive", "").await.unwrap();
    assert_eq!(
        calls_json(&harness.take_calls()),
        response["output"]["steps"][1]["calls"]
    );
    assert_ne!(
        read_session_metadata(&db_path, &session_file)
            .unwrap()
            .unwrap()
            .is_archived,
        Some(true)
    );
    assert!(
        std::path::Path::new(&session_file.to_utf8().expect("utf-8 test path")).exists(),
        "quit action must be deferred"
    );
    harness.run_command("archive", "now").await.unwrap();
    assert_eq!(
        calls_json(&harness.take_calls()),
        response["output"]["steps"][2]["calls"]
    );
    use pi_coding_agent::core::extensions::types::{ExtensionEvent, SessionShutdownEvent, SessionShutdownReason};
    manager
        .lock()
        .unwrap()
        .append_message(
            pi_js_compat::js_json::from_value(&json!({"role":"user","content":"last write before quit","timestamp":1}))
                .unwrap(),
        )
        .unwrap();
    manager.lock().unwrap().persist_for_recovery().unwrap();
    harness
        .runner
        .emit(&mut ExtensionEvent::SessionShutdown(SessionShutdownEvent {
            reason: SessionShutdownReason::Quit,
            target_session_file: None,
        }))
        .await;
    let archived = format!("{}.zst", session_file.to_utf8().expect("utf-8 test path"));
    assert!(
        pi_coding_agent::core::session_transcript::read_session_transcript_text(&archived)
            .unwrap()
            .contains("last write before quit")
    );
    assert!(!std::path::Path::new(&session_file.to_utf8().expect("utf-8 test path")).exists());
    assert!(std::path::Path::new(&archived).exists());
    assert_eq!(
        read_session_metadata(&db_path, &archived).unwrap().unwrap().is_archived,
        Some(true)
    );
    harness
        .runner
        .emit(&mut ExtensionEvent::SessionShutdown(SessionShutdownEvent {
            reason: SessionShutdownReason::Quit,
            target_session_file: None,
        }))
        .await;
}

#[tokio::test]
async fn debug_matches_typescript() {
    let ui = Arc::new(RecordingUi::default());
    let controller_calls: Arc<Mutex<Calls>> = ui.calls();
    let manager = Arc::new(std::sync::Mutex::new(
        SessionManager::in_memory(Some(&JsString::from("/tmp")), None).unwrap(),
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
    assert_eq!(
        response["ok"],
        pi_js_compat::js_json::JsValue::Bool(true),
        "{response:?}"
    );

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
            multi_agent_store: None,
            fast_mode_authority: pi_coding_agent::bundled_extensions::codex_fast::FastModeAuthority::new(),
            fetch: None,
        },
    );
    // Registration order follows `main.ts` `createFirstPartyExtensionFactories`.
    assert_eq!(
        registry.ids(),
        vec![
            "<first-party:approval-controls>",
            "<first-party:ask-secret>",
            "<first-party:browser-cli>",
            "<first-party:claude-bash-hook>",
            "<first-party:claude-memory-enrich>",
            "<first-party:claude-memory-session-end>",
            "<first-party:codex-daybreak>",
            "<first-party:codex-fast>",
            "<first-party:codex-image-generation>",
            "<first-party:codex-usage>",
            "<first-party:codex-web-search>",
            "<first-party:debug>",
            "<first-party:docs-tree-context>",
            "<first-party:default-footer>",
            "<first-party:effort>",
            "<first-party:loop>",
            "<first-party:openai-remote-compact>",
            "<first-party:run-plan>",
            "<first-party:safe>",
            "<first-party:spec-validation>",
            "<first-party:session-archive>",
            "<first-party:session-autoname>",
            "<first-party:self-restart>",
            "<first-party:session-id>",
        ]
    );
    let (extensions, errors) = registry
        .load_registered(
            &[],
            &JsString::from("/tmp"),
            Arc::new(pi_coding_agent::core::event_bus::EventBusController::new()),
            pi_coding_agent::core::extensions::create_extension_runtime(),
        )
        .await;
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(extensions.len(), 24);
    let tool_names: Vec<String> = extensions
        .iter()
        .flat_map(|extension| extension.read().tools.keys().cloned().collect::<Vec<_>>())
        .collect();
    assert_eq!(
        tool_names,
        vec![
            "ask_secret",
            "browser-cli",
            "image_gen",
            "web_search",
            "loop",
            "set_session_name",
            "restart_self",
        ]
    );
    let command_names: Vec<JsString> = extensions
        .iter()
        .flat_map(|extension| extension.read().commands.keys().cloned().collect::<Vec<_>>())
        .collect();
    assert_eq!(
        command_names,
        vec![
            "approvals",
            "sandbox",
            "daybreak",
            "fast",
            "usage",
            "debug",
            "multi-agent",
            "effort",
            "loop",
            "run-plan",
            "safe",
            "spec-validation",
            "unarchive",
            "archive",
            "delete",
            "restart",
            "session-id",
        ]
    );
}

struct RecordingDebugRepl(Arc<Mutex<Calls>>);

impl pi_coding_agent::bundled_extensions::debug::DebugReplController for RecordingDebugRepl {
    fn enable(&self, session_id: &JsString) -> BoxFuture<'static, Result<JsString, HandlerError>> {
        let session_id = session_id.to_owned();
        let calls = Arc::clone(&self.0);
        Box::pin(async move {
            calls
                .lock()
                .unwrap()
                .debug
                .push(("enable".to_owned().into(), Some(session_id.clone().into())));
            Ok(JsString::join([JsString::from("debug "), session_id], ""))
        })
    }

    fn disable(&self) -> BoxFuture<'static, Result<(), HandlerError>> {
        let calls = Arc::clone(&self.0);
        Box::pin(async move {
            calls.lock().unwrap().debug.push(("disable".to_owned().into(), None));
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
                JsString::from("Session archive requires a control database."),
                Some(NotifyType::Error)
            )]
        );
    });
}

#[tokio::test]
async fn session_archive_command_branches_match_frozen() {
    use pi_coding_agent::core::extensions::types::{ExtensionEvent, SessionShutdownEvent, SessionShutdownReason};
    use pi_coding_agent::core::multi_agent_store::{AgentSnapshot, MultiAgentStore};
    use pi_coding_agent::core::session_control_db::archive_session;
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("control.sqlite").display().to_string();
    let mut manager = SessionManager::create(
        &JsString::from(temp.path().to_str().unwrap()),
        Some(&JsString::from(temp.path().to_str().unwrap())),
        None,
    )
    .unwrap();
    manager.persist_for_recovery().unwrap();
    manager
        .set_metadata_control_db_path(Some(&JsString::from(&db)), Default::default())
        .unwrap();
    let path = manager.get_session_file().unwrap().to_owned();
    let manager = Arc::new(Mutex::new(manager));
    for command in ["archive", "delete", "unarchive"] {
        for mode in ["usage", "no-db", "unpersisted", "valid"] {
            let persisted = mode != "unpersisted";
            let db_path = (mode != "no-db").then(|| db.clone());
            let args = if mode == "usage" { "now" } else { "" };
            let session = if persisted {
                Arc::clone(&manager)
            } else {
                Arc::new(Mutex::new(
                    SessionManager::in_memory(Some(&JsString::from("/tmp")), None).unwrap(),
                ))
            };
            let harness = Harness::load_full(
                "/tmp",
                session,
                Arc::new(RecordingUi::default()),
                db_path.clone(),
                pi_coding_agent::bundled_extensions::session_archive::session_archive_extension,
            )
            .await;
            let mut oracle = oracle_for("packages/coding-agent/extensions/session-archive/src/index.ts");
            let response = oracle.request(&json!({"id":1,"extension":"session-archive","steps":[{"op":"command","name":command,"args":args,"controlDbPath":db_path,"sessionFile":persisted.then_some(&path)}]}));
            assert_eq!(
                response["output"]["steps"][0]["error"],
                pi_js_compat::js_json::JsValue::Null,
                "{response:?}"
            );
            harness.run_command(command, args).await.unwrap();
            assert_eq!(
                calls_json(&harness.take_calls()),
                response["output"]["steps"][0]["calls"],
                "{command}/{mode}"
            );
            if command == "archive" && mode == "valid" {
                harness
                    .runner
                    .emit(&mut ExtensionEvent::SessionShutdown(SessionShutdownEvent {
                        reason: SessionShutdownReason::Reload,
                        target_session_file: None,
                    }))
                    .await;
                harness
                    .runner
                    .emit(&mut ExtensionEvent::SessionShutdown(SessionShutdownEvent {
                        reason: SessionShutdownReason::Quit,
                        target_session_file: None,
                    }))
                    .await;
                assert!(
                    std::path::Path::new(&path.to_utf8().expect("utf-8 test path")).exists(),
                    "non-quit consumes pending action"
                );
            }
        }
    }
    archive_session(&db, &path).unwrap();
    let harness = Harness::load_full(
        "/tmp",
        Arc::clone(&manager),
        Arc::new(RecordingUi::default()),
        Some(db.clone()),
        pi_coding_agent::bundled_extensions::session_archive::session_archive_extension,
    )
    .await;
    let mut oracle = oracle_for("packages/coding-agent/extensions/session-archive/src/index.ts");
    let response = oracle.request(&json!({"id":2,"extension":"session-archive","steps":[{"op":"command","name":"unarchive","controlDbPath":db,"sessionFile":path}]}));
    archive_session(&db, &path).unwrap();
    harness.run_command("unarchive", "").await.unwrap();
    assert_eq!(
        calls_json(&harness.take_calls()),
        response["output"]["steps"][0]["calls"]
    );
    assert_eq!(
        read_session_metadata(&db, &path).unwrap().unwrap().is_archived,
        Some(false)
    );
    for (count, has_path) in [(1, true), (2, true), (1, false)] {
        let store = Arc::new(MultiAgentStore::new(Default::default()));
        let agents:Vec<_> = (0..count).map(|index| json!({"id":format!("agent-{index}"),"displayName":"child","agentType":"worker","lifecycle":"running","revision":1,"createdAt":"2026-10-21T00:00:00.000Z","updatedAt":"2026-10-21T00:00:00.000Z","cwd":"/tmp","permission":{"policy":"on-request","narrowed":true},"transcript":{"sessionId":"child","path":has_path.then_some("/tmp/child.jsonl")}})).collect();
        for agent in &agents {
            store
                .publish_lifecycle_coordinator_snapshot(
                    &pi_js_compat::js_json::from_value::<AgentSnapshot>(&agent.clone()).unwrap(),
                )
                .unwrap();
        }
        let ui = Arc::new(RecordingUi::default());
        *ui.confirmed.lock().unwrap() = true;
        let harness = Harness::load_full(
            "/tmp",
            Arc::clone(&manager),
            ui,
            Some(db.clone()),
            pi_coding_agent::bundled_extensions::session_archive::session_archive_extension,
        )
        .await;
        let mut runner = harness.runner.clone();
        runner
            .bind_core(
                support::actions(&harness.calls),
                pi_coding_agent::core::extensions::types::ExtensionContextActions {
                    get_multi_agent_store: Some(Arc::new(move || Some(Arc::clone(&store)))),
                    ..support::context_actions(&harness.calls, Some(db.clone()))
                },
                None,
            )
            .unwrap();
        let response = oracle.request(&json!({"id":3,"extension":"session-archive","steps":[{"op":"command","name":"delete","controlDbPath":db,"sessionFile":path,"activeAgents":agents,"confirmed":true}]}));
        (harness.command("delete").handler)("".to_owned().into(), runner.create_command_context())
            .await
            .unwrap();
        assert_eq!(
            calls_json(&harness.take_calls()),
            response["output"]["steps"][0]["calls"]
        );
    }
    let ui = Arc::new(RecordingUi::default());
    *ui.confirmed.lock().unwrap() = true;
    let harness = Harness::load_full(
        "/tmp",
        Arc::clone(&manager),
        ui,
        Some(db.clone()),
        pi_coding_agent::bundled_extensions::session_archive::session_archive_extension,
    )
    .await;
    let response = oracle.request(&json!({"id":4,"extension":"session-archive","steps":[{"op":"command","name":"delete","controlDbPath":db,"sessionFile":path,"confirmed":true}]}));
    harness.run_command("delete", "").await.unwrap();
    assert_eq!(
        calls_json(&harness.take_calls()),
        response["output"]["steps"][0]["calls"]
    );
    assert!(std::path::Path::new(&path.to_utf8().expect("utf-8 test path")).exists());
    harness
        .runner
        .emit(&mut ExtensionEvent::SessionShutdown(SessionShutdownEvent {
            reason: SessionShutdownReason::Quit,
            target_session_file: None,
        }))
        .await;
    assert!(!std::path::Path::new(&path.to_utf8().expect("utf-8 test path")).exists());
    assert!(read_session_metadata(&db, &path).unwrap().is_none());
}

#[tokio::test]
async fn session_archive_error_worker() {
    use pi_coding_agent::core::extensions::types::{ExtensionEvent, SessionShutdownEvent, SessionShutdownReason};
    let Ok(mode) = std::env::var("ARCHIVE_ERROR_MODE") else {
        return;
    };
    let dir = std::env::var("ARCHIVE_ERROR_DIR").unwrap();
    let db = format!("{dir}/control.sqlite");
    let mut manager = SessionManager::create(&JsString::from(&dir), Some(&JsString::from(&dir)), None).unwrap();
    manager.persist_for_recovery().unwrap();
    manager
        .set_metadata_control_db_path(Some(&JsString::from(&db)), Default::default())
        .unwrap();
    let path = manager.get_session_file().unwrap().to_owned();
    std::fs::remove_file(path.to_utf8().expect("utf-8 test path")).unwrap();
    if mode == "delete" {
        std::fs::create_dir(path.to_utf8().expect("utf-8 test path")).unwrap();
    }
    std::fs::write(
        format!("{dir}/paths.json"),
        pi_js_compat::js_json::stringify_lossless(&json!({"path":path,"db":db})),
    )
    .unwrap();
    let ui = Arc::new(RecordingUi::default());
    *ui.confirmed.lock().unwrap() = true;
    let harness = Harness::load_full(
        &dir,
        Arc::new(Mutex::new(manager)),
        ui,
        Some(db),
        pi_coding_agent::bundled_extensions::session_archive::session_archive_extension,
    )
    .await;
    harness.run_command(&mode, "").await.unwrap();
    harness
        .runner
        .emit(&mut ExtensionEvent::SessionShutdown(SessionShutdownEvent {
            reason: SessionShutdownReason::Quit,
            target_session_file: None,
        }))
        .await;
}

#[test]
fn session_archive_shutdown_errors_match_frozen_stderr() {
    for mode in ["archive", "delete"] {
        let dir = TempDir::new().unwrap();
        let missing_bin = dir.path().join("missing-bin");
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "session_archive_error_worker", "--nocapture"])
            .env("ARCHIVE_ERROR_MODE", mode)
            .env("ARCHIVE_ERROR_DIR", dir.path())
            .env("PATH", &missing_bin)
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let paths: pi_js_compat::js_json::JsValue =
            serde_json::from_slice(&std::fs::read(dir.path().join("paths.json")).unwrap()).unwrap();
        let mut oracle = oracle_for("packages/coding-agent/extensions/session-archive/src/index.ts");
        let response=oracle.request(&json!({"id":1,"extension":"session-archive","steps":[{"op":"command","name":mode,"confirmed":true,"controlDbPath":paths["db"],"sessionFile":paths["path"],"env":{"PATH":missing_bin}},{"op":"archiveShutdownStderr"}]}));
        assert_eq!(
            response["ok"],
            pi_js_compat::js_json::JsValue::Bool(true),
            "{response:?}"
        );
        for step in response["output"]["steps"].as_array().unwrap() {
            assert_eq!(step["error"], pi_js_compat::js_json::JsValue::Null, "{response:?}");
        }
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(stderr.starts_with(&format!("Failed to {mode} session:")), "{stderr}");
        assert_eq!(
            JsString::from(stderr),
            *response["output"]["steps"][1]["result"].as_string().unwrap()
        );
    }
}
