use crate::shared_oracle as oracle;

use std::collections::BTreeSet;

use oracle::TsOracle;
use pi_ai::api::openai_responses_shared::{
    ConvertResponsesMessagesOptions, ConvertResponsesToolsOptions, VecResponsesEventSource, convert_responses_messages,
    convert_responses_tools, process_responses_stream,
};
use pi_ai::types::{AssistantMessage, Context, Model, Tool};
use pi_ai::utils::event_stream::create_assistant_message_event_stream;
use serde_json::{Value, json};

fn model(provider: &str, api: &str, id: &str, reasoning: bool) -> Value {
    json!({
        "id": id,
        "name": id,
        "api": api,
        "provider": provider,
        "baseUrl": if provider == "openrouter" { "https://openrouter.ai/api/v1" } else { "https://api.openai.com/v1" },
        "reasoning": reasoning,
        "input": ["text", "image"],
        "cost": {"input": 1.0, "output": 2.0, "cacheRead": 0.5, "cacheWrite": 0.75},
        "contextWindow": 128000,
        "maxTokens": 4096
    })
}

fn usage() -> Value {
    json!({
        "input": 0.0, "output": 0.0, "cacheRead": 0.0, "cacheWrite": 0.0,
        "totalTokens": 0.0,
        "cost": {"input": 0.0, "output": 0.0, "cacheRead": 0.0, "cacheWrite": 0.0, "total": 0.0}
    })
}

fn assistant(content: Value, api: &str, provider: &str, model_id: &str) -> Value {
    json!({
        "role": "assistant",
        "content": content,
        "api": api,
        "provider": provider,
        "model": model_id,
        "usage": usage(),
        "stopReason": "toolUse",
        "timestamp": 1000
    })
}

fn convert_options(value: &Value) -> ConvertResponsesMessagesOptions {
    ConvertResponsesMessagesOptions {
        include_system_prompt: value.get("includeSystemPrompt").and_then(Value::as_bool),
    }
}

fn convert_tools_options(value: &Value) -> ConvertResponsesToolsOptions {
    ConvertResponsesToolsOptions {
        strict: value.get("strict").map(|strict| strict.as_bool()),
    }
}

fn oracle_response(oracle: &mut TsOracle, request: Value) -> Value {
    let response = oracle.request(&request);
    assert_eq!(response["ok"], Value::Bool(true), "oracle failed: {response}");
    response["output"].clone()
}

fn compare_conversion(
    oracle: &mut TsOracle,
    id: i64,
    model: &Value,
    context: &Value,
    allowed: &[&str],
    options: Option<&Value>,
) {
    let mut request = json!({
        "id": id,
        "operation": "convertMessages",
        "model": model,
        "context": context,
        "allowed": allowed,
    });
    if let Some(options) = options {
        request["options"] = options.clone();
    }
    let expected = oracle_response(oracle, request);

    let parsed_model: Model = serde_json::from_value(model.clone()).expect("model");
    let parsed_context: Context = serde_json::from_value(context.clone()).expect("context");
    let allowed_set = allowed.iter().map(|value| (*value).to_owned()).collect::<BTreeSet<_>>();
    let parsed_options = options.map(convert_options);
    let actual = convert_responses_messages(&parsed_model, &parsed_context, &allowed_set, parsed_options.as_ref())
        .map(|value| json!(value))
        .unwrap_or_else(|error| json!({"error": error}));
    assert_eq!(actual, expected, "convertMessages scenario {id}");
}

fn compare_tools(oracle: &mut TsOracle, id: i64, tools: &Value, options: &Value) {
    let expected = oracle_response(
        oracle,
        json!({"id": id, "operation": "convertTools", "tools": tools, "options": options}),
    );
    let parsed_tools: Vec<Tool> = serde_json::from_value(tools.clone()).expect("tools");
    let parsed_options = convert_tools_options(options);
    let actual = convert_responses_tools(&parsed_tools, Some(&parsed_options));
    assert_eq!(
        actual,
        pi_js_compat::js_json::JsValue::from(expected),
        "convertTools scenario {id}"
    );
}

