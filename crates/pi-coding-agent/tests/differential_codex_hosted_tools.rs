//! Differential tests for the bundled `codex-web-search` and
//! `codex-image-generation` extensions.

#[path = "support/bundled.rs"]
mod bundled;
#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/extensions.rs"]
mod support;

use std::sync::{Arc, Mutex};

use bundled::{Harness, HarnessOptions, test_model};
use oracle::TsOracle;
use pi_ai::types::{ImageContent, TextOrImageContent};
use pi_coding_agent::bundled_extensions::codex_image_generation::{
    ImageGenerationRequest, ImageGenerationToolOptions, RunImageGeneration, add_image_generation_tool_to_payload,
    create_image_generation_tool_definition, is_openai_hosted_image_generation_model,
};
use pi_coding_agent::bundled_extensions::codex_web_search::{
    RunWebSearch, WebSearchRequest, WebSearchToolOptions, add_web_search_tool_to_payload,
    create_web_search_tool_definition, is_openai_hosted_web_search_model,
};
use pi_coding_agent::core::extensions::types::ToolDefinition;
use pi_coding_agent::core::extensions::{ExtensionRuntime, create_extension_runtime};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::JsValue as Value;
use pi_js_compat::js_json::{JsObject as Map, JsString};

const SOURCE: &str = "packages/coding-agent/extensions/codex-web-search/src/index.ts";

fn model(case: &Value) -> pi_ai::types::Model {
    let mut model = test_model(false);
    model.api = case
        .get("api")
        .and_then(Value::as_string)
        .map(|tag| tag.to_utf8().expect("scalar model tag"))
        .unwrap_or_default();
    model.provider = case
        .get("provider")
        .and_then(Value::as_string)
        .map(|tag| tag.to_utf8().expect("scalar model tag"))
        .unwrap_or_default();
    model
}

fn metadata(tool: &ToolDefinition) -> Value {
    json!({
        "name": tool.name,
        "label": tool.label,
        "description": tool.description,
        "promptSnippet": tool.prompt_snippet,
        "promptGuidelines": tool.prompt_guidelines,
        "approvalRequired": tool.approval_required,
        "parameters": tool.parameters,
    })
}

fn oauth_auth(provider: &str) -> Option<Map> {
    json!({(provider): {
        "type": "oauth",
        "refresh": "refresh-token",
        "access": "test-key",
        "expires": 9_999_999_999_999.0
    }})
    .as_object()
    .cloned()
}

fn normalize_path(text: &JsString) -> JsString {
    let pattern = pi_js_compat::js_regex::JsRegex::new(r"image-gen-[a-f0-9]{16}\.png").expect("image name pattern");
    pattern.replace_all(text, "image-gen-<hex>.png")
}

async fn load_web_search(
    runtime: &ExtensionRuntime,
    options: WebSearchToolOptions,
) -> pi_coding_agent::core::extensions::Extension {
    support::extension(runtime, "/repo", "<first-party:codex-web-search>", move |pi| {
        pi_coding_agent::bundled_extensions::codex_web_search::codex_web_search_extension(pi, &options)
            .expect("web search extension")
    })
    .await
}

async fn load_image_generation(
    runtime: &ExtensionRuntime,
    options: ImageGenerationToolOptions,
) -> pi_coding_agent::core::extensions::Extension {
    support::extension(runtime, "/repo", "<first-party:codex-image-generation>", move |pi| {
        pi_coding_agent::bundled_extensions::codex_image_generation::codex_image_generation_extension(pi, &options)
            .expect("image generation extension")
    })
    .await
}

