//! Ported `packages/ai/test/google-*.test.ts` cases for the shared Google helpers.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use pi_ai::api::constrained_sampling::ConstrainedSamplingError;
use pi_ai::api::google_shared::{
    FunctionCallingConfigMode, GoogleRetryOptions, GoogleThinkingLevel, convert_messages, convert_tools,
    is_thinking_part, map_stop_reason, map_stop_reason_string, requires_tool_call_id,
    resolve_google_function_calling_mode, retain_thought_signature, retry_google_request,
    supports_google_strict_tool_sampling,
};
use pi_ai::types::{
    AssistantContent, AssistantMessage, AssistantRole, Context, Message, Model, StopReason, TextContent,
    ThinkingContent, Tool, ToolCall, ToolResultMessage, UserContent, UserMessage, UserRole,
};
use serde_json::{Value, json};

const VALID_SIG: &str = "AAAAAAAAAAAAAAAAAAAAAA==";

fn model(api: &str, provider: &str, id: &str, images: bool) -> Model {
    serde_json::from_value(json!({
        "id": id,
        "name": id,
        "api": api,
        "provider": provider,
        "baseUrl": "https://example.com",
        "reasoning": true,
        "input": if images { json!(["text", "image"]) } else { json!(["text"]) },
        "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0},
        "contextWindow": 128000,
        "maxTokens": 8192
    }))
    .expect("model must deserialize")
}

fn assistant(content: Vec<AssistantContent>, provider: &str, api: &str, id: &str) -> AssistantMessage {
    AssistantMessage {
        role: AssistantRole::Tag,
        content,
        api: api.to_owned(),
        provider: provider.to_owned(),
        model: id.to_owned(),
        response_model: None,
        response_id: None,
        image_generation_result: None,
        diagnostics: None,
        usage: Default::default(),
        stop_reason: StopReason::ToolUse,
        error_message: None,
        raw_stop_reason: None,
        timestamp: 1.0,
    }
}

fn user_text(text: &str) -> Message {
    Message::User(UserMessage {
        role: UserRole::Tag,
        content: UserContent::Text(text.into()),
        input_source: None,
        provider_native: None,
        timestamp: 1.0,
    })
}

fn context(messages: Vec<Message>) -> Context {
    Context {
        system_prompt: None,
        messages,
        tools: None,
    }
}

fn text_block(text: &str, signature: Option<&str>) -> AssistantContent {
    AssistantContent::Text(TextContent {
        r#type: Default::default(),
        text: text.into(),
        text_signature: signature.map(str::to_owned),
        index: None,
    })
}

fn thinking_block(thinking: &str, signature: Option<&str>) -> AssistantContent {
    AssistantContent::Thinking(ThinkingContent {
        r#type: Default::default(),
        thinking: thinking.into(),
        thinking_signature: signature.map(str::to_owned),
        redacted: None,
        index: None,
    })
}

fn tool_call(id: &str, name: &str, signature: Option<&str>) -> AssistantContent {
    let mut call = ToolCall::new(
        id,
        name,
        [("command".into(), pi_js_compat::js_json::JsValue::String("ls".into()))]
            .into_iter()
            .collect(),
    );
    call.thought_signature = signature.map(str::to_owned);
    AssistantContent::ToolCall(call)
}

fn tool_result(id: &str, name: &str, content: pi_ai::types::ToolResultContent) -> Message {
    Message::ToolResult(ToolResultMessage {
        role: Default::default(),
        tool_call_id: id.to_owned(),
        tool_name: name.to_owned(),
        content: vec![content],
        details: None,
        usage: None,
        added_tool_names: None,
        is_error: false,
        timestamp: 1.0,
    })
}

fn function_calls(contents: &[Value]) -> Vec<Value> {
    contents
        .iter()
        .flat_map(|content| content["parts"].as_array().cloned().unwrap_or_default())
        .filter(|part| part.get("functionCall").is_some())
        .collect()
}

