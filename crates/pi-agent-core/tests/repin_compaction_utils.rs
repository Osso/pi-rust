use crate::shared_oracle as oracle;
use pi_agent_core::AgentMessage;
use pi_agent_core::harness::compaction::utils::*;
use pi_ai::types::{AssistantContent, AssistantMessage, Message, TextContent, ThinkingContent, ToolCall};
use pi_js_compat::{
    js_json as json,
    js_json::{JsString, JsValue, from_value, to_value},
};
fn assistant(content: Vec<AssistantContent>) -> AssistantMessage {
    let mut message = AssistantMessage::empty("fixture", "fixture", "fixture", 0.0);
    message.content = content;
    message
}
fn call(name: &str, path: JsValue) -> AssistantContent {
    AssistantContent::ToolCall(ToolCall::new("call", name, [("path".into(), path)].into()))
}
#[test]
fn frozen_file_operation_extraction_ignores_empty_paths_and_keeps_native_set_order() {
    let mut oracle = oracle::TsOracle::spawn(
        "agent-core-compaction-utils-oracle.ts",
        "packages/agent-core/src/harness/compaction/utils.ts",
    );
    let mut blocks = vec![];
    for name in ["read", "write", "edit"] {
        blocks.push(call(name, json!("")));
        blocks.push(call(name, json!(5)));
    }
    for (name, path) in [
        ("read", JsString::from_utf16(vec![0xd800])),
        ("read", "x".into()),
        ("write", "x".into()),
        ("edit", "x".into()),
        ("write", "y".into()),
        ("unknown", "ignored".into()),
    ] {
        blocks.push(call(name, JsValue::String(path)));
    }
    let messages: Vec<AgentMessage> = vec![
        assistant(blocks).into(),
        from_value(&json!({"role":"user","content":"ignored","timestamp":0})).unwrap(),
    ];
    let expected: JsValue = oracle.request(&json!({"op":"fileOps","messages":to_value(&messages).unwrap()}));
    let mut ops = create_file_ops();
    for message in &messages {
        extract_file_ops_from_message(message, &mut ops);
    }
    let lists = compute_file_lists(&ops);
    let actual = json!({"ops":{"read":ops.read,"written":ops.written,"edited":ops.edited},"lists":{"readFiles":lists.read_files,"modifiedFiles":lists.modified_files},"formatted":format_file_operations(&lists.read_files,&lists.modified_files)});
    assert_eq!(actual, expected);
}
#[test]
fn frozen_conversation_serialization_preserves_utf16_arguments_and_full_output_branches() {
    let mut oracle = oracle::TsOracle::spawn(
        "agent-core-compaction-utils-oracle.ts",
        "packages/agent-core/src/harness/compaction/utils.ts",
    );
    let mut messages: Vec<Message> = vec![
        from_value(&json!({"role":"user","content":"","timestamp":0})).unwrap(),
        from_value(
            &json!({"role":"user","content":[{"type":"image","data":"AA==","mimeType":"image/png"}],"timestamp":0}),
        )
        .unwrap(),
    ];
    let args =
        pi_js_compat::js_json::parse_lossless(r#"{"z":"\ud800","2":[1,null],"01":{"\udc00":"x"},"1":true}"#).unwrap();
    messages.push(Message::Assistant(Box::new(assistant(vec![
        AssistantContent::Thinking(ThinkingContent::new("ignored")),
        AssistantContent::Text(TextContent::new("")),
        AssistantContent::Text(TextContent::new(JsString::from_utf16(vec![0xdc00]))),
        AssistantContent::ToolCall(ToolCall::new("call", "lookup", args.as_object().unwrap().clone())),
    ]))));
    let texts = [
        JsString::from("short"),
        JsString::from("short"),
        JsString::from("full/path"),
        JsString::from_utf16(vec![0xd800; 2001]),
        JsString::from("x".repeat(2000)),
        JsString::from("x".repeat(2001)),
    ];
    let details = [
        json!({"fullOutputPath":"full/path"}),
        json!({"fullOutputPath":"full/path","truncation":{"truncated":true}}),
        json!({"fullOutputPath":"full/path","truncation":{"truncated":true}}),
        json!({"fullOutputPath":JsString::from_utf16(vec![0xdc00])}),
        json!({"fullOutputPath":""}),
        JsValue::Null,
    ];
    for (content, details) in texts.into_iter().zip(details) {
        messages.push(from_value(&json!({"role":"toolResult","toolCallId":"call","toolName":"lookup","content":[{"type":"text","text":content}],"details":details,"isError":false,"timestamp":0})).unwrap());
    }
    let expected: JsValue = oracle.request(&json!({"op":"serialize","messages":to_value(&messages).unwrap()}));
    assert_eq!(json!(serialize_conversation(&messages)), expected);
}