async fn run_web_execute(case: &Value) -> Value {
    let captured: Arc<Mutex<Option<Value>>> = Arc::new(Mutex::new(None));
    let capture = Arc::clone(&captured);
    let run_error = case.get("runError").and_then(Value::as_string).cloned();
    let run_result = case
        .get("runResult")
        .and_then(Value::as_string)
        .cloned()
        .unwrap_or_else(|| JsString::from("Search result text"));
    let run_search: RunWebSearch = Arc::new(move |request: WebSearchRequest, _ctx| {
        *capture.lock().unwrap() = Some(json!({
            "query": request.query,
            "apiKey": request.api_key,
            "hasSignal": request.signal.is_some(),
        }));
        let run_error = run_error.clone();
        let run_result = run_result.clone();
        Box::pin(async move {
            if let Some(error) = run_error {
                return Err(error);
            }
            Ok(run_result)
        })
    });
    let runtime = create_extension_runtime();
    let extension = load_web_search(
        &runtime,
        WebSearchToolOptions {
            run_search: Some(run_search),
            timeout_ms: case.get("timeoutMs").and_then(Value::as_u64),
        },
    )
    .await;
    let mut options = HarnessOptions::new("/repo");
    let model_case = case.get("model").cloned().unwrap_or(Value::Null);
    options.model = Some(model(&model_case));
    options.auth = oauth_auth("openai-codex");
    let harness = Harness::new(vec![extension.clone()], runtime, options).await;
    let tool = extension
        .read()
        .tools
        .get("web_search")
        .expect("tool")
        .definition
        .clone();
    let result = (tool.execute)(
        "call-1".to_owned(),
        json!({"query": case.get("query").cloned().unwrap_or(Value::Null)}),
        None,
        None,
        Some(harness.context()),
    )
    .await;
    match result {
        Ok(result) => {
            let text: Vec<Value> = result
                .content
                .iter()
                .map(|content| match content {
                    TextOrImageContent::Text(text) => Value::String(text.text.clone()),
                    TextOrImageContent::Image(_) => Value::Null,
                })
                .collect();
            json!({
                "name": case["name"],
                "text": text,
                "captured": captured.lock().unwrap().clone(),
                "error": null,
            })
        }
        Err(error) => json!({
            "name": case["name"],
            "error": error.message,
            "captured": captured.lock().unwrap().clone(),
        }),
    }
}

async fn run_image_execute(case: &Value) -> Value {
    let captured: Arc<Mutex<Option<Value>>> = Arc::new(Mutex::new(None));
    let capture = Arc::clone(&captured);
    let run_error = case.get("runError").and_then(Value::as_string).cloned();
    let run_generation: RunImageGeneration = Arc::new(move |request: ImageGenerationRequest, _ctx| {
        *capture.lock().unwrap() = Some(json!({
            "prompt": request.prompt,
            "apiKey": request.api_key,
            "hasSignal": request.signal.is_some(),
        }));
        let run_error = run_error.clone();
        Box::pin(async move {
            if let Some(error) = run_error {
                return Err(error);
            }
            Ok(ImageContent::new("aW1hZ2U=", "image/png"))
        })
    });
    let runtime = create_extension_runtime();
    let extension = load_image_generation(
        &runtime,
        ImageGenerationToolOptions {
            run_generation: Some(run_generation),
            timeout_ms: case.get("timeoutMs").and_then(Value::as_u64),
        },
    )
    .await;
    let cwd = case
        .get("cwd")
        .and_then(Value::as_string)
        .cloned()
        .unwrap_or_else(|| JsString::from("/tmp"));
    let mut options = HarnessOptions::new(&cwd.to_utf8().expect("utf-8 test path"));
    let model_case = case.get("model").cloned().unwrap_or(Value::Null);
    options.model = Some(model(&model_case));
    options.auth = oauth_auth("openai-codex");
    let harness = Harness::new(vec![extension.clone()], runtime, options).await;
    let tool = extension
        .read()
        .tools
        .get("image_gen")
        .expect("tool")
        .definition
        .clone();
    let result = (tool.execute)(
        "call-1".to_owned(),
        json!({"prompt": case.get("prompt").cloned().unwrap_or(Value::Null)}),
        None,
        None,
        Some(harness.context()),
    )
    .await;
    match result {
        Ok(result) => {
            let content: Vec<Value> = result
                .content
                .iter()
                .map(|content| match content {
                    TextOrImageContent::Text(text) => {
                        json!({"type": "text", "text": normalize_path(&text.text)})
                    }
                    TextOrImageContent::Image(image) => {
                        json!({"type": "image", "data": image.data, "mimeType": image.mime_type})
                    }
                })
                .collect();
            json!({
                "name": case["name"],
                "content": content,
                "captured": captured.lock().unwrap().clone(),
                "error": null,
            })
        }
        Err(error) => json!({
            "name": case["name"],
            "error": error.message,
            "captured": captured.lock().unwrap().clone(),
        }),
    }
}

