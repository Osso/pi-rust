use std::collections::BTreeSet;

use pi_ai::api::openai_responses_shared::{
    ConvertResponsesMessagesOptions, VecResponsesEventSource, convert_responses_messages, process_responses_stream,
};
use pi_ai::types::{AssistantMessage, Context, Model};
use pi_ai::utils::event_stream::create_assistant_message_event_stream;
use pi_ai::utils::hash::short_hash;
use serde_json::{Value, json};

fn model(provider: &str, api: &str, id: &str) -> Model {
    serde_json::from_value(json!({
        "id": id, "name": id, "api": api, "provider": provider,
        "baseUrl": "https://api.openai.com/v1", "reasoning": true,
        "input": ["text", "image"],
        "cost": {"input": 0.0, "output": 0.0, "cacheRead": 0.0, "cacheWrite": 0.0},
        "contextWindow": 128000, "maxTokens": 4096
    }))
    .expect("model")
}

fn usage() -> Value {
    json!({
        "input": 0.0, "output": 0.0, "cacheRead": 0.0, "cacheWrite": 0.0,
        "totalTokens": 0.0,
        "cost": {"input": 0.0, "output": 0.0, "cacheRead": 0.0, "cacheWrite": 0.0, "total": 0.0}
    })
}

fn context(value: Value) -> Context {
    serde_json::from_value(value).expect("context")
}

fn allowed() -> BTreeSet<String> {
    ["openai", "openai-codex", "opencode"]
        .into_iter()
        .map(str::to_owned)
        .collect()
}

#[test]
fn empty_tool_result_uses_no_tool_output_placeholder() {
    let model = model("openai", "openai-responses", "gpt-4o-mini");
    let context = context(json!({
        "messages": [
            {"role": "user", "content": "Run the command", "timestamp": 1},
            {"role": "assistant",
             "content": [{"type": "toolCall", "id": "tool-1", "name": "bash", "arguments": {"command": "true"}}],
             "api": "openai-responses", "provider": "openai", "model": "gpt-4o-mini",
             "usage": usage(), "stopReason": "toolUse", "timestamp": 2},
            {"role": "toolResult", "toolCallId": "tool-1", "toolName": "bash",
             "content": [{"type": "text", "text": ""}], "isError": false, "timestamp": 3}
        ]
    }));
    let messages = convert_responses_messages(&model, &context, &allowed(), None).expect("convert");
    let output = messages
        .iter()
        .find(|item| item["type"] == pi_js_compat::js_json::JsValue::String(("function_call_output").into()))
        .expect("function_call_output");
    assert_eq!(
        output["output"],
        pi_js_compat::js_json::JsValue::String(("(no tool output)").into())
    );
}

#[test]
fn orphan_tool_result_is_dropped() {
    let model = model("openai-codex", "openai-codex-responses", "gpt-5.5");
    let context = context(json!({
        "systemPrompt": "You are concise.",
        "messages": [
            {"role": "toolResult", "toolCallId": "call_orphan|fc_orphan", "toolName": "grep",
             "content": [{"type": "text", "text": "orphaned output"}], "isError": false, "timestamp": 1},
            {"role": "user", "content": "continue", "timestamp": 2}
        ]
    }));
    let messages = convert_responses_messages(&model, &context, &allowed(), None).expect("convert");
    assert!(
        !messages
            .iter()
            .any(|item| item["type"] == pi_js_compat::js_json::JsValue::String(("function_call_output").into()))
    );
    assert!(messages.contains(&pi_js_compat::js_json::JsValue::from(&json!({
        "role": "user", "content": [{"type": "input_text", "text": "continue"}]
    }))));
}

#[test]
fn foreign_tool_call_item_id_is_hashed_into_fc_shape() {
    let model = model("openai-codex", "openai-codex-responses", "gpt-5.5");
    let raw = "call_4VnzVawQXPB9MgYib7CiQFEY|I9b95oN1wD/cHXKTw3PpRkL6KkCtzTJhUxMouMWYwHeTo2j3htzfSk7YPx2vifiIM4g3A8XXyOj8q4Bt6SLUG7gqY1E3ELkrkVQNHglRfUmWj84lqxJY+Puieb3VKyX0FB+83TUzn91cDMF/4gzt990IzqVrc+nIb9RRscRD070Du16q1glydVjWR0SBJsE6TbY/esOjFpqplogQqrajm1eI++f3eLi73R6q7hVusY0QbeFySVxABCjhN0lXB04caBe1rzHjYzul6MAXj7uq+0r17VLq+yrtyYhN12wkmFqHeqTyEei6EFPbMy24Nc+IbJlkP0OCg02W+gOnyBFcbi2ctvJFSOhSjt1CqBdqCnnhwUqXjbWiT0wh3DmLScRgTHmGkaI+oAcQQjfic65nxj+TnEkReA==";
    let context = context(json!({
        "messages": [
            {"role": "user", "content": "Use the tool.", "timestamp": 1},
            {"role": "assistant",
             "content": [{"type": "toolCall", "id": raw, "name": "edit", "arguments": {"path": "src/styles/app.css"}}],
             "api": "openai-responses", "provider": "github-copilot", "model": "gpt-5.5",
             "usage": usage(), "stopReason": "toolUse", "timestamp": 2},
            {"role": "toolResult", "toolCallId": raw, "toolName": "edit",
             "content": [{"type": "text", "text": "ok"}], "isError": false, "timestamp": 3}
        ]
    }));
    let messages = convert_responses_messages(&model, &context, &allowed(), None).expect("convert");
    let function_call = messages
        .iter()
        .find(|item| item["type"] == pi_js_compat::js_json::JsValue::String(("function_call").into()))
        .expect("function_call");
    let expected = format!("fc_{}", short_hash(raw.split('|').nth(1).expect("item id")));
    assert_eq!(function_call["id"], expected);
    assert!(expected.len() <= 64);
}

