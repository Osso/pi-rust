use pi_ai::api::openai_completions::{OpenAICompletionsOptions, build_params, convert_messages, get_compat};
use pi_ai::request::StreamOptions;
use pi_ai::types::{CacheRetention, Context, Model};
use serde_json::{Value, json};

fn model(provider: &str, id: &str, base_url: &str, reasoning: bool) -> Model {
    serde_json::from_value(json!({
        "id": id, "name": id, "api": "openai-completions", "provider": provider,
        "baseUrl": base_url, "reasoning": reasoning,
        "input": ["text", "image"],
        "cost": {"input": 1.0, "output": 2.0, "cacheRead": 0.5, "cacheWrite": 0.75},
        "contextWindow": 128000, "maxTokens": 4096
    }))
    .expect("model")
}

fn context(value: Value) -> Context {
    serde_json::from_value(value).expect("context")
}

fn assistant(content: Value, provider: &str, model_id: &str) -> Value {
    json!({
        "role": "assistant", "content": content, "api": "openai-completions",
        "provider": provider, "model": model_id,
        "usage": {"input": 0.0, "output": 0.0, "cacheRead": 0.0, "cacheWrite": 0.0,
            "totalTokens": 0.0,
            "cost": {"input": 0.0, "output": 0.0, "cacheRead": 0.0, "cacheWrite": 0.0, "total": 0.0}},
        "stopReason": "stop", "timestamp": 2
    })
}

#[test]
fn thinking_as_text_flattens_reasoning_into_content() {
    let model = serde_json::from_value::<Model>(json!({
        "id": "gpt-5", "name": "gpt-5", "api": "openai-completions", "provider": "openai",
        "baseUrl": "https://api.openai.com/v1", "reasoning": true, "input": ["text"],
        "cost": {"input": 0.0, "output": 0.0, "cacheRead": 0.0, "cacheWrite": 0.0},
        "contextWindow": 128000, "maxTokens": 4096,
        "compat": {"requiresThinkingAsText": true}
    }))
    .expect("model");
    let context = context(json!({
        "messages": [
            {"role": "user", "content": "hi", "timestamp": 1},
            assistant(json!([
                {"type": "thinking", "thinking": "reason"},
                {"type": "text", "text": "answer"}
            ]), "openai", "gpt-5")
        ]
    }));
    let compat = get_compat(&model);
    let messages = convert_messages(&model, &context, &compat);
    let assistant = &messages[1];
    assert_eq!(
        assistant["content"][0]["type"],
        pi_js_compat::js_json::JsValue::String(("text").into())
    );
    assert_eq!(
        assistant["content"][0]["text"],
        pi_js_compat::js_json::JsValue::String(("reason").into())
    );
    assert_eq!(
        assistant["content"][1]["text"],
        pi_js_compat::js_json::JsValue::String(("answer").into())
    );
}

#[test]
fn reasoning_content_is_added_for_deepseek_assistant_messages() {
    let model = model("deepseek", "deepseek-chat", "https://api.deepseek.com", true);
    let context = context(json!({
        "messages": [
            {"role": "user", "content": "hi", "timestamp": 1},
            assistant(json!([{"type": "text", "text": "answer"}]), "deepseek", "deepseek-chat")
        ]
    }));
    let compat = get_compat(&model);
    let messages = convert_messages(&model, &context, &compat);
    assert_eq!(
        messages[1]["reasoning_content"],
        pi_js_compat::js_json::JsValue::String(("").into())
    );
}

#[test]
fn tool_call_ids_are_normalized_for_pipe_ids() {
    let model = model("openai", "gpt-5", "https://api.openai.com/v1", true);
    let context = context(json!({
        "messages": [
            {"role": "user", "content": "hi", "timestamp": 1},
            assistant(json!([{"type": "toolCall", "id": "call_1|item_1", "name": "tool", "arguments": {}}]),
                "openai", "gpt-5"),
            {"role": "toolResult", "toolCallId": "call_1|item_1", "toolName": "tool",
             "content": [{"type": "text", "text": "ok"}], "isError": false, "timestamp": 3}
        ]
    }));
    let compat = get_compat(&model);
    let messages = convert_messages(&model, &context, &compat);
    assert_eq!(
        messages[1]["tool_calls"][0]["id"],
        pi_js_compat::js_json::JsValue::String(("call_1_item_1").into())
    );
    assert_eq!(
        messages[2]["tool_call_id"],
        pi_js_compat::js_json::JsValue::String(("call_1_item_1").into())
    );
}

