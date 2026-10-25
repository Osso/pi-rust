use crate::shared_oracle as oracle;

use pi_agent_core::agent::AgentTool;
use pi_ai::request::BoxFuture;
use pi_coding_agent::core::tools::read::{ReadOperations, ReadRange, ReadTool, ReadToolOptions};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{JsString, JsValue, to_value};
use std::sync::{Arc, Mutex};

struct NumericOperations {
    stat: Option<f64>,
    range: bool,
    empty: bool,
    mime: Option<String>,
    trace: Arc<Mutex<Vec<JsValue>>>,
}
impl ReadOperations for NumericOperations {
    fn read_file<'a>(&'a self, _: &'a JsString) -> BoxFuture<'a, std::io::Result<Vec<u8>>> {
        self.trace.lock().unwrap().push(json!(["read"]));
        Box::pin(async { Ok(b"one\ntwo\nthree".to_vec()) })
    }
    fn access<'a>(&'a self, _: &'a JsString) -> BoxFuture<'a, std::io::Result<()>> {
        self.trace.lock().unwrap().push(json!(["access"]));
        Box::pin(async { Ok(()) })
    }
    fn detect_image_mime_type<'a>(&'a self, _: &'a JsString) -> BoxFuture<'a, std::io::Result<Option<String>>> {
        if self.mime.is_some() {
            self.trace.lock().unwrap().push(json!(["mime"]));
        }
        Box::pin(async { Ok(self.mime.clone()) })
    }
    fn stat<'a>(&'a self, _: &'a JsString) -> BoxFuture<'a, std::io::Result<Option<f64>>> {
        if self.stat.is_some() {
            self.trace.lock().unwrap().push(json!(["stat"]));
        }
        Box::pin(async { Ok(self.stat) })
    }
    fn read_range(&self) -> Option<ReadRange<'_>> {
        if !self.range {
            return None;
        }
        Some(Box::new(move |_, start, end| {
            self.trace.lock().unwrap().push(json!(["range", start, end]));
            Box::pin(async { Ok(if self.empty { Vec::new() } else { b"head\n".to_vec() }) })
        }))
    }
}
fn numeric_spec(value: &JsValue) -> f64 {
    let text = value.as_string().unwrap().to_utf8().unwrap();
    match text.as_str() {
        "NaN" => f64::NAN,
        "Infinity" => f64::INFINITY,
        "-Infinity" => f64::NEG_INFINITY,
        _ => text.parse().unwrap(),
    }
}
fn project_nonfinite(value: JsValue) -> JsValue {
    match value {
        JsValue::Number(value) if !value.is_finite() => {
            json!({"$number": pi_js_compat::number::js_number_to_string(value)})
        }
        JsValue::Array(values) => JsValue::Array(values.into_iter().map(project_nonfinite).collect()),
        JsValue::Object(values) => JsValue::Object(
            values
                .into_iter()
                .map(|(key, value)| (key, project_nonfinite(value)))
                .collect(),
        ),
        other => other,
    }
}
#[tokio::test]
async fn read_preserves_js_number_options_and_stat_metadata() {
    let cwd = tempfile::tempdir().unwrap();
    let cases = vec![
        json!({"stat": null}),
        json!({"stat": null, "mime": ""}),
        json!({"stat": null, "hasRange": false, "limit": "-0.5"}),
        json!({"stat": null, "hasRange": false, "limit": "-5e-324"}),
        json!({"stat": "10240.5"}),
        json!({"stat": "Infinity"}),
        json!({"stat": "NaN"}),
        json!({"stat": "-Infinity"}),
        json!({"stat": "-1"}),
        json!({"stat": "20000", "emptyRange": true}),
        json!({"stat": "20000", "hasRange": false}),
        json!({"stat": "20000", "offset": "Infinity"}),
        json!({"stat": "20000", "offset": "NaN"}),
        json!({"stat": "20000", "limit": "NaN"}),
        json!({"stat": "20000", "limit": "Infinity"}),
        json!({"stat": "20000", "limit": "-Infinity"}),
        json!({"stat": "20000", "offset": "-Infinity"}),
    ];
    let mut oracle = oracle::TsOracle::spawn(
        "read-operations-oracle.ts",
        "packages/coding-agent/src/core/tools/read.ts",
    );
    let expected = oracle.request(&json!({"id": 23, "cwd": cwd.path().to_str().unwrap(), "cases": cases}));
    assert_eq!(expected["ok"], json!(true), "{expected:?}");
    for (index, case) in cases.iter().enumerate() {
        let trace = Arc::new(Mutex::new(Vec::new()));
        let operations = NumericOperations {
            stat: (!case["stat"].is_null()).then(|| numeric_spec(&case["stat"])),
            range: case.get("hasRange").and_then(JsValue::as_bool) != Some(false),
            empty: case.get("emptyRange").and_then(JsValue::as_bool) == Some(true),
            mime: case
                .get("mime")
                .and_then(JsValue::as_string)
                .map(|value| value.to_utf8().unwrap()),
            trace: Arc::clone(&trace),
        };
        let tool = ReadTool::with_options(
            cwd.path().to_str().unwrap(),
            ReadToolOptions {
                operations: Some(Arc::new(operations)),
                ..Default::default()
            },
        );
        let mut params = json!({"path": "virtual.txt"});
        for key in ["offset", "limit"] {
            if let Some(value) = case.get(key) {
                params
                    .as_object_mut()
                    .unwrap()
                    .insert(key.into(), JsValue::Number(numeric_spec(value)));
            }
        }
        let result = tool
            .execute("call".into(), params, None, Arc::new(|_| Ok(())), None)
            .await;
        let mut actual = match result {
            Ok(result) => {
                let mut value = to_value(&result).unwrap();
                if let Some(details) = result.details {
                    value.as_object_mut().unwrap().insert("details".into(), details);
                }
                JsValue::Object([("result".into(), value)].into_iter().collect())
            }
            Err(error) => json!({"error": error.message}),
        };
        actual
            .as_object_mut()
            .unwrap()
            .insert("trace".into(), to_value(&*trace.lock().unwrap()).unwrap());
        assert_eq!(project_nonfinite(actual), expected["output"][index], "case {index}");
    }
}