fn compare_stream(
    oracle: &mut TsOracle,
    id: i64,
    model: &Value,
    events: &Value,
    output: &Value,
    options: Option<&Value>,
) {
    let mut request = json!({
        "id": id,
        "operation": "processStream",
        "model": model,
        "events": events,
        "output": output,
    });
    if let Some(options) = options {
        request["options"] = options.clone();
    }
    let response = oracle.request(&request);
    let expected_events = response["output"]["events"].clone();
    let expected_output = response["output"]["output"].clone();
    let expected_error = response.get("error").and_then(Value::as_str).map(str::to_owned);

    let parsed_model: Model = serde_json::from_value(model.clone()).expect("model");
    let mut parsed_output: AssistantMessage = serde_json::from_value(output.clone()).expect("output");
    let event_values = events.as_array().expect("events").clone();
    let mut source = VecResponsesEventSource::new(event_values);
    let options = options.map(|_| pi_ai::api::openai_responses_shared::OpenAIResponsesStreamOptions::default());
    let (sender, mut stream) = create_assistant_message_event_stream();
    let result = block_on(process_responses_stream(
        &mut source,
        &mut parsed_output,
        &sender,
        &parsed_model,
        options.as_ref(),
    ));
    sender.end(None);
    let mut actual_events = Vec::new();
    while let Some(Ok(event)) = block_on(stream.next()) {
        actual_events.push(serde_json::to_value(event).expect("event"));
    }
    let actual_output = serde_json::to_value(&parsed_output).expect("output");
    let actual_error = match result {
        Ok(()) => None,
        Err(pi_ai::api::openai_responses_shared::ResponsesStreamFailure::Message(message)) => Some(message),
        Err(pi_ai::api::openai_responses_shared::ResponsesStreamFailure::Source(error)) => match error {},
    };

    if expected_error.is_some() {
        assert_eq!(
            actual_error,
            (expected_error).map(pi_js_compat::js_json::JsString::from),
            "processStream error scenario {id}"
        );
        return;
    }

    assert_json_eq(
        &Value::Array(actual_events),
        &expected_events,
        &format!("processStream events scenario {id}"),
    );
    assert_json_eq(
        &actual_output,
        &expected_output,
        &format!("processStream output scenario {id}"),
    );
    assert_eq!(
        actual_error,
        (expected_error).map(pi_js_compat::js_json::JsString::from),
        "processStream error scenario {id}"
    );
}

fn assert_json_eq(actual: &Value, expected: &Value, path: &str) {
    match (actual, expected) {
        (Value::Number(actual), Value::Number(expected)) => {
            assert_eq!(actual.as_f64(), expected.as_f64(), "number mismatch at {path}");
        }
        (Value::Array(actual), Value::Array(expected)) => {
            assert_eq!(actual.len(), expected.len(), "array length mismatch at {path}");
            for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
                assert_json_eq(actual, expected, &format!("{path}[{index}]"));
            }
        }
        (Value::Object(actual), Value::Object(expected)) => {
            assert_eq!(
                actual.keys().collect::<BTreeSet<_>>(),
                expected.keys().collect::<BTreeSet<_>>(),
                "object keys mismatch at {path}"
            );
            for (key, expected) in expected {
                assert_json_eq(&actual[key], expected, &format!("{path}.{key}"));
            }
        }
        _ => assert_eq!(actual, expected, "value mismatch at {path}"),
    }
}

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(future)
}

