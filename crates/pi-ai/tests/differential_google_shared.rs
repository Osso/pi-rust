#[path = "support/oracle.rs"]
mod oracle;
use pi_ai::api::google_shared::{convert_messages, convert_tools};
use pi_js_compat::{
    js_json,
    js_json::{JsValue, stringify_lossless},
};

fn model(api: &str, id: &str) -> JsValue {
    js_json!({"id":id,"name":id,"api":api,"provider":api,"baseUrl":"https://unused.invalid",
        "reasoning":true,"input":["text","image"],"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},"contextWindow":8192,"maxTokens":4096})
}

fn assistant(content: JsValue, api: &str, id: &str) -> JsValue {
    let mut message = pi_ai::types::AssistantMessage::empty(api, api, id, 1234.0);
    message.stop_reason = pi_ai::types::StopReason::ToolUse;
    let mut value = js_json!(message);
    value["content"] = content;
    value
}

fn compare_messages(model: JsValue, context: JsValue) {
    let mut source = oracle::TsOracle::spawn("google-shared-oracle.ts", "packages/ai/src/api/google-shared.ts");
    let expected = source.request(&js_json!({"operation":"messages","model":model,"context":context,"now":1234}));
    assert_eq!(expected["ok"], js_json!(true));
    let model = pi_js_compat::js_json::from_value(&model).unwrap();
    let context = pi_js_compat::js_json::from_value(&context).unwrap();
    let actual = js_json!(convert_messages(&model, &context, 1234.0));
    assert_eq!(stringify_lossless(&actual), stringify_lossless(&expected["output"]));
}

#[test]
fn shared_google_content_and_tool_payload_order_match_frozen() {
    compare_messages(
        model("google-generative-ai", "gemini-3.1-pro"),
        js_json!({"messages":[
            {"role":"user","content":[{"type":"text","text":"look"},{"type":"image","data":"AAAA","mimeType":"image/png"}],"timestamp":1234}
        ]}),
    );
    let mut source = oracle::TsOracle::spawn("google-shared-oracle.ts", "packages/ai/src/api/google-shared.ts");
    let tools = js_json!([{"name":"read","description":"Read","parameters":{"$schema":"removed","type":"object","properties":{"id":{"$id":"removed","type":"string"}},"oneOf":[{"$schema":"kept","type":"object"}]}}]);
    let native: Vec<pi_ai::types::Tool> = pi_js_compat::js_json::from_value(&tools).unwrap();
    for legacy in [false, true] {
        let expected = source.request(&js_json!({"operation":"tools","tools":tools,"legacy":legacy}));
        assert_eq!(
            stringify_lossless(&js_json!(convert_tools(&native, legacy))),
            stringify_lossless(&expected["output"])
        );
    }
}

#[test]
fn signed_empty_google_replay_blocks_are_dropped_like_frozen() {
    let api = "google-generative-ai";
    let id = "gemini-3.1-pro";
    let message = assistant(
        js_json!([
            {"type":"text","text":"","textSignature":"YWJj"},
            {"type":"thinking","thinking":" \t","thinkingSignature":"YWJj"}
        ]),
        api,
        id,
    );
    compare_messages(model(api, id), js_json!({"messages":[message]}));
}

#[test]
fn google_tool_id_replacement_counts_utf16_units() {
    let api = "google-vertex";
    let id = "claude-opus-4";
    let call = assistant(
        js_json!([{"type":"toolCall","id":"call🙈tail","name":"read","arguments":{"key":"value"}}]),
        api,
        id,
    );
    let result = js_json!({"role":"toolResult","toolCallId":"call🙈tail","toolName":"read","content":[{"type":"text","text":"done"}],"isError":false,"timestamp":1234});
    compare_messages(model(api, id), js_json!({"messages":[call,result]}));
}

#[test]
fn google_multimodal_function_results_and_merging_match_frozen() {
    for id in ["gemini-2.5-flash", "gemini-live-3.1-pro", "gpt-oss-120b"] {
        let content = js_json!([{"type":"image","data":"AAAA","mimeType":"image/png"}]);
        let result = js_json!({"role":"toolResult","toolCallId":"call","toolName":"read","content":content,"isError":true,"timestamp":1234});
        compare_messages(
            model("google-vertex", id),
            js_json!({"messages":[result.clone(),result]}),
        );
    }
}

#[test]
fn all_google_shared_helper_exports_match_frozen() {
    use pi_ai::api::google_shared::*;
    let mut source = oracle::TsOracle::spawn("google-shared-oracle.ts", "packages/ai/src/api/google-shared.ts");
    let reasons = [
        "STOP",
        "MAX_TOKENS",
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
        "FUTURE_REASON",
    ];
    let ids = [
        "gemini-flash-latest",
        "GEMINI-FLASH-LATEST",
        "gemini-flash-lite-latest",
        "claude-opus-4",
        "gpt-oss-120b",
        "gemini-2.5-flash",
    ];
    for (index, reason) in reasons.into_iter().enumerate() {
        let model_id = ids[index % ids.len()];
        let max_tokens = [0.0, 0.5, 65535.0, 65536.0][index % 4];
        let thought = [None, Some(false), Some(true)][index % 3];
        let (existing, incoming) = [
            (None, None),
            (Some("old"), None),
            (Some("old"), Some("")),
            (Some("old"), Some("new")),
            (Some(""), None),
        ][index % 5];
        let choice = ["auto", "none", "any", "unknown"][index % 4];
        let mut request = js_json!({"operation":"helpers","reason":reason,"modelId":model_id,"maxTokens":max_tokens,"part":{"thoughtSignature":"YWJj"},"choice":choice});
        if let Some(thought) = thought {
            request["part"]["thought"] = js_json!(thought);
        }
        if let Some(existing) = existing {
            request["existing"] = js_json!(existing);
        }
        if let Some(incoming) = incoming {
            request["incoming"] = js_json!(incoming);
        }
        let stop = match map_stop_reason(reason) {
            Ok(reason) => js_json!(reason),
            Err(error) => js_json!({"error":error}),
        };
        let actual = js_json!({"max":clamp_google_max_output_tokens(max_tokens),"managed":uses_server_managed_thinking(model_id),
            "requiresId":requires_tool_call_id(model_id),"thinking":is_thinking_part(thought),
            "signature":retain_thought_signature(existing,incoming),"choice":map_tool_choice(choice),"stop":stop,"rawStop":map_stop_reason_string(reason)});
        let expected = source.request(&request);
        assert_eq!(stringify_lossless(&actual), stringify_lossless(&expected["output"]));
    }
    assert_eq!(GOOGLE_MAX_OUTPUT_TOKENS, 65535.0);
}
