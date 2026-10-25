//! Differential coverage for the `ask-secret` and `browser-cli` bundled extensions.

use pi_js_compat::js_json::JsString;
#[path = "support/bundled_extension_load.rs"]
mod load_support;
#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/bundled_extensions.rs"]
mod support;

use std::sync::Arc;

use pi_coding_agent::bundled_extensions::ask_secret::{AskSecretToolOptions, create_ask_secret_tool_definition};
use pi_coding_agent::bundled_extensions::browser_cli::{
    BrowserCliCommandResult, BrowserCliCommandRunner, BrowserCliExtensionOptions, create_browser_cli_tool_definition,
};
use pi_js_compat::js_json as json;
use support::{Harness, RecordingUi, calls_json};

fn oracle_for(module: &str) -> oracle::TsOracle {
    oracle::TsOracle::spawn("bundled-extensions-oracle.ts", module)
}

fn browser_params(record: &str) -> pi_js_compat::js_json::JsValue {
    json!({"record":record,"fields":[
        {"type":"email","name":"Username","selector":"#usernameInputField"},
        {"type":"password","name":"Password","selector":"#pwInputField"}
    ]})
}

fn valid_params() -> pi_js_compat::js_json::JsValue {
    browser_params("browser/rei-capital-one.json")
}

#[tokio::test]
async fn ask_secret_matches_typescript() {
    let mut oracle = oracle_for("packages/coding-agent/extensions/ask-secret/src/index.ts");
    let response = oracle.request(&json!({
        "id": 1,
        "extension": "ask-secret",
        "steps": [
            {"op": "registrations"},
            {"op": "tool", "name": "ask_secret", "params": valid_params(), "mode": "tui", "hasUI": true,
             "inputs": ["rei-user", "super-secret"]},
            {"op": "tool", "name": "ask_secret", "params": valid_params(), "mode": "rpc", "hasUI": false},
            {"op": "tool", "name": "ask_secret", "params": browser_params("browser/rei-capital-one.json\nleak"), "mode": "tui", "hasUI": true},
            {"op": "tool", "name": "ask_secret", "params": valid_params(), "mode": "tui", "hasUI": true,
             "inputs": [null]}
        ]
    }));
    assert_eq!(
        response["ok"],
        pi_js_compat::js_json::JsValue::Bool(true),
        "{response:?}"
    );
    let steps = response["output"]["steps"].as_array().unwrap();

    let harness = Harness::load("/tmp", Arc::new(RecordingUi::default()), |api| {
        pi_coding_agent::bundled_extensions::ask_secret::ask_secret_extension(api)
    })
    .await;
    assert_eq!(harness.registrations(), steps[0]["result"]);

    let recorded = Arc::new(std::sync::Mutex::new(Vec::new()));
    let provision_calls = Arc::clone(&recorded);
    let provision_calls_for_harness = Arc::clone(&harness.calls);
    let tool = create_ask_secret_tool_definition(AskSecretToolOptions {
        provision: Some(Arc::new(move |request, values, _signal| {
            provision_calls.lock().unwrap().push((json!(request), values.clone()));
            provision_calls_for_harness
                .lock()
                .unwrap()
                .provision
                .push((json!(request), values));
            Box::pin(async { Ok(()) })
        })),
        ..Default::default()
    });
    let ui = Arc::new(RecordingUi {
        confirmed: std::sync::Mutex::new(false),
        calls: harness.calls.clone(),
        inputs: std::sync::Mutex::new(
            [JsString::from("rei-user"), JsString::from("super-secret")]
                .into_iter()
                .map(|text| Some(JsString::from(text)))
                .collect(),
        ),
    });
    let mut runner = harness.runner.clone();
    runner.set_ui_context(Some(ui), pi_coding_agent::core::extensions::types::ExtensionMode::Tui);
    let result = (tool.execute)(
        "call-1".to_owned(),
        valid_params(),
        None,
        None,
        Some(runner.create_context()),
    )
    .await
    .unwrap();
    assert_eq!(json!(result), steps[1]["result"]);
    assert_eq!(calls_json(&harness.take_calls()), steps[1]["calls"]);
    assert_eq!(
        recorded.lock().unwrap().as_slice(),
        &[(
            valid_params(),
            vec![JsString::from("rei-user"), JsString::from("super-secret")]
        )]
    );

    let non_tui = create_ask_secret_tool_definition(AskSecretToolOptions {
        provision: Some(Arc::new(|_, _, _| Box::pin(async { Ok(()) }))),
        ..Default::default()
    });
    let mut non_tui_runner = harness.runner.clone();
    non_tui_runner.set_ui_context(None, pi_coding_agent::core::extensions::types::ExtensionMode::Print);
    let result = (non_tui.execute)(
        "call-2".to_owned(),
        valid_params(),
        None,
        None,
        Some(non_tui_runner.create_context()),
    )
    .await
    .unwrap();
    assert_eq!(json!(result), steps[2]["result"]);

    let error = (non_tui.execute)(
        "call-3".to_owned(),
        browser_params("browser/rei-capital-one.json\nleak"),
        None,
        None,
        Some(harness.context()),
    )
    .await
    .unwrap_err();
    assert_eq!(
        pi_js_compat::js_json::JsValue::String(error.message),
        steps[3]["error"]["message"]
    );

    let cancelled_ui = Arc::new(RecordingUi {
        confirmed: std::sync::Mutex::new(false),
        calls: harness.calls.clone(),
        inputs: std::sync::Mutex::new([None].into_iter().collect()),
    });
    let mut runner = harness.runner.clone();
    runner.set_ui_context(
        Some(cancelled_ui),
        pi_coding_agent::core::extensions::types::ExtensionMode::Tui,
    );
    let result = (tool.execute)(
        "call-4".to_owned(),
        valid_params(),
        None,
        None,
        Some(runner.create_context()),
    )
    .await
    .unwrap();
    assert_eq!(json!(result), steps[4]["result"]);
    assert_eq!(calls_json(&harness.take_calls()), steps[4]["calls"]);
}