async fn rust_results(cases: &[Value]) -> Vec<Value> {
    let mut results = Vec::new();
    for case in cases {
        let kind = case.get("kind").and_then(Value::as_string).cloned().unwrap_or_default();
        let mut output = Map::new();
        output.insert(
            "name".to_owned().into(),
            case.get("name").cloned().unwrap_or(Value::Null),
        );
        match kind {
            kind if kind == "web_payload" => {
                output.insert(
                    "value".to_owned().into(),
                    add_web_search_tool_to_payload(case.get("payload").unwrap_or(&Value::Null)).unwrap_or(Value::Null),
                );
            }
            kind if kind == "image_payload" => {
                output.insert(
                    "value".to_owned().into(),
                    add_image_generation_tool_to_payload(case.get("payload").unwrap_or(&Value::Null))
                        .unwrap_or(Value::Null),
                );
            }
            kind if kind == "web_model" => {
                output.insert(
                    "value".to_owned().into(),
                    Value::Bool(is_openai_hosted_web_search_model(Some(&model(
                        case.get("model").unwrap_or(&Value::Null),
                    )))),
                );
            }
            kind if kind == "image_model" => {
                output.insert(
                    "value".to_owned().into(),
                    Value::Bool(is_openai_hosted_image_generation_model(Some(&model(
                        case.get("model").unwrap_or(&Value::Null),
                    )))),
                );
            }
            kind if kind == "web_metadata" => {
                output.insert(
                    "metadata".to_owned().into(),
                    metadata(&create_web_search_tool_definition(&WebSearchToolOptions::default())),
                );
            }
            kind if kind == "image_metadata" => {
                output.insert(
                    "metadata".to_owned().into(),
                    metadata(&create_image_generation_tool_definition(
                        &ImageGenerationToolOptions::default(),
                    )),
                );
            }
            kind if kind == "web_execute" => {
                let value = run_web_execute(case).await;
                if let Value::Object(object) = value {
                    for (key, value) in object {
                        output.insert(key, value);
                    }
                }
            }
            kind if kind == "image_execute" => {
                let value = run_image_execute(case).await;
                if let Value::Object(object) = value {
                    for (key, value) in object {
                        output.insert(key, value);
                    }
                }
            }
            other => panic!("unknown hosted tools case {other:?}"),
        }
        results.push(Value::Object(output));
    }
    results
}