#[test]
fn differential_openai_responses_shared_conversion() {
    let mut oracle = TsOracle::spawn(
        "openai-responses-shared-oracle.ts",
        "packages/ai/src/api/openai-responses-shared.ts",
    );
    let openai_model = model("openai", "openai-responses", "gpt-5.5", true);
    let codex_model = model("openai-codex", "openai-codex-responses", "gpt-5.5", true);
    let anthropic_model = model("anthropic", "anthropic-messages", "claude-opus-4-8", false);

    let text_context = json!({
        "systemPrompt": "You are concise.",
        "messages": [
            {"role": "user", "content": "hello", "timestamp": 1},
            assistant(json!([{"type": "thinking", "thinking": "private reasoning"},
                {"type": "text", "text": "visible answer"}]), "openai-responses", "openai", "gpt-5.5")
        ]
    });
    compare_conversion(&mut oracle, 0, &openai_model, &text_context, &["openai"], None);
    compare_conversion(
        &mut oracle,
        1,
        &openai_model,
        &text_context,
        &["openai"],
        Some(&json!({"includeSystemPrompt": false})),
    );

    let two_text_context = json!({
        "messages": [
            {"role": "user", "content": "hello", "timestamp": 1},
            assistant(json!([{"type": "text", "text": "first"}, {"type": "text", "text": "second"}]),
                "anthropic-messages", "anthropic", "claude-opus-4-8")
        ]
    });
    compare_conversion(
        &mut oracle,
        2,
        &codex_model,
        &two_text_context,
        &["openai", "openai-codex", "opencode"],
        None,
    );

    let tool_context = json!({
        "systemPrompt": "sys",
        "messages": [
            {"role": "user", "content": "run", "timestamp": 1},
            assistant(json!([{"type": "toolCall", "id": "call_1|fc_1", "name": "bash",
                "arguments": {"command": "true"}}]), "openai-responses", "openai", "gpt-5.5"),
            {"role": "toolResult", "toolCallId": "call_1|fc_1", "toolName": "bash",
             "content": [{"type": "text", "text": ""}], "isError": false, "timestamp": 2}
        ]
    });
    compare_conversion(
        &mut oracle,
        3,
        &openai_model,
        &tool_context,
        &["openai", "openai-codex", "opencode"],
        None,
    );

    let foreign_id = "call_4VnzVawQXPB9MgYib7CiQFEY|I9b95oN1wD/cHXKTw3PpRkL6KkCtzTJhUxMouMWYwHeTo2j3htzfSk7YPx2vifiIM4g3A8XXyOj8q4Bt6SLUG7gqY1E3ELkrkVQNHglRfUmWj84lqxJY+Puieb3VKyX0FB+83TUzn91cDMF/4gzt990IzqVrc+nIb9RRscRD070Du16q1glydVjWR0SBJsE6TbY/esOjFpqplogQqrajm1eI++f3eLi73R6q7hVusY0QbeFySVxABCjhN0lXB04caBe1rzHjYzul6MAXj7uq+0r17VLq+yrtyYhN12wkmFqHeqTyEei6EFPbMy24Nc+IbJlkP0OCg02W+gOnyBFcbi2ctvJFSOhSjt1CqBdqCnnhwUqXjbWiT0wh3DmLScRgTHmGkaI+oAcQQjfic65nxj+TnEkReA==";
    let foreign_context = json!({
        "systemPrompt": "You are concise.",
        "messages": [
            {"role": "user", "content": "Use the tool.", "timestamp": 1},
            assistant(json!([{"type": "toolCall", "id": foreign_id, "name": "edit",
                "arguments": {"path": "src/styles/app.css"}}]), "openai-responses", "github-copilot", "gpt-5.5"),
            {"role": "toolResult", "toolCallId": foreign_id, "toolName": "edit",
             "content": [{"type": "text", "text": "ok"}], "isError": false, "timestamp": 2}
        ]
    });
    compare_conversion(
        &mut oracle,
        4,
        &codex_model,
        &foreign_context,
        &["openai", "openai-codex", "opencode"],
        None,
    );

    let orphan_context = json!({
        "systemPrompt": "You are concise.",
        "messages": [
            {"role": "toolResult", "toolCallId": "call_orphan|fc_orphan", "toolName": "grep",
             "content": [{"type": "text", "text": "orphaned output"}], "isError": false, "timestamp": 1},
            {"role": "user", "content": "continue", "timestamp": 2}
        ]
    });
    compare_conversion(
        &mut oracle,
        5,
        &codex_model,
        &orphan_context,
        &["openai", "openai-codex", "opencode"],
        None,
    );

    let native_context = json!({
        "messages": [
            {"role": "user", "content": [{"type": "text", "text": "synthetic summary"}],
             "providerNative": {"provider": "openai", "api": "openai-responses",
                "format": "openai.responses.input",
                "value": [{"role": "user", "content": [{"type": "input_text", "text": "compacted"}]},
                          {"type": "compaction", "encrypted_content": "encrypted"}]},
             "timestamp": 1},
            {"role": "user", "content": "recent", "timestamp": 2}
        ]
    });
    compare_conversion(
        &mut oracle,
        6,
        &openai_model,
        &native_context,
        &["openai", "openai-codex", "opencode"],
        Some(&json!({"includeSystemPrompt": false})),
    );

    let image_context = json!({
        "messages": [
            {"role": "user", "content": [{"type": "text", "text": "look"}, {"type": "image", "data": "AAAA", "mimeType": "image/png"}], "timestamp": 1},
            assistant(json!([{"type": "toolCall", "id": "call_img|fc_img", "name": "shot", "arguments": {}}]),
                "openai-responses", "openai", "gpt-5.5"),
            {"role": "toolResult", "toolCallId": "call_img|fc_img", "toolName": "shot",
             "content": [{"type": "text", "text": "text"}, {"type": "image", "data": "BBBB", "mimeType": "image/png"}],
             "isError": false, "timestamp": 2}
        ]
    });
    compare_conversion(&mut oracle, 7, &openai_model, &image_context, &["openai"], None);

    let text_only_model = json!({
        "id": "text-only", "name": "text-only", "api": "openai-responses", "provider": "openai",
        "baseUrl": "https://api.openai.com/v1", "reasoning": false, "input": ["text"],
        "cost": {"input": 0.0, "output": 0.0, "cacheRead": 0.0, "cacheWrite": 0.0},
        "contextWindow": 128000, "maxTokens": 4096
    });
    compare_conversion(&mut oracle, 8, &text_only_model, &image_context, &["openai"], None);

    let _grammar_tool = json!({
        "name": "query", "description": "Query",
        "parameters": {"type": "object", "properties": {"input": {"type": "string"}}, "required": ["input"]},
        "constrainedSampling": {"type": "grammar", "variants": {"openai_lark": "start: /[a-z]+/"}}
    });
    let grammar_context = json!({
        "messages": [
            {"role": "user", "content": "run", "timestamp": 1},
            assistant(json!([{"type": "toolCall", "id": "call_g|ctc_g", "name": "query", "arguments": {"input": "hello"}, "namespace": "ns"}]),
                "openai-responses", "openai", "gpt-5.5"),
            {"role": "toolResult", "toolCallId": "call_g|ctc_g", "toolName": "query",
             "content": [{"type": "text", "text": "done"}], "isError": false, "timestamp": 2}
        ]
    });
    compare_conversion(
        &mut oracle,
        9,
        &openai_model,
        &grammar_context,
        &["openai"],
        Some(&json!({"grammarToolInputProperties": {"query": "input"}})),
    );

    let deferred_tool = json!({
        "name": "later", "description": "Later",
        "parameters": {"type": "object", "properties": {"value": {"type": "string"}}, "required": ["value"]}
    });
    let deferred_context = json!({
        "messages": [
            {"role": "user", "content": "run", "timestamp": 1},
            assistant(json!([{"type": "toolCall", "id": "call_d|fc_d", "name": "first", "arguments": {}}]),
                "openai-responses", "openai", "gpt-5.5"),
            {"role": "toolResult", "toolCallId": "call_d|fc_d", "toolName": "first",
             "content": [{"type": "text", "text": "ok"}], "isError": false, "timestamp": 2,
             "addedToolNames": ["later"]}
        ]
    });
    for (id, mode) in [(10, "additional-tools"), (11, "tool-search")] {
        compare_conversion(
            &mut oracle,
            id,
            &openai_model,
            &deferred_context,
            &["openai"],
            Some(
                &json!({"deferredTools": {"later": deferred_tool}, "deferredToolsMode": mode,
                "toolOptions": {"supportsStrictMode": true}}),
            ),
        );
    }

    let _ = anthropic_model;
}

