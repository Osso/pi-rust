//! Bundled `codex-web-search` extension
//! (`packages/coding-agent/extensions/codex-web-search/src/index.ts`).

use std::sync::Arc;
use std::time::Duration;

use pi_agent_core::AgentToolResult;
use pi_ai::compat::stream_simple;
use pi_ai::request::{BoxFuture, ProviderRequestOptions, SimpleStreamOptions, StreamOptions};
use pi_ai::types::{
    AssistantContent, Context, Model, ProviderEnv, ProviderHeaders, TextContent, TextOrImageContent, UserContent,
    UserMessage, UserRole,
};
use pi_js_compat::{
    js_json,
    js_json::{JsString, JsValue as Value},
};
use serde_json::json as schema_json;
use tokio_util::sync::CancellationToken;

use crate::core::extensions::types::{
    BeforeProviderRequestEvent, ExtensionApi, ExtensionContext, HandlerError, ToolDefinition,
};
use crate::core::model_registry::ResolvedRequestAuth;

const DEFAULT_WEB_SEARCH_TIMEOUT_MS: u64 = 180_000;

#[derive(Clone)]
pub struct WebSearchRequest {
    pub query: JsString,
    pub model: Model,
    pub api_key: Option<JsString>,
    pub headers: Option<ProviderHeaders>,
    pub env: Option<ProviderEnv>,
    pub signal: Option<CancellationToken>,
}

pub type RunWebSearch =
    Arc<dyn Fn(WebSearchRequest, ExtensionContext) -> BoxFuture<'static, Result<JsString, JsString>> + Send + Sync>;

#[derive(Clone, Default)]
pub struct WebSearchToolOptions {
    pub run_search: Option<RunWebSearch>,
    pub timeout_ms: Option<u64>,
}

pub fn is_openai_hosted_web_search_model(model: Option<&Model>) -> bool {
    model.is_some_and(|model| model.api == "openai-responses" || model.api == "openai-codex-responses")
}

#[cfg(test)]
mod lossless_tests {
    use super::*;
    #[test]
    fn hosted_payload_keeps_raw_content_and_tool_arguments() {
        let payload = pi_js_compat::js_json::parse_lossless(
            r#"{"input":"\ud800","tools":[{"type":"function","name":"keep","parameters":{"label":"\udc00"}}]}"#,
        )
        .unwrap();
        let changed = add_web_search_tool_to_payload(&payload).unwrap();
        assert_eq!(changed["input"], payload["input"]);
        assert_eq!(changed["tools"][0], payload["tools"][0]);
    }
}

pub fn add_web_search_tool_to_payload(payload: &Value) -> Option<Value> {
    let object = payload.as_object()?;
    let tools = object
        .get("tools")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let tools_without_function: Vec<Value> = tools
        .iter()
        .filter(|tool| !is_function_web_search_tool(tool))
        .cloned()
        .collect();
    if tools_without_function.iter().any(is_hosted_web_search_tool) {
        if tools_without_function.len() == tools.len() {
            return None;
        }
        let mut updated = object.clone();
        updated.insert("tools".into(), Value::Array(tools_without_function));
        return Some(Value::Object(updated));
    }
    let mut updated_tools = tools_without_function;
    updated_tools.push(js_json!({"type": "web_search", "external_web_access": true}));
    let mut updated = object.clone();
    updated.insert("tools".into(), Value::Array(updated_tools));
    Some(Value::Object(updated))
}

fn is_hosted_web_search_tool(value: &Value) -> bool {
    value
        .get("type")
        .and_then(Value::as_string)
        .is_some_and(|kind| kind == "web_search" || kind == "web_search_2025_08_26")
}

fn is_function_web_search_tool(value: &Value) -> bool {
    value.get("type").and_then(Value::as_string) == Some(&JsString::from("function"))
        && value.get("name").and_then(Value::as_string) == Some(&JsString::from("web_search"))
}

