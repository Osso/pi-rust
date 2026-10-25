use crate::shared_oracle as oracle;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{JsString, JsValue};

fn cases() -> Vec<JsValue> {
    let raw = JsString::from("failure-") + JsString::from_utf16(vec![0xd800]);
    vec![
        json!({"kind":"hook","failure":"hook-failed"}),
        json!({"kind":"hook","failure":raw}),
        json!({"kind":"chunk","failure":"chunk-failed"}),
        json!({"kind":"chunk","failure":raw}),
        json!({"kind":"chunk","failure":raw,"abort":true}),
    ]
}
fn source_contract() -> JsValue {
    let mut oracle = oracle::TsOracle::spawn(
        "bash-hook-chunk-errors-oracle.ts",
        "packages/coding-agent/src/core/tools/bash.ts",
    );
    let expected = oracle.request(&json!({"id":1,"operation":"callback-failure","cases":cases()}));
    assert_eq!(expected["ok"], json!(true), "{expected:?}");
    assert_eq!(
        expected["output"][0],
        json!({"result":{"ok":false,"error":"hook-failed"},"trace":["hook"]})
    );
    assert_eq!(
        expected["output"][2],
        json!({"result":{"ok":false,"error":"chunk-failed"},"trace":["before-data","chunk:payload"]})
    );
    assert_eq!(
        expected["output"][4],
        json!({"result":{"ok":true,"value":{"output":"payload","cancelled":true,"truncated":false}},"trace":["before-data","chunk:payload"]})
    );
    expected["output"].clone()
}
#[test]
fn frozen_hook_and_chunk_errors_have_an_ordinary_rejection_boundary() {
    source_contract();
}

use pi_agent_core::agent::{AgentTool, ToolError};
use pi_ai::request::BoxFuture;
use pi_coding_agent::core::bash_executor::{BashExecutorOptions, execute_bash_with_operations};
use pi_coding_agent::core::tools::bash::{
    BashExecOptions, BashExecResult, BashOperations, BashSpawnHook, BashTool, BashToolOptions,
};
use pi_js_compat::abort::AbortController;
use pi_js_compat::callback_error::CallbackError;
use std::sync::{Arc, Mutex};
struct Operations(Arc<Mutex<Vec<JsString>>>);
impl BashOperations for Operations {
    fn exec<'a>(
        &'a self,
        _: &'a JsString,
        _: &'a JsString,
        options: BashExecOptions,
    ) -> BoxFuture<'a, Result<BashExecResult, ToolError>> {
        Box::pin(async move {
            self.0.lock().unwrap().push("before-data".into());
            (options.on_data)(b"payload").map_err(|error| ToolError::new(error.message))?;
            self.0.lock().unwrap().push("after-data".into());
            Ok(BashExecResult {
                exit_code: Some(0.0),
                detached: None,
            })
        })
    }
}
#[tokio::test]
async fn native_fallible_hook_and_chunk_preserve_source_failures_and_abort() {
    let expected = source_contract();
    let mut actual = Vec::new();
    for input in cases() {
        let trace = Arc::new(Mutex::new(Vec::<JsString>::new()));
        let sink = trace.clone();
        let failure = input["failure"].as_string().unwrap().clone();
        let operations = Arc::new(Operations(trace.clone()));
        let result = if input["kind"] == json!("hook") {
            let hook: BashSpawnHook = Arc::new(move |_| {
                sink.lock().unwrap().push("hook".into());
                Err(CallbackError::new(failure.clone()))
            });
            let tool = BashTool::with_options(
                "/repo",
                BashToolOptions {
                    operations: Some(operations),
                    spawn_hook: Some(hook),
                    ..Default::default()
                },
            );
            let updates = trace.clone();
            match tool
                .execute(
                    "hook".into(),
                    json!({"command":"probe"}),
                    None,
                    Arc::new(move |_| {
                        updates.lock().unwrap().push("update".into());
                        Ok(())
                    }),
                    None,
                )
                .await
            {
                Ok(value) => json!({"ok":true,"value":value}),
                Err(error) => json!({"ok":false,"error":error.message}),
            }
        } else {
            let controller = AbortController::new();
            let signal = controller.signal();
            let abort = input["abort"] == JsValue::Bool(true);
            let options = BashExecutorOptions {
                signal: Some(signal),
                on_chunk: Some(Arc::new(move |text| {
                    sink.lock().unwrap().push(JsString::from("chunk:") + text);
                    if abort {
                        controller.abort(None);
                    }
                    Err(CallbackError::new(failure.clone()))
                })),
            };
            match execute_bash_with_operations(&"probe".into(), &"/repo".into(), operations, &options).await {
                Ok(value) => json!({"ok":true,"value":value}),
                Err(error) => json!({"ok":false,"error":error.message}),
            }
        };
        actual.push(json!({"result":result,"trace":trace.lock().unwrap().clone()}));
    }
    assert_eq!(json!(actual), expected);
}

#[test]
fn frozen_lifecycle_getter_rejects_before_updates_or_execution() {
    let mut source = oracle::TsOracle::spawn(
        "bash-hook-chunk-errors-oracle.ts",
        "packages/coding-agent/src/core/tools/bash.ts",
    );
    let failure = JsString::from("lifecycle-") + JsString::from_utf16(vec![0xd800]);
    let result = source
        .request(&json!({"id":2,"operation":"lifecycle-failure","cases":[{"kind":"lifecycle","failure":failure}]}));
    assert_eq!(result["ok"], json!(true), "{result:?}");
    assert_eq!(
        result["output"][0],
        json!({"result":{"ok":false,"error":failure},"trace":["lifecycle"]})
    );
}

#[tokio::test]
async fn native_lifecycle_getter_rejects_before_updates_or_execution() {
    use pi_coding_agent::core::tools::bash::{BashBackgroundJobs, BashGetLifecycle};
    let trace = Arc::new(Mutex::new(Vec::<JsString>::new()));
    let sink = trace.clone();
    let failure = JsString::from("lifecycle-") + JsString::from_utf16(vec![0xd800]);
    let callback_error = failure.clone();
    let get_lifecycle: BashGetLifecycle = Arc::new(move || {
        sink.lock().unwrap().push("lifecycle".into());
        Err(CallbackError::new(callback_error.clone()))
    });
    let tool = BashTool::with_options(
        "/repo",
        BashToolOptions {
            operations: Some(Arc::new(Operations(trace.clone()))),
            background_jobs: Some(Arc::new(BashBackgroundJobs {
                get_lifecycle: Some(get_lifecycle),
                lifecycle: None,
            })),
            ..Default::default()
        },
    );
    let updates = trace.clone();
    let result = tool
        .execute(
            "lifecycle".into(),
            json!({"command":"probe"}),
            None,
            Arc::new(move |_| {
                updates.lock().unwrap().push("update".into());
                Ok(())
            }),
            None,
        )
        .await;
    assert_eq!(result.unwrap_err().message, failure);
    assert_eq!(*trace.lock().unwrap(), vec![JsString::from("lifecycle")]);
}
