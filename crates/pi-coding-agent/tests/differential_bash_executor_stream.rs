use crate::shared_oracle as oracle;
use pi_agent_core::agent::ToolError;
use pi_ai::request::BoxFuture;
use pi_coding_agent::core::bash_executor::{BashExecutorOptions, execute_bash_with_operations};
use pi_coding_agent::core::tools::bash::{BashExecOptions, BashExecResult, BashOperations};
use pi_js_compat::abort::AbortController;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsString, JsValue};
use std::sync::{Arc, Mutex};

struct Operations {
    input: JsValue,
    calls: Mutex<Vec<JsValue>>,
    controller: AbortController,
}
impl BashOperations for Operations {
    fn exec<'a>(
        &'a self,
        command: &'a JsString,
        cwd: &'a JsString,
        options: BashExecOptions,
    ) -> BoxFuture<'a, Result<BashExecResult, ToolError>> {
        Box::pin(async move {
            self.calls.lock().unwrap().push(json!({"command":command,"cwd":cwd,"hasEnv":options.env.is_some(),"hasTimeout":options.timeout.is_some(),"hasDetach":options.detach.is_some()}));
            for chunk in self.input["chunks"].as_array().unwrap() {
                let bytes: Vec<u8> = chunk
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|value| value.as_u64().unwrap() as u8)
                    .collect();
                (options.on_data)(&bytes).map_err(|error| pi_agent_core::agent::ToolError::new(error.message))?;
            }
            if self.input["abort"] == JsValue::Bool(true) {
                self.controller.abort(None);
            }
            if let Some(error) = self.input["error"].as_string() {
                return Err(ToolError::new(error.clone()));
            }
            Ok(BashExecResult {
                exit_code: Some(0.0),
                detached: None,
            })
        })
    }
}

// Frozen bash-executor.ts:77-149: stream TextDecoder, sanitize/callback, and
// custom exec options absence remain observable independently of the tool.
#[tokio::test]
async fn executor_stream_decoding_and_optional_environment_match_frozen() {
    let cases = vec![
        json!({"chunks":[]}),
        json!({"chunks":[[239,187,191,65]]}),
        json!({"chunks":[[],[239],[187],[191],[65],[239,187,191,66]]}),
        json!({"chunks":[[240,159],[152,128],[13,10]]}),
        json!({"chunks":[[226],[65],[255]]}),
        json!({"chunks":[[226,130]]}),
        json!({"chunks":[b"\x1b[31mred\x1b[0m\r\n\0".to_vec()]}),
        json!({"chunks":[b"before".to_vec()],"error":"failure"}),
        json!({"chunks":[b"before".to_vec()],"abort":true,"error":"failure"}),
        json!({"chunks":[b"before".to_vec()],"abort":true}),
    ];
    let mut source = oracle::TsOracle::spawn(
        "bash-executor-stream-oracle.ts",
        "packages/coding-agent/src/core/tools/bash.ts",
    );
    let expected = source.request(&json!({"id":28052,"cases":cases}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    let mut failures = Vec::new();
    for (index, input) in cases.into_iter().enumerate() {
        let controller = AbortController::new();
        let operations = Arc::new(Operations {
            input,
            calls: Mutex::new(Vec::new()),
            controller: controller.clone(),
        });
        let chunks = Arc::new(Mutex::new(Vec::new()));
        let captured = chunks.clone();
        let options = BashExecutorOptions {
            signal: Some(controller.signal()),
            on_chunk: Some(Arc::new(move |text| {
                captured.lock().unwrap().push(text.clone());
                Ok(())
            })),
        };
        let result =
            match execute_bash_with_operations(&"command".into(), &"/cwd".into(), operations.clone(), &options).await {
                Ok(value) => json!({"ok":true,"value":js_json::to_value(&value).unwrap()}),
                Err(error) => json!({"ok":false,"error":error.message}),
            };
        let actual = json!({"result":result,"chunks":chunks.lock().unwrap().clone(),"calls":operations.calls.lock().unwrap().clone()});
        if actual != expected["output"][index] {
            failures.push(json!({"case":index,"actual":actual,"expected":expected["output"][index]}));
        }
    }
    assert!(failures.is_empty(), "{}", js_json::stringify_lossless(&json!(failures)));
}