fn function_responses(contents: &[Value]) -> Vec<Value> {
    contents
        .iter()
        .flat_map(|content| content["parts"].as_array().cloned().unwrap_or_default())
        .filter(|part| part.get("functionResponse").is_some())
        .collect()
}

fn contents(model: &Model, context: &Context) -> Vec<Value> {
    convert_messages(model, context, 1_700_000_000_000.0)
        .into_iter()
        .map(|content| serde_json::to_value(content).expect("content serializes"))
        .collect()
}

#[test]
fn thinking_detection_requires_the_thought_flag() {
    assert!(is_thinking_part(Some(true)));
    assert!(!is_thinking_part(None));
    assert!(!is_thinking_part(Some(false)));
}

#[test]
fn thought_signatures_are_retained_until_a_new_non_empty_one_arrives() {
    assert_eq!(retain_thought_signature(None, Some("sig-1")).as_deref(), Some("sig-1"));
    assert_eq!(retain_thought_signature(Some("sig-1"), None).as_deref(), Some("sig-1"));
    assert_eq!(
        retain_thought_signature(Some("sig-1"), Some("")).as_deref(),
        Some("sig-1")
    );
    assert_eq!(
        retain_thought_signature(Some("sig-1"), Some("sig-2")).as_deref(),
        Some("sig-2")
    );
}

#[test]
fn tool_call_ids_are_required_for_gemini3_claude_and_gpt_oss() {
    assert!(!requires_tool_call_id("gemini-2.5-flash"));
    assert!(requires_tool_call_id("gemini-3.6-flash"));
    assert!(requires_tool_call_id("claude-sonnet-4-5"));
    assert!(requires_tool_call_id("gpt-oss-120b"));
    assert!(requires_tool_call_id("GEMINI-3-PRO-PREVIEW"));
    assert!(requires_tool_call_id("gemini-live-3-flash"));
}

#[test]
fn signed_empty_blocks_are_kept_so_their_signature_is_echoed_back() {
    let model = model("google-generative-ai", "google", "gemini-3-pro-preview", false);
    let contents = contents(
        &model,
        &context(vec![
            user_text("Hi"),
            Message::Assistant(Box::new(assistant(
                vec![thinking_block("", Some(VALID_SIG)), tool_call("call_1", "bash", None)],
                "google",
                "google-generative-ai",
                "gemini-3-pro-preview",
            ))),
        ]),
    );
    let model_turn = contents.iter().find(|c| c["role"] == "model").unwrap();
    let signed: Vec<&Value> = model_turn["parts"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|part| part["thoughtSignature"] == VALID_SIG)
        .collect();
    assert_eq!(signed.len(), 1);
    assert_eq!(signed[0]["thought"], true);
}

#[test]
fn signed_empty_text_blocks_are_kept() {
    let model = model("google-generative-ai", "google", "gemini-3-pro-preview", false);
    let contents = contents(
        &model,
        &context(vec![
            user_text("Hi"),
            Message::Assistant(Box::new(assistant(
                vec![text_block("", Some(VALID_SIG)), tool_call("call_1", "bash", None)],
                "google",
                "google-generative-ai",
                "gemini-3-pro-preview",
            ))),
        ]),
    );
    let model_turn = contents.iter().find(|c| c["role"] == "model").unwrap();
    assert_eq!(
        model_turn["parts"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|part| part["thoughtSignature"] == VALID_SIG)
            .count(),
        1
    );
}

#[test]
fn unsigned_empty_blocks_are_dropped() {
    let model = model("google-generative-ai", "google", "gemini-3-pro-preview", false);
    let contents = contents(
        &model,
        &context(vec![
            user_text("Hi"),
            Message::Assistant(Box::new(assistant(
                vec![
                    thinking_block("", None),
                    text_block("   ", None),
                    tool_call("call_1", "bash", None),
                ],
                "google",
                "google-generative-ai",
                "gemini-3-pro-preview",
            ))),
        ]),
    );
    let model_turn = contents.iter().find(|c| c["role"] == "model").unwrap();
    let parts = model_turn["parts"].as_array().unwrap();
    assert_eq!(parts.len(), 1);
    assert!(parts[0].get("functionCall").is_some());
}

