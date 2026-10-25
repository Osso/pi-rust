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
use pi_js_compat::js_format;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use pi_js_compat::js_json::JsValue as Value;
use std::sync::Arc;

pub(super) fn start(
    runtime: OAuthRuntime,
    model: Model,
    context: Context,
    options: SimpleStreamOptions,
) -> AssistantMessageEventStream {
    let (sender, stream) = create_assistant_message_event_stream();
    tokio::spawn(async move {
        let mut output = pi_js_compat::js_json::to_value(&AssistantMessage::empty(
            model.api.clone(),
            model.provider.clone(),
            model.id.clone(),
            runtime.now(),
        ))
        .expect("empty assistant");
        let result = drive(&sender, &runtime, &model, &context, &options, &mut output).await;
        if let Err(error) = result {
            for block in output["content"].as_array_mut().expect("content") {
                block.as_object_mut().expect("block").shift_remove("index");
            }
            let reason = if options.stream.request.signal.as_ref().is_some_and(|s| s.aborted()) {
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
    sender.push(pi_js_compat::js_json::from_value(&event).expect("example event fits native event contract"));
}
fn update_usage(model: &Model, output: &mut Value, usage: &JsValue) {
    let mut parsed = Usage::default();
    parsed.input = usage["input_tokens"].as_f64().unwrap_or(0.0);
    parsed.output = usage["output_tokens"].as_f64().unwrap_or(0.0);
    parsed.cache_read = usage["cache_read_input_tokens"].as_f64().unwrap_or(0.0);
    parsed.cache_write = usage["cache_creation_input_tokens"].as_f64().unwrap_or(0.0);
    parsed.total_tokens = parsed.input + parsed.output + parsed.cache_read + parsed.cache_write;
    parsed.cost = calculate_cost(model, &parsed);
    output["usage"] = pi_js_compat::js_json::to_value(&parsed).expect("usage");
}
fn stop_reason(reason: &JsString) -> &str {
    match reason {
        reason if reason == "end_turn" || reason == "pause_turn" || reason == "stop_sequence" => "stop",
        reason if reason == "max_tokens" => "length",
        reason if reason == "tool_use" => "toolUse",
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
) -> Result<(), JsString> {
    let key = options.stream.request.api_key.clone().unwrap_or_default();
    let oauth = oauth_key(&key);
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
            JsString::from(format!("claude-code-20250219,oauth-2025-04-20,{beta}"))
        } else {
            beta.into()
        }),
    );
    if oauth {
        headers.insert("user-agent".into(), Some("claude-cli/2.1.2 (external, cli)".into()));
        headers.insert("x-app".into(), Some("cli".into()));
    }
    let auth = if oauth {
        Auth::Bearer(key.clone())
    } else {
        Auth::ApiKey(key)
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
            pi_ai::utils::provider_retry::ProviderRequestError::Abort(_) => JsString::from("Request was aborted"),
            pi_ai::utils::provider_retry::ProviderRequestError::Provider(e) => e.message,
            pi_ai::utils::provider_retry::ProviderRequestError::Other(e) => e.message,
            pi_ai::utils::provider_retry::ProviderRequestError::RetryDelayLimit(e) => JsString::from(e.to_string()),
        })?;
    let mut reader = AnthropicEventReader::new(response.body, options.stream.request.signal.clone());
    while let Some(event) = reader.next_event().await? {
        match event["type"].as_string() {
            Some(tag) if tag == "message_start" => update_usage(model, output, &event["message"]["usage"]),
            Some(tag) if tag == "message_delta" => {
                if let Some(reason) = event["delta"]["stop_reason"].as_string().filter(|r| !r.is_empty()) {
                    output["stopReason"] = json!(stop_reason(reason));
                }
                update_usage(model, output, &event["usage"]);
            }
            Some(tag) if tag == "content_block_start" => begin_block(sender, output, &event, context, oauth),
            Some(tag) if tag == "content_block_delta" => delta_block(sender, output, &event)?,
            Some(tag) if tag == "content_block_stop" => end_block(sender, output, &event)?,
            _ => {}
        }
    }
    if options.stream.request.signal.as_ref().is_some_and(|s| s.aborted()) {
        return Err("Request was aborted".into());
    }
    Ok(())
}
fn begin_block(
    sender: &AssistantMessageEventSender,
    output: &mut Value,
    event: &JsValue,
    context: &Context,
    oauth: bool,
) {
    let source = &event["content_block"];
    let (block, kind) = match source["type"].as_string() {
        Some(tag) if tag == "text" => (json!({"type":"text","text":"","index":event["index"]}), "text_start"),
        Some(tag) if tag == "thinking" => (
            json!({"type":"thinking","thinking":"","thinkingSignature":"","index":event["index"]}),
            "thinking_start",
        ),
        Some(tag) if tag == "tool_use" => (
            json!({"type":"toolCall","id":source["id"],"name":original_tool_name(&source["name"].as_string().cloned().unwrap_or_else(JsString::default),context,oauth),"arguments":{},"partialJson":"","index":event["index"]}),
            "toolcall_start",
        ),
        _ => return,
    };
    let blocks = output["content"].as_array_mut().expect("content");
    blocks.push(block);
    let index = blocks.len() - 1;
    push(sender, json!({"type":kind,"contentIndex":index,"partial":output}));
}
fn block_index(output: &Value, event: &JsValue) -> Option<usize> {
    output["content"]
        .as_array()
        .expect("content")
        .iter()
        .position(|b| b.get("index") == event.get("index"))
}
fn parse_arguments(block: &mut Value) -> Result<(), JsString> {
    if let Ok(value) = pi_js_compat::js_json::parse_lossless(block["partialJson"].as_string().expect("JSON buffer")) {
        if !value.is_object() {
            return Err("Tool arguments must be a JSON object".into());
        }
        block["arguments"] = value;
    }
    Ok(())
}
fn delta_block(sender: &AssistantMessageEventSender, output: &mut Value, event: &JsValue) -> Result<(), JsString> {
    let Some(index) = block_index(output, event) else {
        return Ok(());
    };
    let block = &mut output["content"].as_array_mut().expect("content array")[index];
    let delta = &event["delta"];
    let (kind, field, delta_field) = match (delta["type"].as_string(), block["type"].as_string()) {
        (Some(delta_type), Some(block_type)) if delta_type == "text_delta" && block_type == "text" => {
            ("text_delta", "text", "text")
        }
        (Some(delta_type), Some(block_type)) if delta_type == "thinking_delta" && block_type == "thinking" => {
            ("thinking_delta", "thinking", "thinking")
        }
        (Some(delta_type), Some(block_type)) if delta_type == "input_json_delta" && block_type == "toolCall" => {
            ("toolcall_delta", "partialJson", "partial_json")
        }
        (Some(delta_type), Some(block_type)) if delta_type == "signature_delta" && block_type == "thinking" => {
            block["thinkingSignature"] = json!(js_format!(
                "{}{}",
                block["thinkingSignature"]
                    .as_string()
                    .cloned()
                    .unwrap_or_else(JsString::default),
                delta["signature"]
                    .as_string()
                    .cloned()
                    .unwrap_or_else(JsString::default)
            ));
            return Ok(());
        }
        _ => return Ok(()),
    };
    block[field] = json!(js_format!(
        "{}{}",
        block[field].as_string().expect("stream string"),
        delta[delta_field].as_string().expect("SDK delta string")
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
fn end_block(sender: &AssistantMessageEventSender, output: &mut Value, event: &JsValue) -> Result<(), JsString> {
    let Some(index) = block_index(output, event) else {
        return Ok(());
    };
    let block = &mut output["content"].as_array_mut().expect("content array")[index];
    block.as_object_mut().expect("block").shift_remove("index");
    let emitted = match block["type"].as_string() {
        Some(tag) if tag == "text" => json!({"type":"text_end","contentIndex":index,"content":block["text"]}),
        Some(tag) if tag == "thinking" => {
            json!({"type":"thinking_end","contentIndex":index,"content":block["thinking"]})
        }
        Some(tag) if tag == "toolCall" => {
            parse_arguments(block)?;
            block.as_object_mut().expect("tool").shift_remove("partialJson");
            json!({"type":"toolcall_end","contentIndex":index,"toolCall":block})
        }
        _ => return Ok(()),
    };
    let mut emitted = emitted;
    emitted["partial"] = output.clone();
    push(sender, emitted);
    Ok(())
}
