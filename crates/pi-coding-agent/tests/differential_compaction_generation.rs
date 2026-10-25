//! Frozen injected-stream request/result contracts; no live provider access.
#[path = "support/oracle.rs"]
mod oracle;

use pi_agent_core::agent::{AgentMessage, StreamFn};
use pi_ai::types::{AssistantMessage, Model, ProviderEnv, ProviderHeaders, StopReason};
use pi_coding_agent::core::compaction::branch_summarization::{GenerateBranchSummaryOptions, generate_branch_summary};
use pi_coding_agent::core::compaction::{
    CompactionPreparation, CompactionSummaryOptions, DEFAULT_COMPACTION_SETTINGS, GenerateSummaryOptions, compact,
    create_file_ops, generate_summary,
};
use pi_coding_agent::core::session_manager::SessionEntry;
use pi_js_compat::js_json::JsString;
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

fn model() -> Model {
    serde_json::from_value(json!({
        "id": "fixture", "name": "Fixture", "api": "faux", "provider": "faux",
        "baseUrl": "https://example.invalid", "reasoning": false, "input": ["text"],
        "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0},
        "contextWindow": 100000, "maxTokens": 8192
    }))
    .expect("fixture model")
}
fn user() -> AgentMessage {
    serde_json::from_value(json!({"role": "user", "content": "Inspect src/lib.rs", "timestamp": 0})).unwrap()
}
fn entry() -> SessionEntry {
    serde_json::from_value(json!({"type": "message", "id": "history", "parentId": null,
        "timestamp": "2026-01-01T00:00:00.000Z", "message": user()}))
    .unwrap()
}
fn response(text: &str) -> AssistantMessage {
    let mut response = AssistantMessage::empty("faux", "faux", "fixture", 0.0);
    response.content = vec![pi_ai::types::AssistantContent::Text(pi_ai::types::TextContent::new(
        text,
    ))];
    response.usage.input = 10.0;
    response.usage.output = 3.0;
    response.usage.total_tokens = 13.0;
    response.stop_reason = StopReason::Stop;
    response
}
fn headers() -> ProviderHeaders {
    [("x-fixture".to_owned(), Some("yes".into()))].into()
}
fn env() -> ProviderEnv {
    [("FIXTURE_ENV".to_owned(), "value".into())].into()
}
fn stream(responses: Vec<AssistantMessage>, requests: Arc<Mutex<Vec<Value>>>) -> StreamFn {
    let responses = Arc::new(Mutex::new(VecDeque::from(responses)));
    Arc::new(move |model, context, options| {
        let mut context = serde_json::to_value(context).unwrap();
        for message in context["messages"].as_array_mut().expect("request messages") {
            message["timestamp"] = json!(0);
        }
        requests.lock().unwrap().push(json!({"model": model, "context": context, "options": {
            "apiKey": options.stream.request.api_key, "headers": options.stream.request.headers,
            "env": options.stream.request.env, "maxTokens": options.stream.max_tokens.map(pi_js_compat::number::js_number_value),
            "cacheRetention": options.stream.cache_retention,
        }}));
        let message = responses.lock().unwrap().pop_front().expect("scripted response");
        Box::pin(async move {
            let (sender, stream) = pi_ai::utils::event_stream::create_assistant_message_event_stream();
            sender.push(pi_ai::types::AssistantMessageEvent::Done {
                reason: pi_ai::types::DoneReason::Stop,
                message,
            });
            Ok(stream)
        })
    })
}
fn outcome<T: Serialize, E: std::fmt::Display>(result: Result<T, E>) -> Value {
    match result {
        Ok(value) => json!({"ok": true, "value": value}),
        Err(error) => json!({"ok": false, "error": {"name": "Error", "message": error.to_string()}}),
    }
}
fn oracle() -> oracle::TsOracle {
    oracle::TsOracle::spawn("coding-agent-compaction-oracle.ts", "packages/coding-agent/src")
}
fn compare(oracle: &mut oracle::TsOracle, request: Value, result: Value, requests: &Arc<Mutex<Vec<Value>>>) {
    let expected = oracle.request(&request);
    assert_eq!(expected["ok"], true, "oracle: {expected}");
    let actual = json!({"result": result, "requests": *requests.lock().unwrap()});
    assert_eq!(actual, expected["output"], "request: {request}");
}