#[test]
fn differential_openai_responses_shared_tools() {
    let mut oracle = TsOracle::spawn(
        "openai-responses-shared-oracle.ts",
        "packages/ai/src/api/openai-responses-shared.ts",
    );
    let plain = json!({
        "name": "ordinary", "description": "An ordinary tool",
        "parameters": {"type": "object", "properties": {"path": {"type": "string"}, "offset": {"type": "number"}}}
    });
    let constrained = json!({
        "name": "constrained", "description": "A constrained tool",
        "parameters": {"type": "object", "properties": {"value": {"type": "string"}}, "required": ["value"]},
        "constrainedSampling": {"type": "json_schema", "strict": "prefer"}
    });
    let grammar = json!({
        "name": "query", "description": "Query",
        "parameters": {"type": "object", "properties": {"input": {"type": "string"}}, "required": ["input"]},
        "constrainedSampling": {"type": "grammar", "variants": {"openai_lark": "start: /[a-z]+/"}}
    });
    let tools = json!([plain, constrained, grammar]);

    for (id, options) in [
        (0, json!({})),
        (1, json!({"supportsStrictMode": true})),
        (
            2,
            json!({"supportsStrictMode": true, "supportsOpenAIGrammarTools": true}),
        ),
        (
            3,
            json!({"supportsStrictMode": false, "supportsOpenAIGrammarTools": true}),
        ),
        (4, json!({"strict": true, "supportsStrictMode": true})),
        (5, json!({"strict": null, "supportsStrictMode": true})),
        (
            6,
            json!({"deferLoading": true, "supportsStrictMode": true, "supportsOpenAIGrammarTools": true}),
        ),
    ] {
        compare_tools(&mut oracle, id, &tools, &options);
    }
}