#[tokio::test]
async fn ask_secret_file_persistence_matches_frozen_bytes_permissions_and_ui() {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("new-parent/secret.txt");
    let params = json!({"path":destination.to_str().unwrap(),"label":"Private test secret"});
    let mut oracle = oracle_for("packages/coding-agent/extensions/ask-secret/src/index.ts");
    let response = oracle.request(&json!({"id":11,"extension":"ask-secret","steps":[
        {"op":"tool","name":"ask_secret","params":params,"mode":"tui","hasUI":true,"inputs":["fixture-secret 🚀"]}
    ]}));
    assert_eq!(response["ok"], json!(true));
    let expected = &response["output"]["steps"][0];
    let expected_bytes = std::fs::read(&destination).unwrap();
    assert_eq!(expected_bytes, "fixture-secret 🚀\n".as_bytes());
    let expected_mode = std::fs::metadata(&destination).unwrap().permissions().mode() & 0o777;
    assert_eq!(expected_mode, 0o600);
    assert_eq!(
        std::fs::metadata(destination.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    std::fs::remove_dir_all(destination.parent().unwrap()).unwrap();

    let ui = Arc::new(RecordingUi::default());
    ui.inputs.lock().unwrap().push_back(Some("fixture-secret 🚀".into()));
    let harness = Harness::load("/tmp", ui, |api| {
        pi_coding_agent::bundled_extensions::ask_secret::ask_secret_extension(api)
    })
    .await;
    let tool = create_ask_secret_tool_definition(AskSecretToolOptions::default());
    let result = (tool.execute)("file-call".to_owned(), params, None, None, Some(harness.context()))
        .await
        .unwrap();
    assert_eq!(json!(result), expected["result"]);
    assert_eq!(calls_json(&harness.take_calls()), expected["calls"]);
    assert_eq!(std::fs::read(&destination).unwrap(), expected_bytes);
    assert_eq!(
        std::fs::metadata(&destination).unwrap().permissions().mode() & 0o777,
        expected_mode
    );
    assert_eq!(
        std::fs::metadata(destination.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(std::fs::read_dir(destination.parent().unwrap()).unwrap().count(), 1);
}

#[tokio::test]
async fn browser_cli_matches_typescript() {
    let mut oracle = oracle_for("packages/coding-agent/extensions/browser-cli/src/index.ts");
    let commands = json!([
        ["click", "[gc-testing-id='generate-report-btn']"],
        ["wait", "3000"],
        ["get", "text", "[gc-testing-id='reporting-screen']"]
    ]);
    let response = oracle.request(&json!({
        "id": 2,
        "extension": "browser-cli",
        "steps": [
            {"op": "registrations"},
            {"op": "tool", "name": "browser-cli", "params": {"commands": commands.clone()}, "runCommandResults": [
                {"stdout": "Clicked\n", "stderr": "", "exitCode": 0},
                {"stdout": "Waited\n", "stderr": "", "exitCode": 0},
                {"stdout": "Report ready\n", "stderr": "", "exitCode": 0}
            ]},
            {"op": "tool", "name": "browser-cli", "params": {"commands": [["click", "#search"], ["wait", "3000"], ["get", "title"]]},
             "runCommandResults": [
                {"stdout": "Clicked\n", "stderr": "", "exitCode": 0},
                {"stdout": "", "stderr": "denied", "exitCode": 7}
            ]}
        ]
    }));
    assert_eq!(
        response["ok"],
        pi_js_compat::js_json::JsValue::Bool(true),
        "{response:?}"
    );
    let steps = response["output"]["steps"].as_array().unwrap();

    let harness = Harness::load("/tmp", Arc::new(RecordingUi::default()), |api| {
        api.register_tool(create_browser_cli_tool_definition(BrowserCliExtensionOptions::default()))
    })
    .await;
    assert_eq!(harness.registrations(), steps[0]["result"]);

    let queue = Arc::new(std::sync::Mutex::new(vec![
        BrowserCliCommandResult {
            stdout: "Clicked\n".into(),
            stderr: JsString::default(),
            exit_code: Some(0),
        },
        BrowserCliCommandResult {
            stdout: "Waited\n".into(),
            stderr: JsString::default(),
            exit_code: Some(0),
        },
        BrowserCliCommandResult {
            stdout: "Report ready\n".into(),
            stderr: JsString::default(),
            exit_code: Some(0),
        },
    ]));
    let runner_calls = Arc::new(std::sync::Mutex::new(Vec::new()));
    let queue_for_runner = Arc::clone(&queue);
    let calls_for_runner = Arc::clone(&runner_calls);
    let harness_calls = Arc::clone(&harness.calls);
    let context = harness.context();
    let manager = context.session_manager().unwrap();
    let expected_session = pi_js_compat::js_format!(
        "{}:main",
        pi_coding_agent::core::session_manager::lock_session_manager(&manager).get_session_id()
    );
    let runner: BrowserCliCommandRunner = Arc::new(move |executable, args, session, signal| {
        assert_eq!(session, expected_session);
        calls_for_runner
            .lock()
            .unwrap()
            .push((executable.clone(), args.clone(), signal.is_some()));
        harness_calls
            .lock()
            .unwrap()
            .run_command
            .push((executable.into(), args.clone(), signal.is_some()));
        let next = queue_for_runner.lock().unwrap().remove(0);
        Box::pin(async move { Ok(next) })
    });
    let tool = create_browser_cli_tool_definition(BrowserCliExtensionOptions {
        run_command: Some(runner),
    });
    let result = (tool.execute)(
        "call-1".to_owned(),
        json!({"commands": commands}),
        None,
        None,
        Some(harness.context()),
    )
    .await
    .unwrap();
    assert_eq!(json!(result), steps[1]["result"]);
    assert_eq!(calls_json(&harness.take_calls()), steps[1]["calls"]);
    assert_eq!(
        runner_calls.lock().unwrap().as_slice(),
        &[
            (
                "browser-cli".to_owned(),
                vec![
                    JsString::from("click"),
                    JsString::from("[gc-testing-id='generate-report-btn']")
                ],
                false
            ),
            (
                "browser-cli".to_owned(),
                vec![JsString::from("wait"), JsString::from("3000")],
                false
            ),
            (
                "browser-cli".to_owned(),
                vec![
                    JsString::from("get"),
                    JsString::from("text"),
                    JsString::from("[gc-testing-id='reporting-screen']")
                ],
                false
            )
        ]
    );

    let failing_queue = Arc::new(std::sync::Mutex::new(vec![
        BrowserCliCommandResult {
            stdout: "Clicked\n".into(),
            stderr: JsString::default(),
            exit_code: Some(0),
        },
        BrowserCliCommandResult {
            stdout: JsString::default(),
            stderr: "denied".into(),
            exit_code: Some(7),
        },
    ]));
    let failing_runner: BrowserCliCommandRunner = Arc::new(move |_executable, _args, _session, _signal| {
        let next = failing_queue.lock().unwrap().remove(0);
        Box::pin(async move { Ok(next) })
    });
    let failing_tool = create_browser_cli_tool_definition(BrowserCliExtensionOptions {
        run_command: Some(failing_runner),
    });
    let error = (failing_tool.execute)(
        "call-2".to_owned(),
        json!({"commands": [["click", "#search"], ["wait", "3000"], ["get", "title"]]}),
        None,
        None,
        Some(harness.context()),
    )
    .await
    .unwrap_err();
    assert_eq!(
        pi_js_compat::js_json::JsValue::String(error.message),
        steps[2]["error"]["message"]
    );
}

#[tokio::test]
async fn browser_cli_rejects_an_aborted_command() {
    let controller_token = pi_js_compat::abort::AbortController::new();
    let token = controller_token.signal();
    let runner: BrowserCliCommandRunner = Arc::new(|_executable, _args, _session, signal| {
        Box::pin(async move {
            if let Some(signal) = signal {
                signal.wait().await;
            }
            Ok(BrowserCliCommandResult {
                stdout: "late".into(),
                stderr: JsString::default(),
                exit_code: Some(0),
            })
        })
    });
    let tool = create_browser_cli_tool_definition(BrowserCliExtensionOptions {
        run_command: Some(runner),
    });
    let harness = Harness::load("/tmp", Arc::new(RecordingUi::default()), |api| {
        api.register_tool(create_browser_cli_tool_definition(BrowserCliExtensionOptions::default()))
    })
    .await;
    let execution = tokio::spawn({
        let token = token.clone();
        async move {
            (tool.execute)(
                "call-3".to_owned(),
                json!({"commands": [["wait", "1000"], ["get", "title"]]}),
                Some(token),
                None,
                Some(harness.context()),
            )
            .await
        }
    });
    tokio::task::yield_now().await;
    controller_token.abort(None);
    let error = execution.await.unwrap().unwrap_err();
    assert_eq!(error.message, "browser-cli command aborted");
}
