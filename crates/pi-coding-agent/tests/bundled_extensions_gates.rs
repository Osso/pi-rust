//! Differential coverage for the `safe` and `approval-controls` bundled extensions.

#[path = "support/bundled_extension_commands.rs"]
mod command_support;
#[path = "support/bundled_extension_load.rs"]
mod load_support;
#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/bundled_extensions.rs"]
mod support;

use std::sync::Arc;

use pi_coding_agent::core::extensions::types::{ExtensionContext, ToolCallEvent, ToolName};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::JsValue as Value;
use support::{Harness, RecordingUi, calls_json};

fn oracle_for(module: &str) -> oracle::TsOracle {
    oracle::TsOracle::spawn("bundled-extensions-oracle.ts", module)
}

fn tool_call(tool_name: &str) -> ToolCallEvent {
    ToolCallEvent {
        tool_name: ToolName::parse(tool_name),
        tool_call_id: format!("{tool_name}-call"),
        bypass_permissions: false,
        input: pi_js_compat::js_json::JsObject::new(),
    }
}

async fn gate(harness: &Harness, tool_name: &str) -> Value {
    let ctx: ExtensionContext = harness.context();
    let gate = harness.extension.read().tool_gates[0].clone();
    let result = gate.call(tool_call(tool_name), ctx).await.unwrap();
    match result {
        Some(result) => json!(result),
        None => Value::Null,
    }
}

#[tokio::test]
async fn safe_matches_typescript() {
    let mut oracle = oracle_for("packages/coding-agent/extensions/safe/src/index.ts");
    let response = oracle.request(&json!({
        "id": 1,
        "extension": "safe",
        "steps": [
            {"op": "registrations"},
            {"op": "completions", "name": "safe", "prefix": "o"},
            {"op": "completions", "name": "safe", "prefix": "s"},
            {"op": "completions", "name": "safe", "prefix": "x"},
            {"op": "command", "name": "safe", "args": "on"},
            {"op": "gate", "toolName": "web_search"},
            {"op": "gate", "toolName": "ask_questions"},
            {"op": "gate", "toolName": "bash"},
            {"op": "gate", "toolName": "read"},
            {"op": "gate", "toolName": "pyrun_eval"},
            {"op": "gate", "toolName": "custom_tool"},
            {"op": "command", "name": "safe", "args": "off"},
            {"op": "gate", "toolName": "bash"},
            {"op": "command", "name": "safe", "args": "status"},
            {"op": "command", "name": "safe", "args": "bogus"}
        ]
    }));
    assert_eq!(
        response["ok"],
        pi_js_compat::js_json::JsValue::Bool(true),
        "{response:?}"
    );
    let steps = response["output"]["steps"].as_array().unwrap();

    let harness = Harness::load("/tmp", Arc::new(RecordingUi::default()), |api| {
        pi_coding_agent::bundled_extensions::safe::safe_extension(api)
    })
    .await;

    assert_eq!(harness.registrations(), steps[0]["result"]);
    let completions = harness
        .command("safe")
        .get_argument_completions
        .expect("safe completions");
    for (index, prefix) in ["o", "s", "x"].iter().enumerate() {
        let actual = pi_js_compat::js_json!(completions(&pi_js_compat::js_json::JsString::from(*prefix)).unwrap());
        assert_eq!(actual, steps[index + 1]["result"], "completions {prefix}");
    }

    harness.run_command("safe", "on").await.unwrap();
    assert_eq!(calls_json(&harness.take_calls()), steps[4]["calls"]);

    for (index, tool_name) in [
        "web_search",
        "ask_questions",
        "bash",
        "read",
        "pyrun_eval",
        "custom_tool",
    ]
    .iter()
    .enumerate()
    {
        assert_eq!(
            gate(&harness, tool_name).await,
            steps[index + 5]["result"],
            "gate {tool_name}"
        );
        assert_eq!(calls_json(&harness.take_calls()), steps[index + 5]["calls"]);
    }

    harness.run_command("safe", "off").await.unwrap();
    assert_eq!(calls_json(&harness.take_calls()), steps[11]["calls"]);
    assert_eq!(gate(&harness, "bash").await, steps[12]["result"]);
    assert_eq!(calls_json(&harness.take_calls()), steps[12]["calls"]);

    harness.run_command("safe", "status").await.unwrap();
    assert_eq!(calls_json(&harness.take_calls()), steps[13]["calls"]);
    harness.run_command("safe", "bogus").await.unwrap();
    assert_eq!(calls_json(&harness.take_calls()), steps[14]["calls"]);
}