#[test]
fn differential_openai_responses_shared_stream() {
    let mut oracle = TsOracle::spawn(
        "openai-responses-shared-oracle.ts",
        "packages/ai/src/api/openai-responses-shared.ts",
    );
    let model_value = model("openai", "openai-responses", "gpt-5.5", true);
    let initial_output = || {
        json!({
            "role": "assistant", "content": [], "api": "openai-responses", "provider": "openai",
            "model": "gpt-5.5", "usage": usage(), "stopReason": "stop", "timestamp": 1234
        })
    };

    let text_events = json!([
        {"type": "response.created", "response": {"id": "resp_1"}},
        {"type": "response.output_item.added", "output_index": 0, "item": {"type": "message", "id": "msg_1", "phase": "final_answer"}},
        {"type": "response.output_text.delta", "output_index": 0, "delta": "Hel"},
        {"type": "response.output_text.delta", "output_index": 0, "delta": "lo"},
        {"type": "response.output_item.done", "output_index": 0,
         "item": {"type": "message", "id": "msg_1", "phase": "final_answer", "content": [{"type": "output_text", "text": "Hello"}]}},
        {"type": "response.completed", "response": {"id": "resp_1", "status": "completed",
         "usage": {"input_tokens": 10, "output_tokens": 5, "total_tokens": 15,
            "input_tokens_details": {"cached_tokens": 2, "cache_write_tokens": 1},
            "output_tokens_details": {"reasoning_tokens": 3}}}}
    ]);
    compare_stream(&mut oracle, 0, &model_value, &text_events, &initial_output(), None);

    let reasoning_events = json!([
        {"type": "response.output_item.added", "output_index": 0, "item": {"type": "reasoning", "id": "rs_1"}},
        {"type": "response.reasoning_summary_text.delta", "output_index": 0, "delta": "think"},
        {"type": "response.reasoning_summary_part.done", "output_index": 0},
        {"type": "response.reasoning_text.delta", "output_index": 0, "delta": "more"},
        {"type": "response.output_item.done", "output_index": 0,
         "item": {"type": "reasoning", "id": "rs_1", "summary": [{"text": "Summary"}],
                  "content": [{"text": "Content"}], "encrypted_content": "enc"}},
        {"type": "response.completed", "response": {"id": "resp_2", "status": "completed"}}
    ]);
    compare_stream(&mut oracle, 1, &model_value, &reasoning_events, &initial_output(), None);

    let tool_events = json!([
        {"type": "response.output_item.added", "output_index": 0,
         "item": {"type": "function_call", "id": "fc_1", "call_id": "call_1", "name": "edit", "arguments": ""}},
        {"type": "response.function_call_arguments.delta", "output_index": 0, "delta": "{\"path\""},
        {"type": "response.function_call_arguments.delta", "output_index": 0, "delta": ":\"README.md\"}"},
        {"type": "response.function_call_arguments.done", "output_index": 0, "arguments": "{\"path\":\"README.md\"}"},
        {"type": "response.output_item.done", "output_index": 0,
         "item": {"type": "function_call", "id": "fc_1", "call_id": "call_1", "name": "edit",
                  "arguments": "{\"path\":\"README.md\"}", "namespace": "ns"}},
        {"type": "response.completed", "response": {"id": "resp_3", "status": "completed"}}
    ]);
    compare_stream(&mut oracle, 2, &model_value, &tool_events, &initial_output(), None);

    let custom_events = json!([
        {"type": "response.output_item.added", "output_index": 0,
         "item": {"type": "custom_tool_call", "id": "ctc_1", "call_id": "call_1", "name": "query", "input": ""}},
        {"type": "response.custom_tool_call_input.delta", "output_index": 0, "delta": "he"},
        {"type": "response.custom_tool_call_input.delta", "output_index": 0, "delta": "llo"},
        {"type": "response.custom_tool_call_input.done", "output_index": 0, "input": "hello"},
        {"type": "response.output_item.done", "output_index": 0,
         "item": {"type": "custom_tool_call", "id": "ctc_1", "call_id": "call_1", "name": "query",
                  "input": "hello", "namespace": "dynamic_tools"}},
        {"type": "response.completed", "response": {"id": "resp_4", "status": "completed"}}
    ]);
    compare_stream(
        &mut oracle,
        3,
        &model_value,
        &custom_events,
        &initial_output(),
        Some(&json!({"grammarToolInputProperties": {"query": "input"}})),
    );

    let citation_text = "Alpha \u{e200}cite\u{e202}turn1a1\u{e201}beta";
    let citation_events = json!([
        {"type": "response.output_item.added", "output_index": 0, "item": {"type": "web_search_call"}},
        {"type": "response.output_item.added", "output_index": 1, "item": {"type": "message", "id": "msg_2"}},
        {"type": "response.output_text.delta", "output_index": 1, "delta": "Alpha "},
        {"type": "response.output_text.delta", "output_index": 1, "delta": "\u{e200}cite\u{e202}turn1a1\u{e201}beta"},
        {"type": "response.output_item.done", "output_index": 1,
         "item": {"type": "message", "id": "msg_2", "content": [{"type": "output_text", "text": citation_text}]}},
        {"type": "response.completed", "response": {"id": "resp_5", "status": "completed"}}
    ]);
    compare_stream(&mut oracle, 4, &model_value, &citation_events, &initial_output(), None);

    let image_events = json!([
        {"type": "response.image_generation_call.partial_image", "partial_image_b64": "partial"},
        {"type": "response.output_item.done", "output_index": 0,
         "item": {"type": "image_generation_call", "id": "img_1", "status": "completed", "result": "final"}},
        {"type": "response.completed", "response": {"id": "resp_6", "status": "completed"}}
    ]);
    compare_stream(&mut oracle, 5, &model_value, &image_events, &initial_output(), None);

    let incomplete_events = json!([
        {"type": "response.output_item.added", "output_index": 0, "item": {"type": "message", "id": "msg_3"}},
        {"type": "response.output_text.delta", "output_index": 0, "delta": "partial"},
        {"type": "response.output_item.done", "output_index": 0,
         "item": {"type": "message", "id": "msg_3", "content": [{"type": "output_text", "text": "partial"}]}},
        {"type": "response.incomplete", "response": {"id": "resp_7", "status": "incomplete",
            "incomplete_details": {"reason": "max_output_tokens"}}}
    ]);
    compare_stream(
        &mut oracle,
        6,
        &model_value,
        &incomplete_events,
        &initial_output(),
        None,
    );

    let failed_events = json!([
        {"type": "response.failed", "response": {"status": "failed", "error": {"code": "server_error", "message": "boom"}}}
    ]);
    compare_stream(&mut oracle, 7, &model_value, &failed_events, &initial_output(), None);

    let no_terminal_events = json!([
        {"type": "response.created", "response": {"id": "resp_8"}}
    ]);
    compare_stream(
        &mut oracle,
        8,
        &model_value,
        &no_terminal_events,
        &initial_output(),
        None,
    );

    let unknown_status_events = json!([
        {"type": "response.completed", "response": {"id": "resp_9", "status": "weird"}}
    ]);
    compare_stream(
        &mut oracle,
        9,
        &model_value,
        &unknown_status_events,
        &initial_output(),
        None,
    );

    let empty_incomplete_events = json!([
        {"type": "response.incomplete", "response": {"status": "incomplete",
            "incomplete_details": {"reason": "max_output_tokens"}}}
    ]);
    compare_stream(
        &mut oracle,
        10,
        &model_value,
        &empty_incomplete_events,
        &initial_output(),
        None,
    );

    let mut output_with_thinking = initial_output();
    output_with_thinking["content"] =
        json!([{"type": "thinking", "thinking": "old", "thinkingSignature": "{\"id\":\"rs_1\"}"}]);
    let backfill_events = json!([
        {"type": "response.output_item.done", "output_index": 0,
         "item": {"type": "reasoning", "id": "rs_1", "summary": [{"text": "new"}]}},
        {"type": "response.completed", "response": {"id": "resp_10", "status": "completed",
            "output": [{"type": "reasoning", "id": "rs_1", "encrypted_content": "backfilled"}]}}
    ]);
    compare_stream(
        &mut oracle,
        11,
        &model_value,
        &backfill_events,
        &output_with_thinking,
        None,
    );
}

