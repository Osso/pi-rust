use crate::shared_bundled as bundled;
use crate::shared_extensions as support;
use crate::shared_oracle as oracle;

use pi_coding_agent::bundled_extensions::{codex_image_generation as image, codex_web_search as web};
use pi_coding_agent::core::extensions::{create_extension_runtime, types::BoxFuture};
use pi_js_compat::abort::{AbortController, AbortReason, AbortSignal};
use pi_js_compat::js_json::{JsString, JsValue};
use std::sync::{Arc, Mutex};

fn reason(input: &JsValue) -> Option<AbortReason> {
    let input = &input["reason"];
    if input["kind"] == JsValue::String("default".into()) {
        return None;
    }
    if input["kind"] == JsValue::String("value".into()) {
        return Some(AbortReason::Value(input["value"].clone()));
    }
    Some(AbortReason::Error(pi_js_compat::node_error::NativeNodeError::new(
        input["name"].as_string().cloned().unwrap_or_else(|| "Error".into()),
        input["message"].as_string().unwrap().clone(),
    )))
}
fn describe(reason: Option<AbortReason>) -> JsValue {
    match reason {
        None => pi_js_compat::js_json!({"kind":"undefined"}),
        Some(AbortReason::Error(error)) => pi_js_compat::js_json!({"kind":"error","value":error.value()}),
        Some(AbortReason::DomException(error)) => pi_js_compat::js_json!({"kind":"dom","value":error.value()}),
        Some(AbortReason::Value(value)) => pi_js_compat::js_json!({"kind":"value","value":value}),
    }
}

fn run_worker(
    input: JsValue,
    caller: AbortController,
    signal: AbortSignal,
) -> BoxFuture<'static, Result<JsString, JsString>> {
    Box::pin(async move {
        if input["mode"] == JsValue::String("success".into()) {
            return Ok("finished".into());
        }
        if input["mode"] == JsValue::String("live".into()) {
            caller.abort(reason(&input));
        }
        if input["tool"] == JsValue::String("web".into()) {
            return std::future::pending().await;
        }
        let reason = signal.wait().await;
        let message = match reason {
            AbortReason::Error(error) | AbortReason::DomException(error) => error.message,
            AbortReason::Value(value) => pi_js_compat::js_json::to_js_string(&value).expect("fixture conversion"),
        };
        Err(message)
    })
}