#[test]
fn signed_empty_blocks_from_another_model_are_dropped_with_their_signature() {
    let model = model("google-generative-ai", "google", "gemini-3-pro-preview", false);
    let contents = contents(
        &model,
        &context(vec![
            user_text("Hi"),
            Message::Assistant(Box::new(assistant(
                vec![
                    thinking_block("", Some(VALID_SIG)),
                    text_block("", Some(VALID_SIG)),
                    tool_call("call_1", "bash", None),
                ],
                "google",
                "google-generative-ai",
                "other-model",
            ))),
        ]),
    );
    let model_turn = contents.iter().find(|c| c["role"] == "model").unwrap();
    let parts = model_turn["parts"].as_array().unwrap();
    assert_eq!(parts.len(), 1);
    assert!(parts[0].get("functionCall").is_some());
    assert!(!serde_json::to_string(model_turn).unwrap().contains(VALID_SIG));
}

#[test]
fn gemini3_history_preserves_tool_call_ids_and_never_adds_skip_signatures() {
    for (api, provider) in [("google-generative-ai", "google"), ("google-vertex", "google-vertex")] {
        let model = model(api, provider, "gemini-3-pro-preview", false);
        let ctx = context(vec![
            user_text("Hi"),
            Message::Assistant(Box::new(assistant(
                vec![tool_call("call_1", "bash", None), tool_call("call_2", "bash", None)],
                provider,
                api,
                "gemini-3-pro-preview",
            ))),
            tool_result(
                "call_1",
                "bash",
                pi_ai::types::TextOrImageContent::Text(TextContent::new("hi")),
            ),
            tool_result(
                "call_2",
                "bash",
                pi_ai::types::TextOrImageContent::Text(TextContent::new("files")),
            ),
        ]);
        let converted = contents(&model, &ctx);

        let calls = function_calls(&converted);
        let call_ids: Vec<&str> = calls
            .iter()
            .map(|part| part["functionCall"]["id"].as_str().unwrap())
            .collect();
        assert_eq!(call_ids, ["call_1", "call_2"]);

        let responses = function_responses(&converted);
        let response_ids: Vec<&str> = responses
            .iter()
            .map(|part| part["functionResponse"]["id"].as_str().unwrap())
            .collect();
        assert_eq!(response_ids, ["call_1", "call_2"]);

        let model_turn = converted.iter().find(|c| c["role"] == "model").unwrap();
        assert!(
            !serde_json::to_string(model_turn)
                .unwrap()
                .contains("skip_thought_signature_validator")
        );
        for part in &calls {
            assert!(part.get("thoughtSignature").is_none());
        }
    }
}

#[test]
fn valid_signatures_survive_for_the_same_provider_and_model_only() {
    let model = model("google-generative-ai", "google", "gemini-3-pro-preview", false);
    let converted = contents(
        &model,
        &context(vec![
            user_text("Hi"),
            Message::Assistant(Box::new(assistant(
                vec![
                    tool_call("call_1", "bash", Some(VALID_SIG)),
                    tool_call("call_2", "bash", None),
                ],
                "google",
                "google-generative-ai",
                "gemini-3-pro-preview",
            ))),
        ]),
    );
    let calls = function_calls(&converted);
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0]["thoughtSignature"], VALID_SIG);
    assert!(calls[1].get("thoughtSignature").is_none());
}

#[test]
fn non_gemini3_models_omit_tool_call_ids() {
    let model = model("google-generative-ai", "google", "gemini-2.5-flash", false);
    let converted = contents(
        &model,
        &context(vec![
            user_text("Hi"),
            Message::Assistant(Box::new(assistant(
                vec![tool_call("call_1", "bash", None), tool_call("call_2", "bash", None)],
                "google",
                "google-generative-ai",
                "other-model",
            ))),
            tool_result(
                "call_1",
                "bash",
                pi_ai::types::TextOrImageContent::Text(TextContent::new("hi")),
            ),
            tool_result(
                "call_2",
                "bash",
                pi_ai::types::TextOrImageContent::Text(TextContent::new("files")),
            ),
        ]),
    );
    let calls = function_calls(&converted);
    assert_eq!(calls.len(), 2);
    for part in &calls {
        assert!(part["functionCall"].get("id").is_none());
        assert!(part.get("thoughtSignature").is_none());
    }
    let responses = function_responses(&converted);
    assert_eq!(responses.len(), 2);
    for part in &responses {
        assert!(part["functionResponse"].get("id").is_none());
    }
}

