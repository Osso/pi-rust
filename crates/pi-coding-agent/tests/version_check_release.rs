//! Frozen release checks at the request, response, error and process-environment boundaries.
#[path = "support/oracle.rs"]
mod oracle;

use pi_ai::request::{BoxFuture, CallbackError, Fetch, HttpBody, HttpRequest, HttpResponse};
use pi_coding_agent::utils::version_check::{
    LatestPiRelease, LatestPiReleaseOptions, check_for_new_pi_version, get_latest_pi_release, get_latest_pi_version,
};
use pi_js_compat::{
    abort::{AbortReason, AbortSignal},
    js_json::{self, JsString},
    node_error::{NativeErrorClass, NativeNodeError},
};
use serde_json::{Value, json};
use std::{process::Command, sync::Mutex, time::Duration};

struct ScriptedFetch {
    case: Value,
    requests: Mutex<Vec<Value>>,
}
struct ScriptedBody(Option<Result<Vec<u8>, CallbackError>>);
impl HttpBody for ScriptedBody {
    fn next_chunk(&mut self) -> BoxFuture<'_, Option<Result<Vec<u8>, CallbackError>>> {
        let next = self.0.take();
        Box::pin(async move { next })
    }
}
impl Fetch for ScriptedFetch {
    fn fetch(
        &self,
        request: HttpRequest,
        signal: Option<AbortSignal>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        let header = |name: &str| {
            request
                .headers
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case(name))
                .map(|(_, value)| value.clone())
        };
        self.requests
            .lock()
            .unwrap()
            .push(json!({"url": request.url, "method": request.method,
            "accept": header("accept"), "userAgent": header("user-agent"), "signal": signal.is_some()}));
        Box::pin(async move {
            match self.case["mode"].as_str() {
                Some("fetch-error") => {
                    return Err(NativeNodeError::from_constructor(NativeErrorClass::TypeError, "fetch failed").into());
                }
                Some("timeout") => {
                    let signal = signal.ok_or_else(|| CallbackError::new("missing timeout signal"))?;
                    return Err(CallbackError::from_rejection(signal.wait().await));
                }
                Some("slow") => tokio::time::sleep(Duration::from_millis(20)).await,
                _ => {}
            }
            let body = if self.case["mode"] == "body-error" {
                Err(CallbackError::new("body failed"))
            } else {
                Ok(self.case["body"]
                    .as_str()
                    .unwrap_or("{\"version\":\"1.2.4\"}")
                    .as_bytes()
                    .to_vec())
            };
            Ok(HttpResponse {
                status: self.case["status"].as_u64().unwrap_or(200) as u16,
                status_text: String::new(),
                headers: vec![],
                body: Box::new(ScriptedBody(Some(body))),
            })
        })
    }
}
fn release_value(release: Option<LatestPiRelease>) -> Value {
    let Some(release) = release else {
        return Value::Null;
    };
    let mut value = json!({"version": release.version});
    if let Some(package) = release.package_name {
        value["packageName"] = json!(package);
    }
    if let Some(note) = release.note {
        value["note"] = json!(note);
    }
    value
}
fn snapshot_error(error: CallbackError) -> Value {
    match error.thrown_value() {
        AbortReason::Error(error) | AbortReason::DomException(error) => {
            let mut value =
                json!({"name": error.name, "message": error.message, "constructor": error.constructor_class().name()});
            if let Some(code) = error.value().get("code") {
                value["code"] = serde_json::to_value(code).unwrap();
            }
            value
        }
        AbortReason::Value(value) => json!({"thrown": value}),
    }
}
async fn run_case(case: Value) -> Value {
    let version = case["currentJson"]
        .as_str()
        .map(|input| {
            js_json::parse_lossless(&JsString::from(input))
                .unwrap()
                .as_string()
                .unwrap()
                .clone()
        })
        .unwrap_or_else(|| JsString::from("1.2.3"));
    let fetch = ScriptedFetch {
        case: case.clone(),
        requests: Mutex::new(vec![]),
    };
    let options = LatestPiReleaseOptions {
        timeout_ms: case["timeoutMs"].as_f64(),
    };
    let result: Result<Value, CallbackError> = match case["op"].as_str() {
        Some("version") => get_latest_pi_version(&fetch, &version, &options)
            .await
            .map(|version| json!(version)),
        Some("check") => Ok(release_value(check_for_new_pi_version(&fetch, &version).await)),
        _ => get_latest_pi_release(&fetch, &version, &options)
            .await
            .map(release_value),
    };
    let mut output = match result {
        Ok(value) => json!({"success": true, "value": value}),
        Err(error) => json!({"success": false, "error": snapshot_error(error)}),
    };
    output["requests"] = json!(fetch.requests.into_inner().unwrap());
    output
}
#[test]
#[ignore = "invoked by release_api_matches_frozen_source"]
fn version_release_child() {
    let case: Value = serde_json::from_str(&std::env::var("PI_VERSION_RELEASE_CASE").unwrap()).unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let output = runtime.block_on(run_case(case));
    println!("RELEASE_RESULT:{output}");
}
fn run_native(case: &Value) -> Value {
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "version_release_child", "--ignored", "--nocapture"])
        .env("PI_VERSION_RELEASE_CASE", case.to_string())
        .env("PI_SKIP_VERSION_CHECK", case["skip"].as_str().unwrap_or(""))
        .env("PI_OFFLINE", case["offline"].as_str().unwrap_or(""))
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let text = String::from_utf8(output.stdout).unwrap();
    let payload = text
        .lines()
        .find_map(|line| line.split_once("RELEASE_RESULT:").map(|(_, value)| value))
        .unwrap();
    serde_json::from_str(payload).unwrap()
}
fn assert_requests_and_result(mut native: Value, mut source: Value, case: &Value) {
    // The parent classifies truthful native runtime metadata as a runtime gap.
    let actual = native["requests"].as_array_mut().unwrap();
    let expected = source["requests"].as_array_mut().unwrap();
    assert_eq!(actual.len(), expected.len(), "request count: {case}");
    for (actual, expected) in actual.iter_mut().zip(expected) {
        let native_agent = actual["userAgent"].as_str().unwrap();
        let source_agent = expected["userAgent"].as_str().unwrap();
        let native_parts: Vec<_> = native_agent.split("; ").collect();
        let source_parts: Vec<_> = source_agent.split("; ").collect();
        assert_eq!(native_parts.len(), 3);
        assert_eq!(source_parts.len(), 3);
        assert_eq!(native_parts[0], source_parts[0]);
        assert_eq!(native_parts[2], source_parts[2]);
        assert_eq!(native_parts[1], "rust");
        assert!(source_parts[1].starts_with("node/") || source_parts[1].starts_with("bun/"));
        actual.as_object_mut().unwrap().remove("userAgent");
        expected.as_object_mut().unwrap().remove("userAgent");
    }
    assert_eq!(native, source, "{case}");
}
#[test]
fn release_api_matches_frozen_source() {
    let mut oracle = oracle::TsOracle::spawn(
        "version-release-oracle.ts",
        "packages/coding-agent/src/utils/version-check.ts",
    );
    let cases = [
        json!({"mode":"fetch-error"}),
        json!({}),
        json!({"op":"version"}),
        json!({"op":"check"}),
        json!({"body":"{\"version\":\" 1.2.4 \",\"packageName\":\" @scope/pi \",\"note\":\" **Note** \",\"extra\":1}"}),
        json!({"body":"{\"version\":\"1.2.4\",\"packageName\":\" \",\"note\":\" \"}"}),
        json!({"body":"{\"version\":\"   \"}"}),
        json!({"body":"{}"}),
        json!({"status":503}),
        json!({"mode":"body-error"}),
        json!({"body":"not json"}),
        json!({"body":""}),
        json!({"body":"\u{feff}{\"version\":\"1.2.4\"}"}),
        json!({"body":"{\"version\":\"1.2.3\"}","op":"check"}),
        json!({"body":"{\"version\":\"1.2.2\"}","op":"check"}),
        json!({"mode":"fetch-error","op":"version"}),
        json!({"mode":"fetch-error","op":"check"}),
        json!({"body":"not json","op":"check"}),
        json!({"mode":"body-error","op":"check"}),
        json!({"timeoutMs":1,"mode":"timeout"}),
        json!({"timeoutMs":1,"mode":"slow"}),
        json!({"skip":"1"}),
        json!({"skip":"0","op":"version"}),
        json!({"skip":"","op":"release"}),
        json!({"offline":"1"}),
        json!({"offline":"0","op":"version"}),
        json!({"offline":"","op":"check"}),
    ];
    for (id, mut case) in cases.into_iter().enumerate() {
        case["id"] = json!(id);
        let response = oracle.request(&case);
        assert_eq!(response["ok"], true, "{response}");
        assert_requests_and_result(run_native(&case), response["output"].clone(), &case);
    }
}
