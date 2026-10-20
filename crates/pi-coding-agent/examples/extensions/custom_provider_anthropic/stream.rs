use super::request::{build_params, oauth_key, original_tool_name};
use pi_ai::api::anthropic_messages::AnthropicEventReader;
use pi_ai::api::anthropic_sdk::{self, Auth};
use pi_ai::auth::oauth::support::OAuthRuntime;
use pi_ai::models::calculate_cost;
use pi_ai::request::SimpleStreamOptions;
use pi_ai::types::{AssistantMessage, Context, Model, ProviderHeaders, Usage};
use pi_ai::utils::event_stream::{
    AssistantMessageEventSender, AssistantMessageEventStream, create_assistant_message_event_stream,
};
use serde_json::{Value, json};
use std::sync::Arc;

pub(super) fn start(
    runtime: OAuthRuntime,
    model: Model,
    context: Context,
    options: SimpleStreamOptions,
) -> AssistantMessageEventStream {
    let (sender, stream) = create_assistant_message_event_stream();
    tokio::spawn(async move {
        let mut output = serde_json::to_value(AssistantMessage::empty(
            model.api.clone(),
            model.provider.clone(),
            model.id.clone(),
            runtime.now() as i64,
        ))
        .expect("empty assistant");
        let result = drive(&sender, &runtime, &model, &context, &options, &mut output).await;
        if let Err(error) = result {
            for block in output["content"].as_array_mut().expect("content") {
                block.as_object_mut().expect("block").remove("index");
            }
            let reason = if options.stream.request.signal.as_ref().is_some_and(|s| s.is_cancelled()) {
                "aborted"
            } else {
                "error"
            };
            output["stopReason"] = json!(reason);
            output["errorMessage"] = json!(error);
            push(&sender, json!({"type":"error","reason":reason,"error":output}));
        } else {
            push(
                &sender,
                json!({"type":"done","reason":output["stopReason"],"message":output}),
            );
        }
        sender.end(None);
    });
    stream
}
fn push(sender: &AssistantMessageEventSender, event: Value) {
    sender.push(serde_json::from_value(event).expect("example event fits native event contract"));
}
fn update_usage(model: &Model, output: &mut Value, usage: &Value) {
    let mut parsed = Usage::default();
    parsed.input = usage["input_tokens"].as_f64().unwrap_or(0.0);
    parsed.output = usage["output_tokens"].as_f64().unwrap_or(0.0);
    parsed.cache_read = usage["cache_read_input_tokens"].as_f64().unwrap_or(0.0);
    parsed.cache_write = usage["cache_creation_input_tokens"].as_f64().unwrap_or(0.0);
    parsed.total_tokens = parsed.input + parsed.output + parsed.cache_read + parsed.cache_write;
    parsed.cost = calculate_cost(model, &parsed);
    output["usage"] = serde_json::to_value(parsed).expect("usage");
}
fn stop_reason(reason: &str) -> &str {
    match reason {
        "end_turn" | "pause_turn" | "stop_sequence" => "stop",
        "max_tokens" => "length",
        "tool_use" => "toolUse",
        _ => "error",
    }
}

