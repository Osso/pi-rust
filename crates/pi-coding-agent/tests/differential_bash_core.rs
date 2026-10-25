use crate::shared_oracle as oracle;
use pi_agent_core::agent::{AgentTool, ToolError};
use pi_ai::request::BoxFuture;
use pi_coding_agent::core::tools::bash::{
    BashDetachedResult, BashExecOptions, BashExecResult, BashOperations, BashSpawnContext, BashTool, BashToolOptions,
};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsString, JsValue};
use std::sync::{Arc, Mutex};
const ENV_NAMES: [&str; 5] = [
    "PI_SESSION_ID",
    "PI_SESSION_FILE",
    "PI_PROVIDER",
    "PI_MODEL",
    "PI_REASONING_LEVEL",
];
struct Operations {
    case: JsValue,
    calls: Mutex<Vec<JsValue>>,
}
impl BashOperations for Operations {
    fn exec<'a>(
        &'a self,
        command: &'a JsString,
        cwd: &'a JsString,
        options: BashExecOptions,
    ) -> BoxFuture<'a, Result<BashExecResult, ToolError>> {
        Box::pin(async move {
            let mut call = json!({"command":command,"cwd":cwd,"timeout":options.timeout.map(pi_js_compat::number::js_number_to_string).unwrap_or_else(||"undefined".to_owned())});
            if self.case["recordEnv"] == JsValue::Bool(true) {
                call["env"] = JsValue::Object(
                    ENV_NAMES
                        .iter()
                        .filter_map(|name| {
                            options
                                .env
                                .as_ref()
                                .expect("tool passes env")
                                .get(&JsString::from(*name))
                                .and_then(Option::as_ref)
                                .map(|value| ((*name).into(), json!(value)))
                        })
                        .collect(),
                );
            }
            self.calls.lock().unwrap().push(call);
            if let Some(chunks) = self.case["chunks"].as_array() {
                for chunk in chunks {
                    let bytes: Vec<_> = chunk
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|value| value.as_u64().unwrap() as u8)
                        .collect();
                    (options.on_data)(&bytes).map_err(|error| pi_agent_core::agent::ToolError::new(error.message))?;
                }
            }
            if let Some(error) = self.case.get("error").and_then(JsValue::as_string) {
                return Err(ToolError::new(error.clone()));
            }
            let detached = self.case.get("detached").map(|value| BashDetachedResult {
                job_id: value["jobId"].as_string().unwrap().clone(),
                message: value["message"].as_string().unwrap().clone(),
                log_path: value.get("logPath").and_then(JsValue::as_string).cloned(),
            });
            Ok(BashExecResult {
                exit_code: match self.case.get("exitNumber").and_then(JsValue::as_string) {
                    Some(number) => Some(pi_js_compat::number::parse_number(&number.to_utf8().unwrap())),
                    None => match self.case.get("exitCode") {
                        None => Some(0.0),
                        Some(JsValue::Null) => None,
                        Some(value) => Some(value.as_f64().unwrap()),
                    },
                },
                detached,
            })
        })
    }
}
fn cases() -> Vec<JsValue> {
    let raw = JsString::from_utf16(vec![0xd800]);
    vec![
        json!({"command":"plain"}),
        json!({"command":"empty-prefix","prefix":"","chunks":[b"hello\n".to_vec()]}),
        json!({"command":JsString::from("command-")+&raw,"prefix":JsString::from("prefix-")+&raw}),
        json!({"command":"original","hook":{"command":"hooked","cwd":"/hook/cwd","env":{"MARK":"set"}}}),
        json!({"command":"environment","recordEnv":true}),
        json!({"command":"error","error":JsString::from("ordinary-")+&raw}),
        json!({"command":"abort","error":"aborted"}),
        json!({"command":"timeout","error":"timeout:5:extra","chunks":[b"before\n".to_vec()]}),
        json!({"command":"timeout-empty","error":"timeout:"}),
        json!({"command":"failure","exitCode":3}),
        json!({"command":"null","exitCode":null,"chunks":[b"data".to_vec()]}),
        json!({"command":"bytes","chunks":[[240,159],[152,128]]}),
        json!({"command":"detach","detached":{"jobId":"job","message":"started"}}),
        json!({"command":"detach-empty-path","detached":{"jobId":"job","message":"started","logPath":""}}),
        json!({"command":"nan","number":"NaN"}),
        json!({"command":"infinity","number":"Infinity"}),
        json!({"command":"negative","timeout":-1}),
        json!({"command":"overridden","prefix":"setup","hook":{"command":"","cwd":"/hook/empty"}}),
        json!({"command":"exit-fraction","exitCode":1.5}),
        json!({"command":"exit-negative-fraction","exitCode":-1.5}),
        json!({"command":"exit-beyond-i32","exitCode":2147483648u64}),
        json!({"command":"exit-exponent","exitCode":1e21}),
        json!({"command":"exit-nan","exitNumber":"NaN"}),
        json!({"command":"exit-infinity","exitNumber":"Infinity"}),
        json!({"command":"exit-negative-infinity","exitNumber":"-Infinity"}),
        json!({"command":"exit-negative-zero","exitNumber":"-0"}),
    ]
}
async fn execute_case(case: &JsValue) -> JsValue {
    let operations = Arc::new(Operations {
        case: case.clone(),
        calls: Mutex::new(Vec::new()),
    });
    let updates = Arc::new(Mutex::new(Vec::new()));
    let captured = updates.clone();
    let hook = case.get("hook").cloned().map(|hook| {
        Arc::new(move |mut context: BashSpawnContext| {
            if let Some(command) = hook.get("command").and_then(JsValue::as_string) {
                context.command = command.clone();
            }
            if let Some(cwd) = hook.get("cwd").and_then(JsValue::as_string) {
                context.cwd = cwd.clone();
            }
            if let Some(env) = hook.get("env").and_then(JsValue::as_object) {
                for (name, value) in env {
                    context
                        .env
                        .insert(name.clone(), Some(value.as_string().unwrap().clone()));
                }
            }
            Ok(context)
        }) as pi_coding_agent::core::tools::bash::BashSpawnHook
    });
    let tool = BashTool::with_options(
        "/repo",
        BashToolOptions {
            operations: Some(operations.clone()),
            command_prefix: case.get("prefix").and_then(JsValue::as_string).cloned(),
            spawn_hook: hook,
            ..Default::default()
        },
    );
    let mut params = json!({"command":case["command"]});
    if let Some(number) = case.get("number").and_then(JsValue::as_string) {
        params["timeout"] = JsValue::Number(match number.to_utf8().unwrap().as_str() {
            "NaN" => f64::NAN,
            "Infinity" => f64::INFINITY,
            _ => unreachable!(),
        });
    } else if let Some(timeout) = case.get("timeout") {
        params["timeout"] = timeout.clone();
    }
    let result = match tool
        .execute(
            "bash-core".into(),
            params,
            None,
            Arc::new(move |value| {
                captured.lock().unwrap().push(js_json::to_value(&value).unwrap());
                Ok(())
            }),
            None,
        )
        .await
    {
        Ok(value) => json!({"ok":true,"value":js_json::to_value(&value).unwrap()}),
        Err(error) => json!({"ok":false,"error":error.message}),
    };
    json!({"result":result,"calls":operations.calls.lock().unwrap().clone(),"updates":updates.lock().unwrap().clone()})
}
#[tokio::test]
async fn bash_executor_preserves_custom_binary64_exit_values() {
    let cases = vec![
        JsValue::Null,
        json!(0),
        json!(1.5),
        json!(-1.5),
        json!(2147483648u64),
        json!(1e21),
        json!("NaN"),
        json!("Infinity"),
        json!("-Infinity"),
        json!("-0"),
    ];
    let mut source = oracle::TsOracle::spawn("bash-core-oracle.ts", "packages/coding-agent/src/core/tools/bash.ts");
    let expected: JsValue = source.request(&json!({"id":2,"operation":"executorExit","cases":cases}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    assert_eq!(expected["output"].as_array().unwrap().len(), cases.len());
    for (index, value) in cases.into_iter().enumerate() {
        let case = if value.as_string().is_some() {
            json!({"exitNumber":value})
        } else {
            json!({"exitCode":value})
        };
        let operations = Arc::new(Operations {
            case,
            calls: Mutex::new(Vec::new()),
        });
        let result = pi_coding_agent::core::bash_executor::execute_bash_with_operations(
            &"number".into(),
            &"/repo".into(),
            operations,
            &Default::default(),
        )
        .await
        .unwrap();
        let actual = json!({"exitCode":result.exit_code.map(pi_js_compat::number::js_number_to_string).unwrap_or_else(||"undefined".into()),"negativeZero":result.exit_code.is_some_and(|code|code==0.0&&code.is_sign_negative()),"output":result.output,"cancelled":result.cancelled,"truncated":result.truncated});
        assert_eq!(actual, expected["output"][index], "executor exit {index}");
    }
}

#[tokio::test]
async fn bash_custom_execution_context_status_and_update_boundaries_match_frozen() {
    let cases = cases();
    let mut source = oracle::TsOracle::spawn("bash-core-oracle.ts", "packages/coding-agent/src/core/tools/bash.ts");
    let expected: JsValue = source.request(&json!({"id":1,"cases":cases}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    assert_eq!(expected["output"].as_array().unwrap().len(), cases.len());
    let original: Vec<_> = ENV_NAMES
        .iter()
        .map(|name| (*name, pi_js_compat::process_env::get(name)))
        .collect();
    for name in ENV_NAMES {
        pi_js_compat::process_env::set(name, &format!("inherited-{name}"));
    }
    let mut failures = Vec::new();
    for (index, case) in cases.iter().enumerate() {
        let actual = execute_case(case).await;
        if actual != expected["output"][index] {
            failures.push(format!(
                "case{index}: {} != {}",
                js_json::stringify_lossless(&actual),
                js_json::stringify_lossless(&expected["output"][index])
            ));
        }
    }
    for (name, value) in original {
        match value {
            Some(value) => pi_js_compat::process_env::set(name, &value),
            None => pi_js_compat::process_env::remove(name),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