fn image_tool_result_context(model: &Model) -> Context {
    context(vec![
        user_text("read the files"),
        Message::Assistant(Box::new(assistant(
            vec![
                tool_call("call_a", "read", None),
                tool_call("call_img", "read", None),
                tool_call("call_b", "read", None),
            ],
            "google",
            &model.api,
            &model.id,
        ))),
        tool_result(
            "call_a",
            "read",
            pi_ai::types::TextOrImageContent::Text(TextContent::new("alpha text")),
        ),
        tool_result(
            "call_img",
            "read",
            pi_ai::types::TextOrImageContent::Image(pi_ai::types::ImageContent::new("abc", "image/png")),
        ),
        tool_result(
            "call_b",
            "read",
            pi_ai::types::TextOrImageContent::Text(TextContent::new("beta text")),
        ),
    ])
}

#[test]
fn gemini2_image_tool_results_use_a_separate_user_turn() {
    let model = model("google-generative-ai", "google", "gemini-2.5-flash", true);
    let converted = contents(&model, &image_tool_result_context(&model));

    assert_eq!(converted.len(), 5);
    let third_parts = converted[2]["parts"].as_array().unwrap();
    assert!(third_parts.iter().all(|part| part.get("functionResponse").is_some()));
    assert_eq!(converted[3]["parts"][0]["text"], "Tool result image:");
    assert!(converted[3]["parts"][1].get("inlineData").is_some());
    assert!(converted[4]["parts"][0].get("functionResponse").is_some());
}

#[test]
fn gemini3_nests_image_tool_results_inside_the_function_response() {
    let model = model("google-generative-ai", "google", "gemini-3-pro-preview", true);
    let converted = contents(&model, &image_tool_result_context(&model));

    assert_eq!(converted.len(), 3);
    let parts = converted[2]["parts"].as_array().unwrap();
    assert_eq!(parts.len(), 3);
    let image_response = &parts[1]["functionResponse"];
    assert_eq!(image_response["parts"].as_array().unwrap().len(), 1);
    assert!(image_response["parts"][0].get("inlineData").is_some());
}

#[test]
fn image_tool_results_are_dropped_for_text_only_models() {
    let model = model("google-generative-ai", "google", "gemini-2.5-flash", false);
    let converted = contents(&model, &image_tool_result_context(&model));
    assert_eq!(converted.len(), 3);
    for content in &converted {
        assert!(!serde_json::to_string(content).unwrap().contains("inlineData"));
    }
}

#[test]
fn cross_model_thinking_becomes_plain_text() {
    let model = model("google-generative-ai", "google", "gemini-3-pro-preview", false);
    let converted = contents(
        &model,
        &context(vec![
            user_text("Hi"),
            Message::Assistant(Box::new(assistant(
                vec![thinking_block("reasoning", Some(VALID_SIG))],
                "anthropic",
                "anthropic-messages",
                "claude-sonnet-4.5",
            ))),
        ]),
    );
    let model_turn = converted.iter().find(|c| c["role"] == "model").unwrap();
    let parts = model_turn["parts"].as_array().unwrap();
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0]["text"], "reasoning");
    assert!(parts[0].get("thought").is_none());
    assert!(parts[0].get("thoughtSignature").is_none());
}

