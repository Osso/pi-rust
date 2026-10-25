//! Differential coverage for the `run-plan` bundled extension.

use pi_js_compat::js_json::JsString;
#[path = "support/bundled_extension_commands.rs"]
mod command_support;
use crate::shared_bundled_extensions as support;
use crate::shared_oracle as oracle;

use std::fs;
use std::sync::Arc;

use pi_coding_agent::core::extensions::types::ExtensionContextActions;
use pi_coding_agent::core::session_manager::SessionManager;
use pi_js_compat::js_json as json;
use pi_js_compat::process_env;
use support::{Harness, RecordingUi, calls_json};
use tempfile::TempDir;

fn oracle_for(module: &str) -> oracle::TsOracle {
    oracle::TsOracle::spawn("bundled-extensions-oracle.ts", module)
}

fn plan_env() -> pi_js_compat::js_json::JsValue {
    json!({
        "PLAN_FILE": process_env::get("PLAN_FILE"),
        "PLAN_PATH": process_env::get("PLAN_PATH"),
        "PI_PLAN_FILE": process_env::get("PI_PLAN_FILE"),
        "PI_PLAN_PATH": process_env::get("PI_PLAN_PATH"),
    })
}

#[tokio::test]
async fn run_plan_matches_typescript() {
    let temp = TempDir::new().expect("temp dir");
    let cwd = temp.path().to_str().expect("utf-8 test path").to_owned();
    let plan_path = temp.path().join("PLAN.md");
    let alt_path = temp.path().join("ALT.md");
    fs::write(
        &plan_path,
        "- [x] Done item\n* [X] Also done\n- [ ] First open item\n* [ ] Second open item\n",
    )
    .unwrap();
    fs::write(&alt_path, "* [ ] Alternate plan\n").unwrap();
    fs::write(temp.path().join("notes.txt"), "- [ ] Ignored\n").unwrap();

    let mut oracle = oracle_for("packages/coding-agent/extensions/run-plan/src/index.ts");
    let entries = json!([{
        "type": "custom",
        "customType": "run-plan:active",
        "data": {"file": "PLAN.md", "path": plan_path.to_str().expect("utf-8 test path")}
    }]);
    let response = oracle.request(&json!({
        "id": 1,
        "extension": "run-plan",
        "steps": [
            {"op": "registrations"},
            {"op": "env"},
            {"op": "command", "name": "run-plan", "args": "", "cwd": cwd},
            {"op": "env"},
            {"op": "event", "name": "agent_end", "event": {}, "cwd": cwd},
            {"op": "event", "name": "agent_end", "event": {"sessionContinuation": "cwd_relocation"}, "cwd": cwd},
            {"op": "completions", "name": "run-plan", "prefix": "", "cwd": cwd},
            {"op": "command", "name": "run-plan", "args": "ALT.md", "cwd": cwd},
            {"op": "command", "name": "run-plan", "args": "MISSING.md", "cwd": cwd},
            {"op": "command", "name": "run-plan", "args": "", "cwd": cwd, "idle": false},
            {"op": "event", "name": "session_start", "event": {}, "cwd": cwd, "entries": entries},
            {"op": "env"},
            {"op": "event", "name": "agent_end", "event": {}, "cwd": cwd}
        ]
    }));
    assert_eq!(
        response["ok"],
        pi_js_compat::js_json::JsValue::Bool(true),
        "{response:?}"
    );
    let steps = response["output"]["steps"].as_array().unwrap();

    let previous_dir = std::env::current_dir().unwrap();
    pi_js_compat::process_cwd::chdir(&pi_js_compat::node_fs::decode_node_path(std::ffi::OsStr::new(
        &(temp.path()),
    )))
    .unwrap();

    let harness = Harness::load_full(
        &cwd,
        Arc::new(std::sync::Mutex::new(
            SessionManager::in_memory(Some(&JsString::from(&cwd)), None).unwrap(),
        )),
        Arc::new(RecordingUi::default()),
        None,
        pi_coding_agent::bundled_extensions::run_plan::run_plan_extension,
    )
    .await;

    assert_eq!(harness.registrations(), steps[0]["result"]);
    assert_eq!(plan_env(), steps[1]["result"]);

    harness.run_command("run-plan", "").await.unwrap();
    assert_eq!(calls_json(&harness.take_calls()), steps[2]["calls"]);
    assert_eq!(plan_env(), steps[3]["result"]);

    emit_agent_end(&harness, json!({})).await;
    assert_eq!(calls_json(&harness.take_calls()), steps[4]["calls"]);
    emit_agent_end(&harness, json!({"sessionContinuation": "cwd_relocation"})).await;
    assert_eq!(calls_json(&harness.take_calls()), steps[5]["calls"]);

    let completions = harness
        .command("run-plan")
        .get_argument_completions
        .expect("run-plan completions");
    let completions = completions(&"".into()).unwrap().unwrap_or_default();
    assert_eq!(json!(completions), steps[6]["result"]);

    harness.run_command("run-plan", "ALT.md").await.unwrap();
    assert_eq!(calls_json(&harness.take_calls()), steps[7]["calls"]);

    let error = harness.run_command("run-plan", "MISSING.md").await.unwrap_err();
    assert_eq!(
        pi_js_compat::js_json::JsValue::String(error.message),
        steps[8]["error"]["message"]
    );

    let mut idle_runner = harness.runner.clone();
    idle_runner
        .bind_core(
            support::actions(&harness.calls),
            ExtensionContextActions {
                is_idle: Arc::new(|| false),
                ..support::context_actions(&harness.calls, None)
            },
            None,
        )
        .unwrap();
    let handler = harness.command("run-plan").handler;
    let error = handler("".to_owned().into(), idle_runner.create_command_context())
        .await
        .unwrap_err();
    assert_eq!(
        pi_js_compat::js_json::JsValue::String(error.message),
        steps[9]["error"]["message"]
    );
    assert_eq!(calls_json(&harness.take_calls()), steps[9]["calls"]);

    process_env::clear();
    let mut restored_manager = SessionManager::in_memory(Some(&JsString::from(&cwd)), None).unwrap();
    restored_manager
        .append_custom_entry(
            &JsString::from("run-plan:active"),
            Some(json!({"file": "PLAN.md", "path": plan_path.to_str().expect("utf-8 test path")})),
        )
        .unwrap();
    let restored = Harness::load_full(
        &cwd,
        Arc::new(std::sync::Mutex::new(restored_manager)),
        Arc::new(RecordingUi::default()),
        None,
        pi_coding_agent::bundled_extensions::run_plan::run_plan_extension,
    )
    .await;
    emit_session_start(&restored).await;
    assert_eq!(plan_env(), steps[11]["result"]);
    emit_agent_end(&restored, json!({})).await;
    assert_eq!(calls_json(&restored.take_calls()), steps[12]["calls"]);

    process_env::clear();
    pi_js_compat::process_cwd::chdir(&pi_js_compat::node_fs::decode_node_path(std::ffi::OsStr::new(
        &(previous_dir),
    )))
    .unwrap();
}