#[test]
fn empty_system_prompt_matches_frozen_omission() {
    let mut source = TsOracle::spawn(
        "openai-responses-shared-oracle.ts",
        "packages/ai/src/api/openai-responses-shared.ts",
    );
    compare_conversion(
        &mut source,
        200,
        &model("openai", "openai-responses", "test", true),
        &json!({"systemPrompt":"","messages":[]}),
        &["openai"],
        None,
    );
}

#[test]
fn replay_unicode_ids_and_pipe_segments_match_frozen() {
    let mut source = TsOracle::spawn(
        "openai-responses-shared-oracle.ts",
        "packages/ai/src/api/openai-responses-shared.ts",
    );
    let model = model("openai", "openai-responses", "test", true);
    let signed = assistant(
        json!([{"type":"text","text":"replay","textSignature":"é".repeat(40)}]),
        "openai-responses",
        "openai",
        "test",
    );
    compare_conversion(
        &mut source,
        201,
        &model,
        &json!({"messages":[signed]}),
        &["openai"],
        None,
    );
    let id = "call🙈end|item|tail";
    let call = assistant(
        json!([{"type":"toolCall","id":id,"name":"read","arguments":{}}]),
        "openai-responses",
        "openai",
        "test",
    );
    let result = json!({"role":"toolResult","toolCallId":id,"toolName":"read","content":[{"type":"text","text":"done"}],"isError":false,"timestamp":1234});
    compare_conversion(
        &mut source,
        202,
        &model,
        &json!({"messages":[call,result]}),
        &["openai"],
        None,
    );
}

