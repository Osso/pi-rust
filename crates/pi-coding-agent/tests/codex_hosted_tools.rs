//! Behavioral tests for the bundled `codex-web-search` and
//! `codex-image-generation` extensions.

use crate::shared_bundled as bundled;
use crate::shared_extensions as support;
use pi_js_compat::js_json::JsString;

use std::sync::{Arc, Mutex};

use bundled::{Harness, HarnessOptions, test_model};
use pi_ai::types::{ImageContent, TextOrImageContent};
use pi_coding_agent::bundled_extensions::codex_image_generation::{
    ImageGenerationToolOptions, RunImageGeneration, add_image_generation_tool_to_payload,
    is_openai_hosted_image_generation_model,
};
use pi_coding_agent::bundled_extensions::codex_web_search::{
    RunWebSearch, WebSearchRequest, WebSearchToolOptions, add_web_search_tool_to_payload,
    is_openai_hosted_web_search_model,
};
use pi_coding_agent::core::extensions::ExtensionRuntime;
use pi_coding_agent::core::extensions::{Extension, create_extension_runtime};
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_json as json;

fn oauth_auth(provider: &str, token: &str) -> Option<pi_js_compat::js_json::JsObject> {
    json!({(provider): {
        "type": "oauth",
        "refresh": "refresh-token",
        "access": token,
        "expires": 9_999_999_999_999.0
    }})
    .as_object()
    .cloned()
}

fn text_of(result: &pi_agent_core::AgentToolResult) -> JsString {
    let TextOrImageContent::Text(text) = &result.content[0] else {
        panic!("text result");
    };
    text.text.clone()
}

async fn load_web_search(runtime: &ExtensionRuntime, options: WebSearchToolOptions) -> Extension {
    support::extension(runtime, "/repo", "<first-party:codex-web-search>", move |pi| {
        pi_coding_agent::bundled_extensions::codex_web_search::codex_web_search_extension(pi, &options)
            .expect("web search extension")
    })
    .await
}

async fn load_image_gen(runtime: &ExtensionRuntime, options: ImageGenerationToolOptions) -> Extension {
    support::extension(runtime, "/repo", "<first-party:codex-image-generation>", move |pi| {
        pi_coding_agent::bundled_extensions::codex_image_generation::codex_image_generation_extension(pi, &options)
            .expect("image generation extension")
    })
    .await
}

fn model_with(api: &str, provider: &str) -> pi_ai::types::Model {
    let mut model = test_model(false);
    model.api = api.to_owned();
    model.provider = provider.to_owned();
    model
}

#[test]
fn web_search_payload_injection_matches_source() {
    assert_eq!(
        add_web_search_tool_to_payload(&json!({
            "model": "gpt",
            "tools": [{"type": "function", "name": "read"}]
        })),
        Some(json!({
            "model": "gpt",
            "tools": [
                {"type": "function", "name": "read"},
                {"type": "web_search", "external_web_access": true}
            ]
        }))
    );
    assert_eq!(
        add_web_search_tool_to_payload(&json!({"tools": [{"type": "web_search"}]})),
        None
    );
    assert_eq!(
        add_web_search_tool_to_payload(&json!({
            "tools": [
                {"type": "function", "name": "web_search"},
                {"type": "function", "name": "read"}
            ]
        })),
        Some(json!({
            "tools": [
                {"type": "function", "name": "read"},
                {"type": "web_search", "external_web_access": true}
            ]
        }))
    );
    assert_eq!(add_web_search_tool_to_payload(&json!("no")), None);
}

#[test]
fn image_generation_payload_injection_matches_source() {
    assert_eq!(
        add_image_generation_tool_to_payload(&json!({
            "model": "gpt",
            "tools": [
                {"type": "function", "name": "image_gen"},
                {"type": "function", "name": "read"}
            ]
        })),
        Some(json!({
            "model": "gpt",
            "tools": [
                {"type": "function", "name": "read"},
                {"type": "image_generation", "output_format": "png"}
            ]
        }))
    );
    assert_eq!(
        add_image_generation_tool_to_payload(&json!({"tools": [{"type": "image_generation", "output_format": "png"}]})),
        None
    );
}