async fn observe(input: &JsValue) -> JsValue {
    let caller = AbortController::new();
    if input["mode"] == JsValue::String("pre".into()) {
        caller.abort(reason(input));
    }
    let signal = (input["caller"] != JsValue::Bool(false)).then(|| caller.signal());
    let captured: Arc<Mutex<Option<AbortSignal>>> = Arc::new(Mutex::new(None));
    let capture = captured.clone();
    let worker_input = input.clone();
    let worker_caller = caller.clone();
    let runtime = create_extension_runtime();
    let timeout_ms = input["timeoutMs"].as_u64().unwrap_or(5);
    let is_web = input["tool"] == JsValue::String("web".into());
    let extension = if is_web {
        let run_search: web::RunWebSearch = Arc::new(move |request, _ctx| {
            let signal = request.signal.unwrap();
            *capture.lock().unwrap() = Some(signal.clone());
            run_worker(worker_input.clone(), worker_caller.clone(), signal)
        });
        support::extension(&runtime, "/repo", "<web>", move |pi| {
            web::codex_web_search_extension(
                pi,
                &web::WebSearchToolOptions {
                    run_search: Some(run_search.clone()),
                    timeout_ms: Some(timeout_ms),
                },
            )
            .unwrap()
        })
        .await
    } else {
        let run_generation: image::RunImageGeneration = Arc::new(move |request, _ctx| {
            let signal = request.signal.unwrap();
            *capture.lock().unwrap() = Some(signal.clone());
            let future = run_worker(worker_input.clone(), worker_caller.clone(), signal);
            Box::pin(async move {
                match future.await {
                    Err(error) => Err(error),
                    Ok(_) => Err("finished".into()),
                }
            })
        });
        support::extension(&runtime, "/repo", "<image>", move |pi| {
            image::codex_image_generation_extension(
                pi,
                &image::ImageGenerationToolOptions {
                    run_generation: Some(run_generation.clone()),
                    timeout_ms: Some(timeout_ms),
                },
            )
            .unwrap()
        })
        .await
    };
    let mut options = bundled::HarnessOptions::new("/repo");
    let mut model = bundled::test_model(false);
    model.api = "openai-codex-responses".into();
    model.provider = "openai-codex".into();
    options.model = Some(model);
    options.auth = pi_js_compat::js_json!({"openai-codex":{"type":"oauth","refresh":"refresh-token","access":"test-key","expires":9999999999999.0}}).as_object().cloned();
    let harness = bundled::Harness::new(vec![extension.clone()], runtime, options).await;
    let tool = extension
        .read()
        .tools
        .get(if is_web { "web_search" } else { "image_gen" })
        .unwrap()
        .definition
        .clone();
    let params = if is_web {
        pi_js_compat::js_json!({"query":"query"})
    } else {
        pi_js_compat::js_json!({"prompt":"prompt"})
    };
    let outcome = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        (tool.execute)("call".into(), params, signal, None, Some(harness.context())),
    )
    .await;
    let error = match outcome {
        Ok(result) => result
            .err()
            .map(|error| JsValue::String(error.message))
            .unwrap_or(JsValue::Null),
        Err(_) => JsValue::String("test harness deadline".into()),
    };
    if input["waitAfter"] == JsValue::Bool(true) {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    if input["abortAfter"] == JsValue::Bool(true) {
        caller.abort(reason(input));
    }
    let observed = captured.lock().unwrap().clone();
    pi_js_compat::js_json!({"error":error,"invoked":observed.is_some(),"signal":observed.map(|signal|pi_js_compat::js_json!({"aborted":signal.aborted(),"reason":describe(signal.reason())})).unwrap_or(JsValue::Null)})
}

#[tokio::test(start_paused = true)]
async fn bundled_hosted_tools_preserve_timeout_and_caller_reasons_with_frozen_lifetimes() {
    let mut oracle = oracle::TsOracle::spawn(
        "codex-hosted-abort-oracle.ts",
        "packages/coding-agent/extensions/codex-web-search/src/index.ts",
    );
    let cases = pi_js_compat::js_json::parse_lossless(
        r#"[
        {"mode":"timeout"},
        {"mode":"timeout","caller":false},
        {"mode":"success","waitAfter":true},
        {"mode":"success","abortAfter":true,"reason":{"kind":"error","name":"CallerError","message":"native\ud800"}},
        {"mode":"pre","reason":{"kind":"error","name":"CallerError","message":"native\ud800"}},
        {"mode":"pre","reason":{"kind":"value","value":{"name":"PretendError","message":"not Error"}}},
        {"mode":"live","reason":{"kind":"error","name":"CallerError","message":"native\ud800"}},
        {"mode":"live","reason":{"kind":"value","value":false}},
        {"mode":"live","reason":{"kind":"default"}}
    ]"#,
    )
    .unwrap();
    let mut failures = Vec::new();
    for tool in ["web", "image"] {
        for case in cases.as_array().unwrap() {
            let mut input = case.clone();
            input
                .as_object_mut()
                .unwrap()
                .insert("tool".into(), JsValue::String(tool.into()));
            input
                .as_object_mut()
                .unwrap()
                .insert("id".into(), JsValue::String("hosted-abort".into()));
            let expected = oracle.request(&input);
            assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
            let actual = observe(&input).await;
            if actual != expected["output"] {
                failures.push(format!(
                    "input={} actual={} source={}",
                    pi_js_compat::js_json::stringify_lossless(&input),
                    pi_js_compat::js_json::stringify_lossless(&actual),
                    pi_js_compat::js_json::stringify_lossless(&expected)
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