#[test]
fn corrupt_reasoning_signature_returns_the_frozen_parse_error() {
    let mut source = TsOracle::spawn(
        "openai-responses-shared-oracle.ts",
        "packages/ai/src/api/openai-responses-shared.ts",
    );
    let model = model("openai", "openai-responses", "test", true);
    let message = assistant(
        json!([{"type":"thinking","thinking":"replay","thinkingSignature":"{not json}"}]),
        "openai-responses",
        "openai",
        "test",
    );
    let context = json!({"messages":[message]});
    let expected = source
        .request(&json!({"id":203,"operation":"convertMessages","model":model,"context":context,"allowed":["openai"]}));
    assert_eq!(expected["ok"], json!(false));
    let result = convert_responses_messages(
        &serde_json::from_value(model).unwrap(),
        &serde_json::from_value(context).unwrap(),
        &BTreeSet::from(["openai".into()]),
        None,
    );
    let error = result.expect_err("corrupt replay signatures must not silently disappear");
    assert_eq!(json!(error), expected["error"]);
}

fn stream_boundary_output() -> Value {
    json!({"role":"assistant","content":[],"api":"openai-responses","provider":"openai","model":"test","usage":usage(),"stopReason":"stop","timestamp":1234})
}

#[test]
fn citations_are_normalized_without_a_web_search_item() {
    let mut source = TsOracle::spawn(
        "openai-responses-shared-oracle.ts",
        "packages/ai/src/api/openai-responses-shared.ts",
    );
    let text = "Alpha citeturn0search0beta";
    let events = json!([
        {"type":"response.output_item.added","output_index":0,"item":{"type":"message","id":"msg_cite"}},
        {"type":"response.output_text.delta","output_index":0,"delta":"Alpha ci"},
        {"type":"response.output_text.delta","output_index":0,"delta":"teturn0search0beta"},
        {"type":"response.output_item.done","output_index":0,"item":{"type":"message","id":"msg_cite","content":[{"type":"output_text","text":text}]}},
        {"type":"response.completed","response":{"status":"completed"}}
    ]);
    compare_stream(
        &mut source,
        204,
        &model("openai", "openai-responses", "test", false),
        &events,
        &stream_boundary_output(),
        None,
    );
}