#[tokio::test]
async fn branch_generation_matches_frozen_result_and_instruction_truthiness() {
    let mut oracle = oracle();
    let model = model();
    for (instructions, replace) in [
        (None, false),
        (Some(""), true),
        (Some(""), false),
        (Some("focus"), true),
        (Some("focus"), false),
    ] {
        let entries = vec![entry()];
        let response = response("summary");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stream = stream(vec![response.clone()], requests.clone());
        let instructions = instructions.map(JsString::from);
        let result = generate_branch_summary(
            &entries,
            &GenerateBranchSummaryOptions {
                model: &model,
                api_key: Some("fixture-key".into()),
                headers: Some(headers()),
                env: Some(env()),
                signal: None,
                custom_instructions: instructions.as_ref(),
                replace_instructions: replace,
                reserve_tokens: Some(1000.0),
                stream_fn: Some(&stream),
            },
        )
        .await;
        compare(
            &mut oracle,
            json!({"id": 1, "module": "core/compaction/branch-summarization.ts", "operation": "generation", "kind": "branch",
            "entries": entries, "model": model, "responses": [response], "customInstructions": instructions,
            "replaceInstructions": replace}),
            outcome(result),
            &requests,
        );
    }
}

#[tokio::test]
async fn summary_generation_matches_frozen_guidance_focus_and_empty_errors() {
    let mut oracle = oracle();
    let model = model();
    for (custom, previous, text) in [
        (None, None, "summary"),
        (Some(" \u{feff} focus \n"), Some("prior"), "summary"),
        (Some(" \u{feff} "), Some(""), "summary"),
        (None, None, " \u{feff} "),
    ] {
        let messages = vec![user()];
        let response = response(text);
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stream = stream(vec![response.clone()], requests.clone());
        let custom = custom.map(JsString::from);
        let previous = previous.map(JsString::from);
        let result = generate_summary(&GenerateSummaryOptions {
            current_messages: &messages,
            model: &model,
            reserve_tokens: 1000.0,
            api_key: Some("fixture-key".into()),
            headers: Some(headers()),
            env: Some(env()),
            signal: None,
            custom_instructions: custom.as_ref(),
            previous_summary: previous.as_ref(),
            thinking_level: None,
            stream_fn: Some(&stream),
        })
        .await;
        compare(
            &mut oracle,
            json!({"id": 1, "module": "core/compaction/compaction.ts", "operation": "generation", "kind": "summary",
            "messages": messages, "model": model, "responses": [response], "customInstructions": custom,
            "previousSummary": previous}),
            outcome(result),
            &requests,
        );
    }
}

#[tokio::test]
async fn split_compaction_matches_frozen_request_and_result_shapes() {
    let mut oracle = oracle();
    let model = model();
    for text in ["summary", " \u{feff} "] {
        let preparation = CompactionPreparation {
            first_kept_entry_id: "kept".into(),
            messages_to_summarize: vec![user()],
            turn_prefix_messages: vec![user()],
            is_split_turn: true,
            tokens_before: 10000.0,
            previous_summary: None,
            file_ops: create_file_ops(),
            settings: pi_coding_agent::core::compaction::CompactionSettings {
                reserve_tokens: 1000.0,
                ..DEFAULT_COMPACTION_SETTINGS
            },
        };
        let responses = vec![response("history summary"), response(text)];
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stream = stream(responses.clone(), requests.clone());
        let instructions = JsString::from("  preserve pending questions  ");
        let result = compact(
            &preparation,
            &model,
            &CompactionSummaryOptions {
                model: &model,
                api_key: Some("fixture-key".into()),
                headers: Some(headers()),
                env: Some(env()),
                custom_instructions: Some(&instructions),
                signal: None,
                thinking_level: None,
                stream_fn: Some(&stream),
            },
        )
        .await
        .map(|mut result| {
            result.duration_ms = Some(0.0);
            result
        });
        compare(
            &mut oracle,
            json!({"id": 1, "module": "core/compaction/compaction.ts", "operation": "generation", "kind": "compact",
            "preparation": preparation, "model": model, "responses": responses, "customInstructions": instructions}),
            outcome(result),
            &requests,
        );
    }
}