fn format_timeout(timeout_ms: u64) -> String {
    if timeout_ms.is_multiple_of(1000) {
        format!("{}s", timeout_ms / 1000)
    } else {
        format!("{timeout_ms}ms")
    }
}

fn text_result(text: JsString) -> AgentToolResult {
    AgentToolResult {
        content: vec![TextOrImageContent::Text(TextContent::new(text))],
        details: None,
        is_error: None,
        terminate: None,
    }
}

async fn execute_web_search(
    params: &Value,
    ctx: &ExtensionContext,
    signal: Option<CancellationToken>,
    run_search: &RunWebSearch,
    timeout_ms: u64,
) -> Result<JsString, HandlerError> {
    let query = params
        .get("query")
        .and_then(Value::as_string)
        .cloned()
        .unwrap_or_default()
        .trim();
    if query.is_empty() {
        return Err(HandlerError::new("web_search query is required"));
    }
    let Some(model) = ctx.model()? else {
        return Err(HandlerError::new("web_search requires an OpenAI Responses model"));
    };
    if !is_openai_hosted_web_search_model(Some(&model)) {
        return Err(HandlerError::new("web_search requires an OpenAI Responses model"));
    }
    let auth = ctx.model_registry()?.get_api_key_and_headers(&model).await;
    let (api_key, headers, env) = match auth {
        ResolvedRequestAuth::Ok {
            api_key, headers, env, ..
        } => (api_key, headers, env),
        ResolvedRequestAuth::Err { error } => return Err(HandlerError::new(error)),
    };
    run_search_with_timeout(
        WebSearchRequest {
            query: query.to_owned(),
            model,
            api_key,
            headers,
            env,
            signal,
        },
        ctx.clone(),
        run_search.clone(),
        timeout_ms,
    )
    .await
}

async fn run_search_with_timeout(
    request: WebSearchRequest,
    ctx: ExtensionContext,
    run_search: RunWebSearch,
    timeout_ms: u64,
) -> Result<JsString, HandlerError> {
    let combined = CancellationToken::new();
    let timed_out = Arc::new(std::sync::atomic::AtomicBool::new(false));
    if let Some(signal) = &request.signal {
        let combined = combined.clone();
        let signal = signal.clone();
        tokio::spawn(async move {
            signal.cancelled().await;
            combined.cancel();
        });
    }
    {
        let combined = combined.clone();
        let timed_out = Arc::clone(&timed_out);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(timeout_ms)).await;
            timed_out.store(true, std::sync::atomic::Ordering::SeqCst);
            combined.cancel();
        });
    }
    let mut request = request;
    request.signal = Some(combined.clone());
    let future = run_search(request, ctx);
    tokio::select! {
        result = future => result.map_err(HandlerError::new),
        _ = combined.cancelled() => {
            if timed_out.load(std::sync::atomic::Ordering::SeqCst) {
                Err(HandlerError::new(format!(
                    "OpenAI hosted web_search timed out after {}",
                    format_timeout(timeout_ms)
                )))
            } else {
                Err(HandlerError::new("OpenAI hosted web_search aborted"))
            }
        }
    }
}