#[test]
fn terminal_without_usage_still_reprices_existing_usage() {
    let mut source = TsOracle::spawn(
        "openai-responses-shared-oracle.ts",
        "packages/ai/src/api/openai-responses-shared.ts",
    );
    let mut output = stream_boundary_output();
    output["usage"]["input"] = json!(10);
    output["usage"]["output"] = json!(4);
    output["usage"]["totalTokens"] = json!(14);
    compare_stream(
        &mut source,
        205,
        &model("openai", "openai-responses", "test", false),
        &json!([{"type":"response.completed","response":{"status":"completed"}}]),
        &output,
        None,
    );
}

#[test]
fn failed_response_with_empty_details_uses_frozen_error_defaults() {
    let mut source = TsOracle::spawn(
        "openai-responses-shared-oracle.ts",
        "packages/ai/src/api/openai-responses-shared.ts",
    );
    compare_stream(
        &mut source,
        206,
        &model("openai", "openai-responses", "test", false),
        &json!([{"type":"response.failed","response":{"error":{}}}]),
        &stream_boundary_output(),
        None,
    );
}

#[test]
fn message_phase_does_not_override_existing_stop_reason() {
    let mut source = TsOracle::spawn(
        "openai-responses-shared-oracle.ts",
        "packages/ai/src/api/openai-responses-shared.ts",
    );
    let mut output = stream_boundary_output();
    output["stopReason"] = json!("length");
    let events = json!([
        {"type":"response.output_item.added","output_index":0,"item":{"type":"message","id":"msg_phase","phase":"final_answer"}},
        {"type":"response.output_item.done","output_index":0,"item":{"type":"message","id":"msg_phase","phase":"final_answer","content":[{"type":"output_text","text":"answer"}]}},
        {"type":"response.completed","response":{"status":"completed"}}
    ]);
    compare_stream(
        &mut source,
        207,
        &model("openai", "openai-responses", "test", false),
        &events,
        &output,
        None,
    );
}