#[test]
fn same_model_thinking_is_replayed_as_a_thought_part() {
    let model = model("google-generative-ai", "google", "gemini-3-pro-preview", false);
    let converted = contents(
        &model,
        &context(vec![
            user_text("Hi"),
            Message::Assistant(Box::new(assistant(
                vec![thinking_block("reasoning", Some(VALID_SIG))],
                "google",
                "google-generative-ai",
                "gemini-3-pro-preview",
            ))),
        ]),
    );
    let model_turn = converted.iter().find(|c| c["role"] == "model").unwrap();
    let parts = model_turn["parts"].as_array().unwrap();
    assert_eq!(parts[0]["thought"], true);
    assert_eq!(parts[0]["text"], "reasoning");
    assert_eq!(parts[0]["thoughtSignature"], VALID_SIG);
}

#[test]
fn user_image_blocks_become_inline_data() {
    let model = model("google-generative-ai", "google", "gemini-3-pro-preview", true);
    let message = Message::User(UserMessage {
        role: UserRole::Tag,
        content: UserContent::Blocks(vec![
            pi_ai::types::TextOrImageContent::Text(TextContent::new("look")),
            pi_ai::types::TextOrImageContent::Image(pi_ai::types::ImageContent::new("abc", "image/png")),
        ]),
        input_source: None,
        provider_native: None,
        timestamp: 1.0,
    });
    let converted = contents(&model, &context(vec![message]));
    assert_eq!(converted.len(), 1);
    assert_eq!(converted[0]["role"], "user");
    assert_eq!(converted[0]["parts"][0]["text"], "look");
    assert_eq!(converted[0]["parts"][1]["inlineData"]["mimeType"], "image/png");
    assert_eq!(converted[0]["parts"][1]["inlineData"]["data"], "abc");
}

#[test]
fn empty_user_block_lists_are_skipped() {
    let model = model("google-generative-ai", "google", "gemini-3-pro-preview", true);
    let message = Message::User(UserMessage {
        role: UserRole::Tag,
        content: UserContent::Blocks(vec![]),
        input_source: None,
        provider_native: None,
        timestamp: 1.0,
    });
    assert!(contents(&model, &context(vec![message])).is_empty());
}

#[test]
fn consecutive_tool_results_share_one_user_turn() {
    let model = model("google-generative-ai", "google", "gemini-2.5-flash", false);
    let converted = contents(
        &model,
        &context(vec![
            user_text("go"),
            Message::Assistant(Box::new(assistant(
                vec![tool_call("call_1", "bash", None), tool_call("call_2", "bash", None)],
                "google",
                "google-generative-ai",
                "gemini-2.5-flash",
            ))),
            tool_result(
                "call_1",
                "bash",
                pi_ai::types::TextOrImageContent::Text(TextContent::new("a")),
            ),
            tool_result(
                "call_2",
                "bash",
                pi_ai::types::TextOrImageContent::Text(TextContent::new("b")),
            ),
        ]),
    );
    let user_turns: Vec<&Value> = converted.iter().filter(|c| c["role"] == "user").collect();
    assert_eq!(user_turns.len(), 2);
    assert_eq!(user_turns[1]["parts"].as_array().unwrap().len(), 2);
}

#[test]
fn tool_result_errors_use_the_error_response_key() {
    let model = model("google-generative-ai", "google", "gemini-2.5-flash", false);
    let result = ToolResultMessage {
        role: Default::default(),
        tool_call_id: "call_1".to_owned(),
        tool_name: "bash".to_owned(),
        content: vec![pi_ai::types::TextOrImageContent::Text(TextContent::new("boom"))],
        details: None,
        usage: None,
        added_tool_names: None,
        is_error: true,
        timestamp: 1.0,
    };
    let converted = contents(&model, &context(vec![Message::ToolResult(result)]));
    assert_eq!(
        converted[0]["parts"][0]["functionResponse"]["response"]["error"],
        "boom"
    );
}

fn tool(parameters: Value) -> Tool {
    Tool::new("test_tool", "A test tool", parameters)
}

fn declarations(tools: &[Tool], use_parameters: bool) -> Vec<Value> {
    convert_tools(tools, use_parameters, true)
        .expect("tools convert")
        .expect("tools are present")
}