#[tokio::test]
async fn approval_controls_matches_typescript() {
    pi_coding_agent::modes::interactive::theme::init_theme(Some(&"dark".into()), false).unwrap();
    let mut oracle = oracle_for("packages/coding-agent/extensions/approval-controls/src/index.ts");
    let args = [
        "",
        "read-only session",
        "inherit session",
        "full-access global",
        "inherit project",
        "read-only",
        "bogus session",
    ];
    let mut steps = vec![json!({"op": "registrations"})];
    for arg in args {
        steps.push(json!({"op": "command", "name": "sandbox", "args": arg}));
    }
    steps.push(json!({"op": "command", "name": "approvals", "args": ""}));
    let response = oracle.request(&json!({
        "id": 2,
        "extension": "approval-controls",
        "steps": steps
    }));
    assert_eq!(
        response["ok"],
        pi_js_compat::js_json::JsValue::Bool(true),
        "{response:?}"
    );
    let steps = response["output"]["steps"].as_array().unwrap();

    use pi_coding_agent::core::{
        auth_storage::AuthStorage,
        extensions::{ExtensionRunner, create_extension_runtime},
        model_registry::ModelRegistry,
        session_manager::SessionManager,
        settings_manager::SettingsManager,
    };
    use std::sync::Mutex;
    let ui = Arc::new(RecordingUi::default());
    let mut harness = Harness::load("/tmp", ui.clone(), |api| {
        pi_coding_agent::bundled_extensions::approval_controls::approval_controls_extension(api)
    })
    .await;
    assert_eq!(harness.registrations(), steps[0]["result"]);
    let settings = Arc::new(Mutex::new(SettingsManager::in_memory(
        Default::default(),
        Default::default(),
    )));
    harness.runner = ExtensionRunner::new(
        vec![harness.extension.clone()],
        create_extension_runtime(),
        "/tmp",
        Arc::new(Mutex::new(
            SessionManager::in_memory(Some(&"/tmp".into()), None).unwrap(),
        )),
        Arc::new(ModelRegistry::in_memory(AuthStorage::in_memory(serde_json::Map::new())).unwrap()),
        Some(settings.clone()),
    );
    harness
        .runner
        .bind_command_context(Some(support::command_actions(&harness.calls)));
    harness
        .runner
        .set_ui_context(Some(ui), pi_coding_agent::core::extensions::types::ExtensionMode::Tui);
    let actions = args
        .iter()
        .map(|arg| json!({"name":"sandbox","args":arg}))
        .chain(std::iter::once(json!({"name":"approvals","args":""})))
        .collect::<Vec<_>>();
    let mut actual_runner_source = oracle::TsOracle::spawn(
        "extensions-boundary-oracle.ts",
        "packages/coding-agent/src/core/extensions/runner.ts",
    );
    let expected = actual_runner_source.request(&json!({"id":3,"operation":"approval_commands","actions":actions}));
    for (index, action) in actions.iter().enumerate() {
        let name = action["name"].as_string().unwrap().to_utf8().unwrap();
        let args = action["args"].as_string().unwrap().to_utf8().unwrap();
        let error = harness.run_command(&name, &args).await.err().map(|error| error.message);
        let calls = harness.take_calls();
        assert_eq!(
            json!({"error":error,"notify":calls.notify,"status":calls.set_status,"editor":calls.set_editor_text,"sandboxSelector":calls.show_sandbox_selector,"approvalSelector":calls.show_approval_selector,"profile":settings.lock().unwrap().get_explicit_sandbox_profile()}),
            expected["output"][index],
            "{name} {args}"
        );
    }
}
