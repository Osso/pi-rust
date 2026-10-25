use crate::shared_oracle as oracle;

use pi_agent_core::agent::AgentTool;
use pi_ai::request::BoxFuture;
use pi_coding_agent::core::tools::find::{FindOperations, FindTool, FindToolOptions};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsString, JsValue};
use std::io;
use std::sync::{Arc, Mutex};

fn number_cases() -> Vec<JsValue> {
    vec![
        json!({}),
        json!({"limit":0}),
        json!({"number":"-0"}),
        json!({"limit":1}),
        json!({"limit":0.1}),
        json!({"limit":1.5}),
        json!({"limit":-1}),
        json!({"limit":-0.5}),
        json!({"number":"NaN"}),
        json!({"number":"Infinity"}),
        json!({"number":"-Infinity"}),
        json!({"limit":5e-324}),
        json!({"limit":-5e-324}),
        json!({"limit":3.5}),
        json!({"limit":3}),
        json!({"limit":4}),
        json!({"limit":1e20}),
    ]
}

fn number_args(case: &JsValue) -> JsValue {
    let mut args = json!({"pattern":"*.ts","path":"list"});
    let number = match case.get("number").and_then(JsValue::as_string) {
        Some(number) => Some(match number.to_utf8().unwrap().as_str() {
            "NaN" => f64::NAN,
            "Infinity" => f64::INFINITY,
            "-Infinity" => f64::NEG_INFINITY,
            "-0" => -0.0,
            _ => unreachable!("controlled numeric fixture"),
        }),
        None => case.get("limit").and_then(JsValue::as_f64),
    };
    if let Some(number) = number {
        args.as_object_mut()
            .unwrap()
            .insert("limit".into(), JsValue::Number(number));
    }
    args
}

#[derive(Default)]
struct Operations {
    calls: Mutex<Vec<JsValue>>,
}

impl FindOperations for Operations {
    fn supports_glob(&self) -> bool {
        true
    }

    fn exists<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, io::Result<bool>> {
        Box::pin(async move {
            self.calls.lock().unwrap().push(json!(["exists", path]));
            Ok(true)
        })
    }

    fn glob<'a>(
        &'a self,
        pattern: &'a JsString,
        cwd: &'a JsString,
        ignore: &'a [JsString],
        limit: f64,
    ) -> BoxFuture<'a, io::Result<Vec<JsString>>> {
        Box::pin(async move {
            self.calls.lock().unwrap().push(json!([
                "glob",
                pattern,
                cwd,
                ignore,
                pi_js_compat::number::js_number_to_string(limit)
            ]));
            Ok(["/repo/list/alpha", "/repo/list/beta", "/repo/list/gamma"]
                .map(JsString::from)
                .to_vec())
        })
    }
}

#[tokio::test]
async fn injected_find_preserves_binary64_glob_limits_and_result_notices() {
    let cases = number_cases();
    let mut source = oracle::TsOracle::spawn("find-numbers-oracle.ts", "packages/coding-agent/src/core/tools/find.ts");
    let expected: JsValue = source.request(&json!({"id":1,"cases":cases}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    assert_eq!(expected["output"].as_array().unwrap().len(), cases.len());
    let mut failures = Vec::new();
    for (index, case) in cases.iter().enumerate() {
        let operations = Arc::new(Operations::default());
        let tool = FindTool::with_options(
            "/repo",
            FindToolOptions {
                operations: Some(operations.clone()),
            },
        );
        let result = tool
            .execute(
                "find-numbers".into(),
                number_args(case),
                None,
                Arc::new(|_| Ok(())),
                None,
            )
            .await
            .unwrap();
        let actual =
            json!({"calls":operations.calls.lock().unwrap().clone(),"result":js_json::to_value(&result).unwrap()});
        if actual != expected["output"][index] {
            failures.push(format!(
                "case {index}: {} != {}",
                js_json::stringify_lossless(&actual),
                js_json::stringify_lossless(&expected["output"][index])
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