async fn drive(
    sender: &AssistantMessageEventSender,
    runtime: &OAuthRuntime,
    model: &Model,
    context: &Context,
    options: &SimpleStreamOptions,
    output: &mut Value,
) -> Result<(), String> {
    let key = options.stream.request.api_key.as_deref().unwrap_or_default();
    let oauth = oauth_key(key);
    let beta = "fine-grained-tool-streaming-2025-05-14,interleaved-thinking-2025-05-14";
    let mut headers = ProviderHeaders::new();
    for (name, value) in [
        ("accept", "application/json"),
        ("anthropic-dangerous-direct-browser-access", "true"),
        ("X-Stainless-Helper-Method", "stream"),
    ] {
        headers.insert(name.into(), Some(value.into()));
    }
    headers.insert(
        "anthropic-beta".into(),
        Some(if oauth {
            format!("claude-code-20250219,oauth-2025-04-20,{beta}")
        } else {
            beta.into()
        }),
    );
    if oauth {
        headers.insert("user-agent".into(), Some("claude-cli/2.1.2 (external, cli)".into()));
        headers.insert("x-app".into(), Some("cli".into()));
    }
    let auth = if oauth {
        Auth::Bearer(key.into())
    } else {
        Auth::ApiKey(key.into())
    };
    let params = build_params(model, context, options, oauth);
    let request = anthropic_sdk::build_request(model, &params, auth, &headers, None).map_err(|e| e.message)?;
    // messages.stream() returns before its asynchronous HTTP request completes.
    push(sender, json!({"type":"start","partial":output}));
    let fetch = options
        .stream
        .request
        .fetch
        .clone()
        .unwrap_or_else(|| Arc::clone(&runtime.fetch));
    let response = anthropic_sdk::execute(&request, fetch, options.stream.request.signal.clone())
        .await
        .map_err(|e| match e {
            pi_ai::utils::provider_retry::ProviderRequestError::Abort(_) => "Request was aborted".to_owned(),
            pi_ai::utils::provider_retry::ProviderRequestError::Provider(e) => e.message,
            pi_ai::utils::provider_retry::ProviderRequestError::Other(e) => e.message,
            pi_ai::utils::provider_retry::ProviderRequestError::RetryDelayLimit(e) => e.to_string(),
        })?;
    let mut reader = AnthropicEventReader::new(response.body, options.stream.request.signal.clone());
    while let Some(event) = reader.next_event().await? {
        match event["type"].as_str() {
            Some("message_start") => update_usage(model, output, &event["message"]["usage"]),
            Some("message_delta") => {
                if let Some(reason) = event["delta"]["stop_reason"].as_str().filter(|r| !r.is_empty()) {
                    output["stopReason"] = json!(stop_reason(reason));
                }
                update_usage(model, output, &event["usage"]);
            }
            Some("content_block_start") => begin_block(sender, output, &event, context, oauth),
            Some("content_block_delta") => delta_block(sender, output, &event)?,
            Some("content_block_stop") => end_block(sender, output, &event)?,
            _ => {}
        }
    }
    if options.stream.request.signal.as_ref().is_some_and(|s| s.is_cancelled()) {
        return Err("Request was aborted".into());
    }
    match output["stopReason"].as_str() {
        Some("pending") => Err("Anthropic stream ended without a stop reason".into()),
        Some("error" | "aborted") => Err("An unknown error occurred".into()),
        _ => Ok(()),
    }
}
fn begin_block(
    sender: &AssistantMessageEventSender,
    output: &mut Value,
    event: &Value,
    context: &Context,
    oauth: bool,
) {
    let source = &event["content_block"];
    let (block, kind) = match source["type"].as_str() {
        Some("text") => (json!({"type":"text","text":"","index":event["index"]}), "text_start"),
        Some("thinking") => (
            json!({"type":"thinking","thinking":"","thinkingSignature":"","index":event["index"]}),
            "thinking_start",
        ),
        Some("tool_use") => (
            json!({"type":"toolCall","id":source["id"],"name":original_tool_name(source["name"].as_str().unwrap_or_default(),context,oauth),"arguments":{},"partialJson":"","index":event["index"]}),
            "toolcall_start",
        ),
        _ => return,
    };
    let blocks = output["content"].as_array_mut().expect("content");
    blocks.push(block);
    let index = blocks.len() - 1;
    push(sender, json!({"type":kind,"contentIndex":index,"partial":output}));
}
fn block_index(output: &Value, event: &Value) -> Option<usize> {
    output["content"]
        .as_array()
        .expect("content")
        .iter()
        .position(|b| b.get("index") == event.get("index"))
}
fn parse_arguments(block: &mut Value) -> Result<(), String> {
    if let Ok(value) = pi_js_compat::js_json::parse(block["partialJson"].as_str().expect("JSON buffer")) {
        if !value.is_object() {
            return Err("Tool arguments must be a JSON object".into());
        }
        block["arguments"] = value;
    }
    Ok(())
}
fn delta_block(sender: &AssistantMessageEventSender, output: &mut Value, event: &Value) -> Result<(), String> {
    let Some(index) = block_index(output, event) else {
        return Ok(());
    };
    let block = &mut output["content"][index];
    let delta = &event["delta"];
    let (kind, field, delta_field) = match (delta["type"].as_str(), block["type"].as_str()) {
        (Some("text_delta"), Some("text")) => ("text_delta", "text", "text"),
        (Some("thinking_delta"), Some("thinking")) => ("thinking_delta", "thinking", "thinking"),
        (Some("input_json_delta"), Some("toolCall")) => ("toolcall_delta", "partialJson", "partial_json"),
        (Some("signature_delta"), Some("thinking")) => {
            block["thinkingSignature"] = json!(format!(
                "{}{}",
                block["thinkingSignature"].as_str().unwrap_or_default(),
                delta["signature"].as_str().unwrap_or_default()
            ));
            return Ok(());
        }
        _ => return Ok(()),
    };
    block[field] = json!(format!(
        "{}{}",
        block[field].as_str().expect("stream string"),
        delta[delta_field].as_str().expect("SDK delta string")
    ));
    if field == "partialJson" {
        parse_arguments(block)?;
    }
    push(
        sender,
        json!({"type":kind,"contentIndex":index,"delta":delta[delta_field],"partial":output}),
    );
    Ok(())
}
fn end_block(sender: &AssistantMessageEventSender, output: &mut Value, event: &Value) -> Result<(), String> {
    let Some(index) = block_index(output, event) else {
        return Ok(());
    };
    let block = &mut output["content"][index];
    block.as_object_mut().expect("block").remove("index");
    let emitted = match block["type"].as_str() {
        Some("text") => json!({"type":"text_end","contentIndex":index,"content":block["text"]}),
        Some("thinking") => json!({"type":"thinking_end","contentIndex":index,"content":block["thinking"]}),
        Some("toolCall") => {
            parse_arguments(block)?;
            block.as_object_mut().expect("tool").remove("partialJson");
            json!({"type":"toolcall_end","contentIndex":index,"toolCall":block})
        }
        _ => return Ok(()),
    };
    let mut emitted = emitted;
    emitted["partial"] = output.clone();
    push(sender, emitted);
    Ok(())
}