#[test]
fn empty_tools_are_sent_when_the_transcript_has_tool_history() {
    let model = model("openai", "gpt-5", "https://api.openai.com/v1", true);
    let context = context(json!({
        "messages": [
            {"role": "user", "content": "hi", "timestamp": 1},
            assistant(json!([{"type": "toolCall", "id": "call_1", "name": "tool", "arguments": {}}]),
                "openai", "gpt-5"),
            {"role": "toolResult", "toolCallId": "call_1", "toolName": "tool",
             "content": [{"type": "text", "text": "ok"}], "isError": false, "timestamp": 3}
        ]
    }));
    let compat = get_compat(&model);
    let params = build_params(
        &model,
        &context,
        &StreamOptions::default(),
        &OpenAICompletionsOptions::default(),
        &compat,
        CacheRetention::Short,
    );
    assert_eq!(params["tools"], pi_js_compat::js_json::JsValue::from(json!([])));
}

#[test]
fn anthropic_cache_control_is_applied_to_system_tools_and_last_message() {
    let model = serde_json::from_value::<Model>(json!({
        "id": "anthropic/claude", "name": "claude", "api": "openai-completions",
        "provider": "openrouter", "baseUrl": "https://openrouter.ai/api/v1",
        "reasoning": false, "input": ["text"],
        "cost": {"input": 0.0, "output": 0.0, "cacheRead": 0.0, "cacheWrite": 0.0},
        "contextWindow": 128000, "maxTokens": 4096,
        "compat": {"cacheControlFormat": "anthropic", "supportsLongCacheRetention": true}
    }))
    .expect("model");
    let context = context(json!({
        "systemPrompt": "system",
        "messages": [{"role": "user", "content": "hi", "timestamp": 1}],
        "tools": [{"name": "tool", "description": "Tool",
            "parameters": {"type": "object", "properties": {"a": {"type": "string"}}}}]
    }));
    let compat = get_compat(&model);
    let params = build_params(
        &model,
        &context,
        &StreamOptions {
            session_id: Some(("session".to_owned()).into()),
            ..Default::default()
        },
        &OpenAICompletionsOptions::default(),
        &compat,
        CacheRetention::Long,
    );
    assert_eq!(
        params["messages"][0]["content"][0]["cache_control"]["type"],
        pi_js_compat::js_json::JsValue::String(("ephemeral").into())
    );
    assert_eq!(
        params["messages"][0]["content"][0]["cache_control"]["ttl"],
        pi_js_compat::js_json::JsValue::String(("1h").into())
    );
    assert_eq!(
        params["tools"][0]["cache_control"]["ttl"],
        pi_js_compat::js_json::JsValue::String(("1h").into())
    );
    assert_eq!(
        params["messages"][1]["content"][0]["cache_control"]["type"],
        pi_js_compat::js_json::JsValue::String(("ephemeral").into())
    );
    assert_eq!(
        params["prompt_cache_key"],
        pi_js_compat::js_json::JsValue::String(("session").into())
    );
}

#[test]
fn max_tokens_field_and_prompt_cache_follow_compat() {
    let model = model("deepseek", "deepseek-chat", "https://api.deepseek.com", true);
    let compat = get_compat(&model);
    let params = build_params(
        &model,
        &context(json!({"messages": [{"role": "user", "content": "hi", "timestamp": 1}]})),
        &StreamOptions {
            max_tokens: Some(123.0),
            session_id: Some(("session".to_owned()).into()),
            ..Default::default()
        },
        &OpenAICompletionsOptions::default(),
        &compat,
        CacheRetention::Short,
    );
    // Frozen1214-1215 excludes DeepSeek from useMaxTokens; explicit provider
    // factory overrides are separate from a bare model's detected defaults.
    assert_eq!(params["max_completion_tokens"].as_f64(), Some(123.0));
    assert_eq!(params.get("max_tokens"), None);
    assert_eq!(params.get("prompt_cache_key"), None);
    assert_eq!(params.get("store"), None);
}

#[test]
fn old_thinking_budget_options_do_not_add_frozen_request_fields() {
    let model = serde_json::from_value::<Model>(json!({
        "id": "custom", "name": "custom", "api": "openai-completions", "provider": "custom",
        "baseUrl": "https://example.com/v1", "reasoning": true, "input": ["text"],
        "cost": {"input": 0.0, "output": 0.0, "cacheRead": 0.0, "cacheWrite": 0.0},
        "contextWindow": 128000, "maxTokens": 4096,
        "compat": {"supportsThinkingTokenBudget": true, "supportsReasoningEffort": true}
    }))
    .expect("model");
    let compat = get_compat(&model);
    let params = build_params(
        &model,
        &context(json!({"messages": [{"role": "user", "content": "hi", "timestamp": 1}]})),
        &StreamOptions {
            max_tokens: Some(2000.0),
            ..Default::default()
        },
        &OpenAICompletionsOptions {
            reasoning_effort: Some("high".to_owned()),
            thinking_budgets: Some(pi_ai::types::ThinkingBudgets {
                high: Some(5000.0),
                ..Default::default()
            }),
            ..Default::default()
        },
        &compat,
        CacheRetention::Short,
    );
    assert!(params.get("thinking_token_budget").is_none());
    assert_eq!(params["max_completion_tokens"].as_f64(), Some(2000.0));
}