#[test]
fn identifies_hosted_models() {
    assert!(is_openai_hosted_web_search_model(Some(&model_with(
        "openai-responses",
        "openai"
    ))));
    assert!(is_openai_hosted_web_search_model(Some(&model_with(
        "openai-codex-responses",
        "openai-codex"
    ))));
    assert!(!is_openai_hosted_web_search_model(Some(&model_with(
        "anthropic-messages",
        "anthropic"
    ))));
    assert!(!is_openai_hosted_web_search_model(None));

    assert!(is_openai_hosted_image_generation_model(Some(&model_with(
        "openai-codex-responses",
        "openai-codex"
    ))));
    assert!(is_openai_hosted_image_generation_model(Some(&model_with(
        "openai-codex-responses",
        "openai-codex-gc"
    ))));
    assert!(!is_openai_hosted_image_generation_model(Some(&model_with(
        "openai-responses",
        "openai"
    ))));
}

#[tokio::test(start_paused = true)]
async fn web_search_registers_tool_and_payload_handler() {
    let runtime = create_extension_runtime();
    let extension = load_web_search(&runtime, WebSearchToolOptions::default()).await;
    let tool = extension
        .read()
        .tools
        .get("web_search")
        .expect("web_search tool")
        .definition
        .clone();
    assert_eq!(tool.name, "web_search");
    assert_eq!(tool.label, "Web Search");
    assert_eq!(tool.approval_required, Some(true));
    assert_eq!(
        pi_js_compat::js_json::JsValue::from(tool.parameters.clone()),
        json!({
            "type": "object",
            "required": ["query"],
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query to send to the web search provider."
                }
            }
        })
    );
    assert!(extension.read().handlers.before_provider_request.len() == 1);

    let mut options = HarnessOptions::new("/repo");
    options.model = Some(model_with("openai-responses", "openai"));
    let harness = Harness::new(vec![extension], runtime, options).await;
    assert_eq!(
        harness
            .runner
            .emit_before_provider_request(json!({
                "tools": [{"type": "function", "name": "web_search"}]
            }))
            .await,
        json!({"tools": [{"type": "web_search", "external_web_access": true}]})
    );
}

#[tokio::test(start_paused = true)]
async fn image_generation_registers_tool_without_payload_handler() {
    let runtime = create_extension_runtime();
    let extension = load_image_gen(&runtime, ImageGenerationToolOptions::default()).await;
    let tool = extension
        .read()
        .tools
        .get("image_gen")
        .expect("image_gen tool")
        .definition
        .clone();
    assert_eq!(tool.name, "image_gen");
    assert_eq!(tool.label, "Image Generation");
    assert_eq!(tool.approval_required, Some(true));
    assert_eq!(
        pi_js_compat::js_json::JsValue::from(tool.parameters.clone()),
        json!({
            "type": "object",
            "required": ["prompt"],
            "properties": {
                "prompt": {
                    "type": "string",
                    "description": "Detailed description of the image to generate."
                }
            }
        })
    );
    assert!(extension.read().handlers.before_provider_request.is_empty());
}

#[tokio::test(start_paused = true)]
async fn web_search_executes_with_injected_search() {
    let captured: Arc<Mutex<Option<WebSearchRequest>>> = Arc::new(Mutex::new(None));
    let capture = Arc::clone(&captured);
    let run_search: RunWebSearch = Arc::new(move |request, _ctx| {
        *capture.lock().unwrap() = Some(request);
        Box::pin(async { Ok(JsString::from("Search result text")) })
    });
    let runtime = create_extension_runtime();
    let extension = load_web_search(
        &runtime,
        WebSearchToolOptions {
            run_search: Some(run_search),
            timeout_ms: None,
        },
    )
    .await;
    let mut options = HarnessOptions::new("/repo");
    options.model = Some(model_with("openai-responses", "openai-codex"));
    options.auth = oauth_auth("openai-codex", "test-key");
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
        json!({"query": "  pi web search  "}),
        None,
        None,
        Some(harness.context()),
    )
    .await
    .expect("execute");

    assert_eq!(text_of(&result), "Search result text");
    let request = captured.lock().unwrap().clone().expect("request");
    assert_eq!(request.query, "pi web search");
    assert_eq!(request.api_key.clone(), Some(JsString::from("test-key")));
    assert!(request.signal.is_some());
}