#[tokio::test]
async fn differential_codex_hosted_tools() {
    let cases = vec![
        json!({"name": "web-payload-add", "kind": "web_payload", "payload": {"model": "gpt", "tools": [{"type": "function", "name": "read"}]}}),
        json!({"name": "web-payload-hosted", "kind": "web_payload", "payload": {"tools": [{"type": "web_search"}]}}),
        json!({"name": "web-payload-replace", "kind": "web_payload", "payload": {"tools": [{"type": "function", "name": "web_search"}, {"type": "function", "name": "read"}]}}),
        json!({"name": "web-payload-newer", "kind": "web_payload", "payload": {"tools": [{"type": "web_search_2025_08_26"}]}}),
        json!({"name": "web-payload-non-object", "kind": "web_payload", "payload": "no"}),
        json!({"name": "image-payload-add", "kind": "image_payload", "payload": {"model": "gpt", "tools": [{"type": "function", "name": "image_gen"}, {"type": "function", "name": "read"}]}}),
        json!({"name": "image-payload-hosted", "kind": "image_payload", "payload": {"tools": [{"type": "image_generation", "output_format": "png"}]}}),
        json!({"name": "web-model-responses", "kind": "web_model", "model": {"api": "openai-responses", "provider": "openai"}}),
        json!({"name": "web-model-codex", "kind": "web_model", "model": {"api": "openai-codex-responses", "provider": "openai-codex"}}),
        json!({"name": "web-model-anthropic", "kind": "web_model", "model": {"api": "anthropic-messages", "provider": "anthropic"}}),
        json!({"name": "image-model-codex", "kind": "image_model", "model": {"api": "openai-codex-responses", "provider": "openai-codex"}}),
        json!({"name": "image-model-gc", "kind": "image_model", "model": {"api": "openai-codex-responses", "provider": "openai-codex-gc"}}),
        json!({"name": "image-model-team", "kind": "image_model", "model": {"api": "openai-codex-responses", "provider": "openai-codex-team"}}),
        json!({"name": "image-model-openai", "kind": "image_model", "model": {"api": "openai-responses", "provider": "openai"}}),
        json!({"name": "web-metadata", "kind": "web_metadata"}),
        json!({"name": "image-metadata", "kind": "image_metadata"}),
        json!({"name": "web-execute", "kind": "web_execute", "query": "  pi web search  ", "model": {"api": "openai-responses", "provider": "openai-codex"}}),
        json!({"name": "web-execute-error", "kind": "web_execute", "query": "pi", "model": {"api": "openai-responses", "provider": "openai-codex"}, "runError": "search failed"}),
        json!({"name": "web-execute-empty", "kind": "web_execute", "query": "   ", "model": {"api": "openai-responses", "provider": "openai-codex"}}),
        json!({"name": "web-execute-wrong-model", "kind": "web_execute", "query": "pi", "model": {"api": "anthropic-messages", "provider": "anthropic"}}),
        json!({"name": "image-execute", "kind": "image_execute", "prompt": "  A simple blue circle  ", "model": {"api": "openai-codex-responses", "provider": "openai-codex"}, "cwd": "/tmp"}),
        json!({"name": "image-execute-error", "kind": "image_execute", "prompt": "circle", "model": {"api": "openai-codex-responses", "provider": "openai-codex"}, "cwd": "/tmp", "runError": "generation failed"}),
        json!({"name": "image-execute-empty", "kind": "image_execute", "prompt": "  ", "model": {"api": "openai-codex-responses", "provider": "openai-codex"}, "cwd": "/tmp"}),
        json!({"name": "image-execute-wrong-model", "kind": "image_execute", "prompt": "circle", "model": {"api": "openai-responses", "provider": "openai"}, "cwd": "/tmp"}),
    ];
    let mut ts = TsOracle::spawn("codex-hosted-tools-oracle.ts", SOURCE);
    let response = ts.request(&json!({"id": 1, "operation": "cases", "cases": cases}));
    assert_eq!(
        response["ok"],
        pi_js_compat::js_json::JsValue::Bool(true),
        "oracle failed: {response:?}"
    );
    let expected: Vec<Value> =
        pi_js_compat::js_json::from_value(&response["output"]["results"].clone()).expect("oracle results");
    let actual = rust_results(&cases).await;
    assert_eq!(actual.len(), expected.len());
    for (index, (actual, expected)) in actual.iter().zip(expected.iter()).enumerate() {
        assert_eq!(
            actual,
            expected,
            "case {index} ({:?}) mismatch\nactual:\n{}\nexpected:\n{}",
            cases[index]["name"],
            serde_json::to_string_pretty(actual).unwrap(),
            serde_json::to_string_pretty(expected).unwrap()
        );
    }
}
