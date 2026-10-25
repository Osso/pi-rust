use crate::shared_oracle as oracle;

use pi_agent_core::agent::{AgentTool, ToolError};
use pi_ai::request::BoxFuture;
use pi_coding_agent::core::tools::bash::{BashExecOptions, BashExecResult, BashOperations, BashTool, BashToolOptions};
use pi_coding_agent::utils::uncaught;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsString, JsValue};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

struct Operations(Arc<Mutex<Vec<&'static str>>>);
impl BashOperations for Operations {
    fn exec<'a>(
        &'a self,
        _: &'a JsString,
        _: &'a JsString,
        options: BashExecOptions,
    ) -> BoxFuture<'a, Result<BashExecResult, ToolError>> {
        Box::pin(async move {
            self.0.lock().unwrap().push("before-data");
            (options.on_data)(b"payload").map_err(|error| pi_agent_core::agent::ToolError::new(error.message))?;
            self.0.lock().unwrap().push("after-data");
            Ok(BashExecResult {
                exit_code: Some(0.0),
                detached: None,
            })
        })
    }
}
fn run_child(case: &JsValue, path: &std::path::Path) -> JsValue {
    std::fs::write(path, js_json::stringify_lossless(case)).unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "bash_callback_error_child", "--nocapture"])
        .env("PI_BASH_CALLBACK_CASE", path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            child.kill().unwrap();
            let output = child.wait_with_output().unwrap();
            panic!("callback child timed out: {}", String::from_utf8_lossy(&output.stderr));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let record = stdout
        .lines()
        .find_map(|line| line.strip_prefix("BASH_CALLBACK:"))
        .unwrap_or_else(|| {
            panic!(
                "missing callback record: {stdout}; {}",
                String::from_utf8_lossy(&output.stderr)
            )
        });
    let record = js_json::parse_lossless(record).unwrap();
    let expected_status = if record.get("uncaught").is_some() { 19 } else { 0 };
    assert_eq!(output.status.code(), Some(expected_status), "{record:?}");
    record
}
#[test]
fn synchronous_custom_operation_update_failure_rejects_instead_of_crashing() {
    let mut oracle = oracle::TsOracle::spawn(
        "bash-callback-error-oracle.ts",
        "packages/coding-agent/src/core/tools/bash.ts",
    );
    let cases = vec![
        JsValue::Null,
        json!("update-failed"),
        json!(JsString::from("update-") + JsString::from_utf16(vec![0xd800])),
    ];
    let expected = oracle.request(&json!({"id":1,"operation":"update-failure","cases":cases}));
    assert_eq!(expected["ok"], json!(true), "{expected:?}");
    assert_eq!(
        expected["output"][1]["result"],
        json!({"ok":false,"error":"update-failed"}),
        "{expected:?}"
    );
    let directory = tempfile::tempdir().unwrap();
    let actual: Vec<_> = cases
        .iter()
        .map(|case| run_child(case, &directory.path().join("case.json")))
        .collect();
    assert_eq!(json!(actual), expected["output"]);
}
#[test]
fn real_local_stream_update_failure_remains_uncaught() {
    let mut oracle = oracle::TsOracle::spawn(
        "bash-callback-error-oracle.ts",
        "packages/coding-agent/src/core/tools/bash.ts",
    );
    let cases = vec![
        json!("stream-failed"),
        json!(JsString::from("stream-") + JsString::from_utf16(vec![0xd800])),
    ];
    let expected = oracle.request(&json!({"id":2,"operation":"stream","cases":cases}));
    assert_eq!(expected["ok"], json!(true), "{expected:?}");
    assert_eq!(
        expected["output"][0]["uncaught"],
        json!({"name":"Error","message":"stream-failed"}),
        "{expected:?}"
    );
    let directory = tempfile::tempdir().unwrap();
    let actual: Vec<_> = cases
        .iter()
        .map(|failure| {
            run_child(
                &json!({"local":true,"failure":failure}),
                &directory.path().join("case.json"),
            )
        })
        .collect();
    assert_eq!(json!(actual), expected["output"]);
}
#[test]
#[ignore = "isolated process uncaught callback boundary"]
fn bash_callback_error_child() {
    let path = std::env::var_os("PI_BASH_CALLBACK_CASE").unwrap();
    let input = js_json::parse_lossless(String::from_utf8(std::fs::read(path).unwrap()).unwrap()).unwrap();
    let local = input["local"] == JsValue::Bool(true);
    let failure = if local { input["failure"].clone() } else { input };
    let trace = Arc::new(Mutex::new(Vec::<&'static str>::new()));
    let captured = trace.clone();
    uncaught::install_uncaught_handler();
    uncaught::set_process_crash_handler(Some(Arc::new(move |payload| {
        let error = payload
            .downcast_ref::<pi_js_compat::callback_error::CallbackError>()
            .unwrap();
        println!(
            "BASH_CALLBACK:{}",
            js_json::stringify_lossless(
                &json!({"uncaught":{"name":error.name,"message":error.message},"trace":captured.lock().unwrap().clone()})
            )
        );
        std::process::exit(19);
    })));
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let tool = BashTool::with_options(
            if local {
                pi_js_compat::process_cwd::cwd().unwrap()
            } else {
                "/repo".into()
            },
            BashToolOptions {
                operations: if local {
                    None
                } else {
                    Some(Arc::new(Operations(trace.clone())))
                },
                ..Default::default()
            },
        );
        let updates = trace.clone();
        let result = tool
            .execute(
                "callback".into(),
                json!({"command":if local { "printf payload" } else { "probe" }}),
                None,
                Arc::new(move |update| {
                    updates.lock().unwrap().push(if update.content.is_empty() {
                        "initial-update"
                    } else {
                        "data-update"
                    });
                    if !update.content.is_empty()
                        && let Some(message) = failure.as_string()
                    {
                        return Err(ToolError::new(message.clone()));
                    }
                    Ok(())
                }),
                None,
            )
            .await;
        let result = match result {
            Ok(value) => json!({"ok":true,"value":value}),
            Err(error) => json!({"ok":false,"error":error.message}),
        };
        println!(
            "BASH_CALLBACK:{}",
            js_json::stringify_lossless(&json!({"result":result,"trace":trace.lock().unwrap().clone()}))
        );
    });
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