#[test]
fn fallback_message_ids_are_unique_per_text_block() {
    let model = model("openai-codex", "openai-codex-responses", "gpt-5.5");
    let context = context(json!({
        "systemPrompt": "You are concise.",
        "messages": [
            {"role": "user", "content": "hello", "timestamp": 1},
            {"role": "assistant",
             "content": [{"type": "thinking", "thinking": "private reasoning"},
                         {"type": "text", "text": "first"}, {"type": "text", "text": "second"}],
             "api": "anthropic-messages", "provider": "anthropic", "model": "claude-opus-4-8",
             "usage": usage(), "stopReason": "stop", "timestamp": 2}
        ]
    }));
    let messages = convert_responses_messages(&model, &context, &allowed(), None).expect("convert");
    let ids = messages
        .iter()
        .filter(|item| item["type"] == pi_js_compat::js_json::JsValue::String(("message").into()))
        .map(|item| item["id"].as_str().expect("id").to_owned())
        .collect::<Vec<_>>();
    assert_eq!(ids, ["msg_pi_1", "msg_pi_1_1", "msg_pi_1_2"]);
}

#[test]
fn provider_native_context_is_inserted_before_regular_messages() {
    let model = model("openai", "openai-responses", "gpt-4.1-mini");
    let native_items = json!([
        {"role": "user", "content": [{"type": "input_text", "text": "compacted"}]},
        {"type": "compaction", "encrypted_content": "encrypted"}
    ]);
    let context = context(json!({
        "messages": [
            {"role": "user", "content": [{"type": "text", "text": "synthetic summary"}],
             "providerNative": {"provider": "openai", "api": "openai-responses",
                "format": "openai.responses.input", "value": native_items},
             "timestamp": 1},
            {"role": "user", "content": "recent", "timestamp": 2}
        ]
    }));
    let messages = convert_responses_messages(
        &model,
        &context,
        &allowed(),
        Some(&ConvertResponsesMessagesOptions {
            include_system_prompt: Some(false),
            ..Default::default()
        }),
    )
    .expect("convert");
    let mut expected = native_items.as_array().expect("native").clone();
    expected.push(json!({"role": "user", "content": [{"type": "input_text", "text": "recent"}]}));
    assert_eq!(Value::Array(messages), Value::Array(expected));
}

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(future)
}

#[test]
fn stream_without_terminal_event_fails() {
    let model = model("openai", "openai-responses", "gpt-5.5");
    let mut output = AssistantMessage::empty("openai-responses", "openai", "gpt-5.5", 1234);
    let mut source = VecResponsesEventSource::new(vec![json!({
        "type": "response.created", "response": {"id": "resp_early"}
    })]);
    let (sender, _stream) = create_assistant_message_event_stream();
    let failure = block_on(process_responses_stream(
        &mut source,
        &mut output,
        &sender,
        &model,
        None,
    ))
    .expect_err("terminal event is required");
    let message = match failure {
        pi_ai::api::openai_responses_shared::ResponsesStreamFailure::Message(message) => message,
        _ => panic!("expected message failure"),
    };
    assert_eq!(
        message,
        "OpenAI Responses stream ended before a terminal response event"
    );
}

#[test]
fn function_call_arguments_are_parsed_and_scratch_state_removed() {
    let model = model("openai", "openai-responses", "gpt-5.5");
    let mut output = AssistantMessage::empty("openai-responses", "openai", "gpt-5.5", 1234);
    let events = vec![
        json!({"type": "response.output_item.added", "output_index": 0,
            "item": {"type": "function_call", "id": "fc_test", "call_id": "call_test", "name": "edit", "arguments": ""}}),
        json!({"type": "response.function_call_arguments.delta", "output_index": 0, "delta": "{\"path\":\"README.md\""}),
        json!({"type": "response.function_call_arguments.delta", "output_index": 0, "delta": ",\"content\":\"updated\"}"}),
        json!({"type": "response.function_call_arguments.done", "output_index": 0,
            "arguments": "{\"path\":\"README.md\",\"content\":\"updated\"}"}),
        json!({"type": "response.output_item.done", "output_index": 0,
            "item": {"type": "function_call", "id": "fc_test", "call_id": "call_test", "name": "edit",
                     "arguments": "{\"path\":\"README.md\",\"content\":\"updated\"}"}}),
        json!({"type": "response.completed", "response": {"id": "resp_test", "status": "completed"}}),
    ];
    let mut source = VecResponsesEventSource::new(events);
    let (sender, _stream) = create_assistant_message_event_stream();
    block_on(process_responses_stream(
        &mut source,
        &mut output,
        &sender,
        &model,
        None,
    ))
    .expect("stream");
    let assistant = output;
    assert_eq!(assistant.content.len(), 1);
    let pi_ai::types::AssistantContent::ToolCall(tool_call) = &assistant.content[0] else {
        panic!("expected tool call");
    };
    assert_eq!(
        tool_call.arguments["path"],
        pi_js_compat::js_json::JsValue::String(("README.md").into())
    );
    assert_eq!(
        tool_call.arguments["content"],
        pi_js_compat::js_json::JsValue::String(("updated").into())
    );
}