async fn run_hosted_web_search(request: WebSearchRequest) -> Result<JsString, JsString> {
    let context = Context {
        system_prompt: Some(
            "You are a web search tool. Search the web and return concise, source-backed findings for the query."
                .to_owned()
                .into(),
        ),
        messages: vec![pi_ai::types::Message::User(UserMessage {
            role: UserRole::Tag,
            content: UserContent::Text(request.query.clone().into()),
            input_source: None,
            provider_native: None,
            timestamp: crate::core::compaction::now_millis(),
        })],
        tools: None,
    };
    let options = SimpleStreamOptions {
        stream: StreamOptions {
            request: ProviderRequestOptions {
                signal: request.signal.clone(),
                api_key: request.api_key.clone(),
                headers: request.headers.clone(),
                env: request.env.clone(),
                on_payload: Some(Arc::new(|payload, _model| {
                    Box::pin(async move { Ok(add_web_search_tool_to_payload(&payload)) })
                })),
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    };
    let stream = stream_simple(request.model, context, options);
    let message = stream.result().await.map_err(|failure| failure.message().to_owned())?;
    if matches!(
        message.stop_reason,
        pi_ai::types::StopReason::Error | pi_ai::types::StopReason::Aborted
    ) {
        return Err(message
            .error_message
            .unwrap_or_else(|| "OpenAI hosted web search failed".into()));
    }
    extract_text(&message)
}

fn extract_text(message: &pi_ai::types::AssistantMessage) -> Result<JsString, JsString> {
    let joined = JsString::join(
        message
            .content
            .iter()
            .filter_map(|block| match block {
                AssistantContent::Text(text) => Some(text.text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>(),
        "\n",
    );
    let text = joined.trim();
    if text.is_empty() {
        return Err("OpenAI hosted web search returned no text".into());
    }
    Ok(text.to_owned())
}

pub fn create_web_search_tool_definition(options: &WebSearchToolOptions) -> ToolDefinition {
    let run_search = options
        .run_search
        .clone()
        .unwrap_or_else(|| Arc::new(|request, _ctx| Box::pin(run_hosted_web_search(request))));
    let timeout_ms = options.timeout_ms.unwrap_or(DEFAULT_WEB_SEARCH_TIMEOUT_MS);
    ToolDefinition {
        name: "web_search".to_owned(),
        label: "Web Search".to_owned(),
        description: "Search the web using OpenAI Responses hosted web search and return a concise result summary."
            .to_owned(),
        prompt_snippet: Some("Search the web for current or external information.".into()),
        prompt_guidelines: Some(vec![
            "Use web_search when the user asks for current, external, or source-backed information that is not available in the repo or local context.".into(),
            "Do not use web_search for codebase investigation; use local file/search tools first.".into(),
        ]),
        approval_required: Some(true),
        approval_kind: None,
        parameters: schema_json!({
            "type": "object",
            "required": ["query"],
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query to send to the web search provider."
                }
            }
        }),
        constrained_sampling: None,
        render_shell: None,
        prepare_arguments: None,
        prepare_execution: None,
        execution_mode: None,
        execute: Arc::new(move |_tool_call_id, params, signal, _on_update, ctx| {
            let run_search = run_search.clone();
            Box::pin(async move {
                let text = execute_web_search(&params, &ctx, signal, &run_search, timeout_ms).await?;
                Ok(text_result(text))
            })
        }),
        render_call: None,
        render_result: None,
    }
}

/// Bundled `codex-web-search` extension.
pub fn codex_web_search_extension(pi: &ExtensionApi, options: &WebSearchToolOptions) -> Result<(), HandlerError> {
    pi.register_tool(create_web_search_tool_definition(options))
        .map_err(HandlerError::new)?;
    pi.on_before_provider_request(
        move |event: BeforeProviderRequestEvent,
              ctx: ExtensionContext|
              -> BoxFuture<'static, Result<Option<Value>, HandlerError>> {
            Box::pin(async move {
                if !is_openai_hosted_web_search_model(ctx.model()?.as_ref()) {
                    return Ok(None);
                }
                Ok(add_web_search_tool_to_payload(&event.payload))
            })
        },
    )
    .map_err(HandlerError::new)?;
    Ok(())
}

/// Registers the extension factory under `<first-party:codex-web-search>`.
pub fn register(registry: &mut crate::core::extensions::loader::ExtensionRegistry) {
    registry.register(
        "<first-party:codex-web-search>",
        Arc::new(|pi| {
            let result = codex_web_search_extension(pi, &WebSearchToolOptions::default());
            Box::pin(async move { result })
        }),
    );
}
