//! Common-pin session naming tool: registration, refusal ordering, writes, and UTF-16.
use crate::shared_bundled as bundled;
use crate::shared_oracle as oracle;

use bundled::{Harness, HarnessOptions};
use pi_coding_agent::core::extensions::{create_extension_runtime, load_extension_from_factory};
use pi_coding_agent::core::session_manager::{NewSessionOptions, SessionManager, lock_session_manager};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{JsString, JsValue as Value};
use std::sync::{Arc, Mutex};

async fn run_case(case: &Value) -> Value {
    let dir = tempfile::tempdir().expect("private directory");
    let cwd = JsString::from(dir.path().to_str().expect("scalar temporary path"));
    let session_dir = cwd.clone() + "/sessions";
    let options = Some(NewSessionOptions {
        is_subagent: case["subagent"].as_bool().unwrap_or(false),
        ..Default::default()
    });
    let mut manager = if case["persisted"].as_bool() == Some(false) {
        SessionManager::in_memory(Some(&cwd), options).expect("memory session")
    } else {
        SessionManager::create(&cwd, Some(&session_dir), options).expect("persisted session")
    };
    manager
        .set_metadata_control_db_path(Some(&(cwd.clone() + "/control.sqlite")), Default::default())
        .expect("private DB");
    if let Some(name) = case.get("initialName") {
        manager.set_session_name(name.as_string()).expect("initial name");
    }
    let manager = Arc::new(Mutex::new(manager));
    let runtime = create_extension_runtime();
    let extension = load_extension_from_factory(
        Arc::new(|pi| {
            let result = pi_coding_agent::bundled_extensions::session_autoname::session_autoname_extension(pi);
            Box::pin(async move { result })
        }),
        &cwd,
        Arc::new(pi_coding_agent::core::event_bus::EventBusController::new()),
        runtime.clone(),
        &"<first-party:session-autoname>".into(),
    )
    .await
    .expect("extension");
    let mut options = HarnessOptions::new(cwd.to_utf8().expect("scalar cwd").as_str());
    options.session_manager = manager.clone();
    options.multi_agent_agent_id = case.get("child").and_then(Value::as_string).cloned();
    let harness = Harness::new(vec![extension.clone()], runtime.clone(), options).await;
    let writes = Arc::new(Mutex::new(Vec::<JsString>::new()));
    let fail = Arc::new(Mutex::new(false));
    let mut actions = runtime.actions();
    actions.set_session_name = {
        let writes = writes.clone();
        let fail = fail.clone();
        let manager = manager.clone();
        Arc::new(move |name| {
            if *fail.lock().unwrap() {
                return Err("name write failed".into());
            }
            writes.lock().unwrap().push(name.clone());
            lock_session_manager(&manager)
                .set_session_name(Some(name))
                .map_err(|error| error.to_string().into())
        })
    };
    runtime.set_actions(actions);
    let definition = extension
        .read()
        .tools
        .get("set_session_name")
        .expect("frozen naming tool")
        .definition
        .clone();
    let metadata = json!({
        "name": definition.name, "label": definition.label, "description": definition.description,
        "promptGuidelines": definition.prompt_guidelines, "approvalRequired": definition.approval_required,
        "parameters": definition.parameters,
    });
    let mut results = Vec::new();
    for step in case["steps"].as_array().expect("steps") {
        if let Some(name) = step.get("externalName") {
            lock_session_manager(&manager)
                .set_session_name(name.as_string())
                .expect("external name");
            continue;
        }
        *fail.lock().unwrap() = step["fail"].as_bool().unwrap_or(false);
        let result = (definition.execute)(
            "name".into(),
            json!({"name": step.get("name").cloned().unwrap_or_else(|| json!(""))}),
            None,
            None,
            Some(harness.context()),
        )
        .await;
        results.push(match result {
            Ok(result) => {
                let mut value = json!({"content": result.content, "details": result.details});
                if let Some(is_error) = result.is_error {
                    value.as_object_mut().unwrap().insert("isError".into(), json!(is_error));
                }
                value
            }
            Err(error) => json!({"error": error.to_string()}),
        });
    }
    let current_name = lock_session_manager(&manager).get_session_name();
    let has_name_state = lock_session_manager(&manager).has_session_name_state();
    let writes = writes.lock().unwrap().clone();
    json!({"metadata": metadata, "results": results, "writes": writes, "currentName": current_name, "hasNameState": has_name_state})
}

#[tokio::test]
async fn session_name_tool_matches_frozen_registration_refusals_and_updates() {
    let cases = vec![
        json!({"steps": [{"name": "  Fix\r\n  the\t login  "}, {"name": "Revise title"}]}),
        json!({"steps": [{"name": "## Session Title: Keep formatting"}]}),
        json!({"steps": [{"name": "   \n\t"}]}),
        json!({"child": "child", "steps": [{"name": "Title"}]}),
        json!({"child": "", "steps": [{"name": "Title"}]}),
        json!({"subagent": true, "steps": [{"name": "Title"}]}),
        json!({"persisted": false, "steps": [{"name": "Title"}]}),
        json!({"initialName": null, "steps": [{"name": "Title"}]}),
        json!({"initialName": "Existing", "steps": [{"name": "Title"}]}),
        json!({"steps": [{"name": "One"}, {"externalName": "Manual"}, {"name": "Two"}]}),
        json!({"steps": [{"name": "One"}, {"externalName": null}, {"name": "Two"}]}),
        json!({"steps": [{"name": "First", "fail": true}, {"name": "Second"}]}),
        json!({"steps": [{"name": "a".repeat(100)}]}),
        json!({"steps": [{"name": format!("{} finalword", "word ".repeat(15))}]}),
        json!({"steps": [{"name": format!("{}😀end", "a".repeat(79))}]}),
        json!({"steps": [{"name": JsString::from_utf16(vec![0x44, 0xd800, 0x20, 0xdc00])}]}),
    ];
    let mut ts = oracle::TsOracle::spawn(
        "session-name-tool-oracle.ts",
        "packages/coding-agent/extensions/session-autoname/src/index.ts",
    );
    let response = ts.request(&json!({"id": 1, "operation": "tool", "cases": cases}));
    assert_eq!(response["ok"], json!(true), "oracle: {response:?}");
    for (index, case) in cases.iter().enumerate() {
        let actual = run_case(case).await;
        assert_eq!(actual, response["output"][index], "case {index}");
    }
}