async fn emit_agent_end(harness: &Harness, event: pi_js_compat::js_json::JsValue) {
    let event: pi_coding_agent::core::extensions::types::AgentEndEvent = pi_js_compat::js_json::from_value(
        &json!({"messages": [], "sessionContinuation": event.get("sessionContinuation")}),
    )
    .unwrap();
    let handler = harness.extension.read().handlers.agent_end[0].clone();
    handler.call(event, harness.context()).await.unwrap();
}

async fn emit_session_start(harness: &Harness) {
    let event = pi_coding_agent::core::extensions::types::SessionStartEvent {
        reason: pi_coding_agent::core::extensions::types::SessionStartReason::Resume,
        previous_session_file: None,
    };
    let handler = harness.extension.read().handlers.session_start[0].clone();
    handler.call(event, harness.context()).await.unwrap();
}

#[test]
fn find_next_plan_item_skips_checked_items() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("PLAN.md");
    fs::write(
        &path,
        [
            "- [x] Done item",
            "* [X] Also done",
            "- [ ] First open item",
            "* [ ] Second open item",
        ]
        .join("\n"),
    )
    .unwrap();
    assert_eq!(
        pi_coding_agent::bundled_extensions::run_plan::find_next_plan_item(&JsString::from(
            path.to_str().expect("utf-8 test path")
        ))
        .unwrap(),
        Some(JsString::from("First open item"))
    );

    fs::write(&path, "- [x] Done item\n* [X] Also done\n").unwrap();
    assert_eq!(
        pi_coding_agent::bundled_extensions::run_plan::find_next_plan_item(&JsString::from(
            path.to_str().expect("utf-8 test path")
        ))
        .unwrap(),
        None
    );
}

// Shared support is compiled once per integration target.
#[path = "support/bundled_extensions.rs"]
pub mod shared_bundled_extensions;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