#[test]
fn legacy_parameters_strip_json_schema_meta_keys_recursively() {
    let tools = [tool(json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "$id": "urn:bash-tool",
        "$comment": "A bash tool for demonstration",
        "$defs": {"commandDef": {"type": "string"}},
        "definitions": {"legacyDef": {"type": "number"}},
        "type": "object",
        "properties": {"command": {"type": "string"}},
        "required": ["command"]
    }))];
    let converted = declarations(&tools, true);
    let parameters = &converted[0]["functionDeclarations"][0]["parameters"];
    assert_eq!(
        *parameters,
        json!({
            "type": "object",
            "properties": {"command": {"type": "string"}},
            "required": ["command"]
        })
    );
}

#[test]
fn nested_meta_keys_are_stripped_and_refs_preserved() {
    let tools = [tool(json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "properties": {
            "deep": {"$schema": "http://json-schema.org/draft-07/schema#", "$id": "urn:nested", "type": "string"},
            "refProp": {"$ref": "#/$defs/someDef", "type": "string"}
        }
    }))];
    let converted = declarations(&tools, true);
    assert_eq!(
        converted[0]["functionDeclarations"][0]["parameters"],
        json!({
            "type": "object",
            "properties": {
                "deep": {"type": "string"},
                "refProp": {"$ref": "#/$defs/someDef", "type": "string"}
            }
        })
    );
}

#[test]
fn json_schema_parameters_keep_their_meta_keys() {
    let parameters = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "properties": {"command": {"type": "string"}},
        "required": ["command"]
    });
    let tools = [tool(parameters.clone())];
    let converted = declarations(&tools, false);
    assert_eq!(
        converted[0]["functionDeclarations"][0]["parametersJsonSchema"],
        parameters
    );
    assert_eq!(tools[0].parameters, parameters);
}

#[test]
fn empty_tool_lists_produce_no_tools() {
    assert!(convert_tools(&[], false, true).unwrap().is_none());
    assert!(convert_tools(&[], true, true).unwrap().is_none());
}

#[test]
fn strict_tools_use_validated_mode_on_gemini3() {
    let mut strict_tool = tool(json!({"type": "object", "properties": {}}));
    strict_tool.constrained_sampling = Some(pi_ai::types::ConstrainedSampling::Config(
        pi_ai::types::ConstrainedSamplingConfig::JsonSchema {
            strict: pi_ai::types::JsonSchemaStrictness::Require,
        },
    ));
    assert!(supports_google_strict_tool_sampling("gemini-3.1-pro-preview"));
    assert!(!supports_google_strict_tool_sampling("gemini-2.5-pro"));
    assert_eq!(
        resolve_google_function_calling_mode(&[strict_tool.clone()], None, true).unwrap(),
        Some(FunctionCallingConfigMode::Validated)
    );
    let error: ConstrainedSamplingError =
        resolve_google_function_calling_mode(&[strict_tool], None, false).unwrap_err();
    assert_eq!(
        error.message,
        "Tool \"test_tool\" requires JSON-schema constrained sampling, but strict tools are unsupported."
    );
}

#[test]
fn tool_choice_maps_to_function_calling_modes() {
    let tools = [tool(json!({"type": "object", "properties": {}}))];
    assert_eq!(
        resolve_google_function_calling_mode(&tools, Some("none"), false).unwrap(),
        Some(FunctionCallingConfigMode::None)
    );
    assert_eq!(
        resolve_google_function_calling_mode(&tools, Some("any"), false).unwrap(),
        Some(FunctionCallingConfigMode::Any)
    );
    assert_eq!(
        resolve_google_function_calling_mode(&tools, Some("auto"), false).unwrap(),
        Some(FunctionCallingConfigMode::Auto)
    );
    assert_eq!(
        resolve_google_function_calling_mode(&tools, Some("bogus"), false).unwrap(),
        Some(FunctionCallingConfigMode::Auto)
    );
    assert_eq!(resolve_google_function_calling_mode(&tools, None, false).unwrap(), None);
}