#[tokio::test(start_paused = true)]
async fn web_search_times_out_with_source_message() {
    let captured_signal: Arc<Mutex<Option<AbortSignal>>> = Arc::new(Mutex::new(None));
    let capture = Arc::clone(&captured_signal);
    let run_search: RunWebSearch = Arc::new(move |request, _ctx| {
        *capture.lock().unwrap() = request.signal.clone();
        Box::pin(async { std::future::pending::<Result<JsString, JsString>>().await })
    });
    let runtime = create_extension_runtime();
    let extension = load_web_search(
        &runtime,
        WebSearchToolOptions {
            run_search: Some(run_search),
            timeout_ms: Some(10),
        },
    )
    .await;
    let mut options = HarnessOptions::new("/repo");
    options.model = Some(model_with("openai-responses", "openai-codex"));
    options.auth = oauth_auth("openai-codex", "test-key");
    let harness = Harness::new(vec![extension.clone()], runtime, options).await;
    let tool = extension
        .read()
        .tools
        .get("web_search")
        .expect("tool")
        .definition
        .clone();
    let context = harness.context();

    let future = (tool.execute)(
        "call-1".to_owned(),
        json!({"query": "slow search"}),
        None,
        None,
        Some(context),
    );
    let handle = tokio::spawn(future);
    for _ in 0..10 {
        tokio::task::yield_now().await;
    }
    tokio::time::advance(std::time::Duration::from_millis(10)).await;
    tokio::task::yield_now().await;
    let error = handle.await.expect("join").unwrap_err();
    assert_eq!(error.message, "OpenAI hosted web_search timed out after 10ms");
    assert!(
        captured_signal
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(AbortSignal::aborted)
    );
}

#[tokio::test(start_paused = true)]
async fn web_search_preserves_caller_abort() {
    let controller_controller = pi_js_compat::abort::AbortController::new();
    let controller = controller_controller.signal();
    let run_search: RunWebSearch = Arc::new(move |request, _ctx| {
        assert!(request.signal.is_some());
        controller_controller.abort(None);
        Box::pin(async { std::future::pending::<Result<JsString, JsString>>().await })
    });
    let runtime = create_extension_runtime();
    let extension = load_web_search(
        &runtime,
        WebSearchToolOptions {
            run_search: Some(run_search),
            timeout_ms: Some(10_000),
        },
    )
    .await;
    let mut options = HarnessOptions::new("/repo");
    options.model = Some(model_with("openai-responses", "openai-codex"));
    options.auth = oauth_auth("openai-codex", "test-key");
    let harness = Harness::new(vec![extension.clone()], runtime, options).await;
    let tool = extension
        .read()
        .tools
        .get("web_search")
        .expect("tool")
        .definition
        .clone();

    let error = (tool.execute)(
        "call-1".to_owned(),
        json!({"query": "abort search"}),
        Some(controller),
        None,
        Some(harness.context()),
    )
    .await
    .unwrap_err();

    assert_eq!(error.message, "This operation was aborted");
}

#[tokio::test(start_paused = true)]
async fn web_search_validates_query_and_model() {
    let called = Arc::new(Mutex::new(false));
    let flag = Arc::clone(&called);
    let run_search: RunWebSearch = Arc::new(move |_request, _ctx| {
        *flag.lock().unwrap() = true;
        Box::pin(async { Ok(JsString::from("text")) })
    });
    let runtime = create_extension_runtime();
    let extension = load_web_search(
        &runtime,
        WebSearchToolOptions {
            run_search: Some(run_search),
            timeout_ms: None,
        },
    )
    .await;
    let mut options = HarnessOptions::new("/repo");
    options.model = Some(model_with("anthropic-messages", "anthropic"));
    let harness = Harness::new(vec![extension.clone()], runtime, options).await;
    let tool = extension
        .read()
        .tools
        .get("web_search")
        .expect("tool")
        .definition
        .clone();

    let error = (tool.execute)(
        "call-1".to_owned(),
        json!({"query": "pi"}),
        None,
        None,
        Some(harness.context()),
    )
    .await
    .unwrap_err();
    assert_eq!(error.message, "web_search requires an OpenAI Responses model");
    assert!(!*called.lock().unwrap());

    let error = (tool.execute)(
        "call-1".to_owned(),
        json!({"query": "   "}),
        None,
        None,
        Some(harness.context()),
    )
    .await
    .unwrap_err();
    assert_eq!(error.message, "web_search query is required");
}

#[tokio::test(start_paused = true)]
async fn image_generation_saves_png_and_returns_content() {
    let output = tempfile::tempdir().expect("temp dir");
    let run_generation: RunImageGeneration = Arc::new(|request, _ctx| {
        assert_eq!(request.prompt, "A simple blue circle");
        Box::pin(async { Ok(ImageContent::new("aW1hZ2U=", "image/png")) })
    });
    let runtime = create_extension_runtime();
    let extension = load_image_gen(
        &runtime,
        ImageGenerationToolOptions {
            run_generation: Some(run_generation),
            timeout_ms: None,
        },
    )
    .await;
    let mut options = HarnessOptions::new(output.path().to_str().unwrap());
    options.model = Some(model_with("openai-codex-responses", "openai-codex"));
    options.auth = oauth_auth("openai-codex", "test-key");
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
        json!({"prompt": "  A simple blue circle  "}),
        None,
        None,
        Some(harness.context()),
    )
    .await
    .expect("execute");

    assert_eq!(result.content.len(), 2);
    let path_text = text_of(&result);
    let path = path_text.strip_prefix("Generated image: ").expect("path prefix");
    assert!(
        path.starts_with(output.path().to_str().unwrap()),
        "unexpected path {path:?}"
    );
    let scalar_path = path.to_utf8().expect("utf-8 test path");
    let file_name = std::path::Path::new(&scalar_path)
        .file_name()
        .unwrap()
        .to_str()
        .expect("utf-8 test path");
    let hex = file_name
        .strip_prefix("image-gen-")
        .and_then(|name| name.strip_suffix(".png"))
        .expect("image name");
    assert_eq!(hex.len(), 16);
    assert!(hex.chars().all(|character| character.is_ascii_hexdigit()));
    assert_eq!(std::fs::read(&scalar_path).expect("saved image"), b"image");
    assert_eq!(
        result.content[1],
        TextOrImageContent::Image(ImageContent::new("aW1hZ2U=", "image/png"))
    );
}

#[tokio::test(start_paused = true)]
async fn image_generation_validates_prompt_and_model() {
    let run_generation: RunImageGeneration =
        Arc::new(|_request, _ctx| Box::pin(async { Ok(ImageContent::new("aQ==", "image/png")) }));
    let runtime = create_extension_runtime();
    let extension = load_image_gen(
        &runtime,
        ImageGenerationToolOptions {
            run_generation: Some(run_generation),
            timeout_ms: None,
        },
    )
    .await;
    let mut options = HarnessOptions::new("/repo");
    options.model = Some(model_with("openai-responses", "openai"));
    let harness = Harness::new(vec![extension.clone()], runtime, options).await;
    let tool = extension
        .read()
        .tools
        .get("image_gen")
        .expect("tool")
        .definition
        .clone();

    let error = (tool.execute)(
        "call-1".to_owned(),
        json!({"prompt": "circle"}),
        None,
        None,
        Some(harness.context()),
    )
    .await
    .unwrap_err();
    assert_eq!(error.message, "image_gen requires a Codex provider model");

    let error = (tool.execute)(
        "call-1".to_owned(),
        json!({"prompt": "  "}),
        None,
        None,
        Some(harness.context()),
    )
    .await
    .unwrap_err();
    assert_eq!(error.message, "image_gen prompt is required");
}

#[tokio::test(start_paused = true)]
async fn image_generation_applies_timeout_to_the_request_signal() {
    let run_generation: RunImageGeneration = Arc::new(|request, _ctx| {
        Box::pin(async move {
            let signal = request.signal.expect("signal");
            signal.wait().await;
            Err(JsString::from("aborted"))
        })
    });
    let runtime = create_extension_runtime();
    let extension = load_image_gen(
        &runtime,
        ImageGenerationToolOptions {
            run_generation: Some(run_generation),
            timeout_ms: Some(5),
        },
    )
    .await;
    let mut options = HarnessOptions::new("/repo");
    options.model = Some(model_with("openai-codex-responses", "openai-codex"));
    options.auth = oauth_auth("openai-codex", "test-key");
    let harness = Harness::new(vec![extension.clone()], runtime, options).await;
    let tool = extension
        .read()
        .tools
        .get("image_gen")
        .expect("tool")
        .definition
        .clone();
    let context = harness.context();

    let handle = tokio::spawn((tool.execute)(
        "call-1".to_owned(),
        json!({"prompt": "circle"}),
        None,
        None,
        Some(context),
    ));
    for _ in 0..10 {
        tokio::task::yield_now().await;
    }
    tokio::time::advance(std::time::Duration::from_millis(5)).await;
    tokio::task::yield_now().await;
    let error = handle.await.expect("join").unwrap_err();
    assert_eq!(error.message, "aborted");
}