#[test]
fn finish_reasons_map_to_stop_reasons() {
    assert_eq!(map_stop_reason("STOP").unwrap(), StopReason::Stop);
    assert_eq!(map_stop_reason("MAX_TOKENS").unwrap(), StopReason::Length);
    for reason in [
        "BLOCKLIST",
        "PROHIBITED_CONTENT",
        "SPII",
        "SAFETY",
        "IMAGE_SAFETY",
        "IMAGE_PROHIBITED_CONTENT",
        "IMAGE_RECITATION",
        "IMAGE_OTHER",
        "RECITATION",
        "FINISH_REASON_UNSPECIFIED",
        "OTHER",
        "LANGUAGE",
        "MALFORMED_FUNCTION_CALL",
        "UNEXPECTED_TOOL_CALL",
        "NO_IMAGE",
    ] {
        assert_eq!(map_stop_reason(reason).unwrap(), StopReason::Error, "{reason}");
    }
    assert_eq!(map_stop_reason("NOVEL").unwrap_err(), "Unhandled stop reason: NOVEL");
    assert_eq!(map_stop_reason_string("STOP"), StopReason::Stop);
    assert_eq!(map_stop_reason_string("MAX_TOKENS"), StopReason::Length);
    assert_eq!(map_stop_reason_string("SAFETY"), StopReason::Error);
}

#[test]
fn thinking_levels_serialize_as_google_enum_values() {
    assert_eq!(
        serde_json::to_value(GoogleThinkingLevel::Unspecified).unwrap(),
        json!("THINKING_LEVEL_UNSPECIFIED")
    );
    assert_eq!(
        serde_json::to_value(GoogleThinkingLevel::Minimal).unwrap(),
        json!("MINIMAL")
    );
    assert_eq!(serde_json::to_value(GoogleThinkingLevel::Low).unwrap(), json!("LOW"));
    assert_eq!(
        serde_json::to_value(GoogleThinkingLevel::Medium).unwrap(),
        json!("MEDIUM")
    );
    assert_eq!(serde_json::to_value(GoogleThinkingLevel::High).unwrap(), json!("HIGH"));
}

fn google_error(status: Option<f64>) -> pi_ai::api::google_shared::GoogleRequestError {
    pi_ai::api::google_shared::GoogleRequestError::new("boom", status)
}

#[tokio::test]
async fn retry_google_request_retries_a_retryable_status() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&attempts);
    let result = retry_google_request(
        move || {
            let seen = Arc::clone(&seen);
            async move {
                let attempt = seen.fetch_add(1, Ordering::SeqCst);
                if attempt == 0 {
                    Err(google_error(Some(429.0)))
                } else {
                    Ok("ok")
                }
            }
        },
        GoogleRetryOptions {
            max_retries: Some(1.0),
            max_retry_delay_ms: None,
            signal: None,
        },
    )
    .await
    .expect("retry succeeds");
    assert_eq!(result, "ok");
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn retry_google_request_does_not_retry_without_max_retries() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&attempts);
    let error = retry_google_request(
        move || {
            let seen = Arc::clone(&seen);
            async move {
                seen.fetch_add(1, Ordering::SeqCst);
                Err::<&str, _>(google_error(Some(429.0)))
            }
        },
        GoogleRetryOptions::default(),
    )
    .await
    .unwrap_err();
    assert_eq!(error.message, "boom");
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn retry_google_request_does_not_retry_non_retryable_or_statusless_errors() {
    for (status, expected_attempts) in [(Some(400.0), 1), (None, 1)] {
        let attempts = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&attempts);
        let error = retry_google_request(
            move || {
                let seen = Arc::clone(&seen);
                async move {
                    seen.fetch_add(1, Ordering::SeqCst);
                    Err::<&str, _>(google_error(status))
                }
            },
            GoogleRetryOptions {
                max_retries: Some(2.0),
                max_retry_delay_ms: None,
                signal: None,
            },
        )
        .await
        .unwrap_err();
        assert_eq!(error.message, "boom");
        assert_eq!(attempts.load(Ordering::SeqCst), expected_attempts);
    }
}
