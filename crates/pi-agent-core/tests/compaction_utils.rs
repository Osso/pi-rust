use indexmap::IndexSet;
use pi_agent_core::agent::AgentMessage;
use pi_agent_core::harness::compaction::utils::{
    FileOperations, compute_file_lists, create_file_ops, extract_file_ops_from_message, format_file_operations,
    serialize_conversation,
};
use pi_ai::types::{
    AssistantContent, AssistantMessage, Message, StopReason, TextContent, TextOrImageContent, ToolCall,
    ToolResultMessage, ToolResultRole, UserContent, UserMessage, UserRole,
};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::{JsObject as Map, JsValue as Value};

fn user(text: &str) -> Message {
    Message::User(UserMessage {
        role: UserRole::Tag,
        content: UserContent::Text(JsString::from(text.to_owned())),
        input_source: None,
        provider_native: None,
        timestamp: 1.0,
    })
}

fn assistant(content: Vec<AssistantContent>) -> Message {
    let mut message = AssistantMessage::empty("anthropic-messages", "anthropic", "claude-sonnet-4-5", 1.0);
    message.content = content;
    message.stop_reason = StopReason::Stop;
    Message::Assistant(Box::new(message))
}

fn tool_result(text: &str, details: Option<Value>) -> Message {
    Message::ToolResult(ToolResultMessage {
        role: ToolResultRole::Tag,
        tool_call_id: "call-1".to_owned(),
        tool_name: "read".to_owned(),
        content: vec![TextOrImageContent::Text(TextContent::new(text))],
        details,
        is_error: false,
        timestamp: 1.0,
    })
}

fn tool_call(name: &str, arguments: Value) -> AgentMessage {
    let mut message = AssistantMessage::empty("anthropic-messages", "anthropic", "claude-sonnet-4-5", 1.0);
    message.content = vec![AssistantContent::ToolCall(ToolCall::new(
        "call-1",
        name,
        arguments
            .as_object()
            .cloned()
            .expect("arguments must be an object")
            .into_iter()
            .collect::<pi_js_compat::js_json::JsObject>(),
    ))];
    message.stop_reason = StopReason::ToolUse;
    AgentMessage::Assistant(Box::new(message))
}

#[test]
fn creates_empty_file_operations() {
    let file_ops = create_file_ops();

    assert!(file_ops.read.is_empty());
    assert!(file_ops.written.is_empty());
    assert!(file_ops.edited.is_empty());
}

#[test]
fn extracts_read_write_and_edit_paths_from_tool_calls() {
    let mut file_ops = create_file_ops();
    extract_file_ops_from_message(&tool_call("read", json!({"path": "a.ts"})), &mut file_ops);
    extract_file_ops_from_message(&tool_call("write", json!({"path": "b.ts"})), &mut file_ops);
    extract_file_ops_from_message(&tool_call("edit", json!({"path": "c.ts"})), &mut file_ops);

    assert_eq!(file_ops.read, IndexSet::from([JsString::from("a.ts")]));
    assert_eq!(file_ops.written, IndexSet::from([JsString::from("b.ts")]));
    assert_eq!(file_ops.edited, IndexSet::from([JsString::from("c.ts")]));
}

#[test]
fn ignores_non_matching_tool_calls_and_messages() {
    let mut file_ops = create_file_ops();
    extract_file_ops_from_message(&tool_call("bash", json!({"path": "a.ts"})), &mut file_ops);
    extract_file_ops_from_message(&tool_call("read", json!({"file": "a.ts"})), &mut file_ops);
    extract_file_ops_from_message(&tool_call("read", json!({"path": 5})), &mut file_ops);
    extract_file_ops_from_message(&AgentMessage::from(user("hi")), &mut file_ops);
    extract_file_ops_from_message(&AgentMessage::from(tool_result("x", None)), &mut file_ops);

    assert!(file_ops.read.is_empty());
    assert!(file_ops.written.is_empty());
    assert!(file_ops.edited.is_empty());
}

#[test]
fn accumulates_multiple_tool_calls_in_one_message() {
    let mut message = AssistantMessage::empty("anthropic-messages", "anthropic", "claude-sonnet-4-5", 1.0);
    message.content = vec![
        AssistantContent::ToolCall(ToolCall::new(
            "call-1",
            "read",
            Map::from_iter([(JsString::from("path"), json!("a.ts"))])
                .into_iter()
                .collect::<pi_js_compat::js_json::JsObject>(),
        )),
        AssistantContent::ToolCall(ToolCall::new(
            "call-2",
            "read",
            Map::from_iter([(JsString::from("path"), json!("b.ts"))])
                .into_iter()
                .collect::<pi_js_compat::js_json::JsObject>(),
        )),
        AssistantContent::Text(TextContent::new("done")),
    ];
    let mut file_ops = create_file_ops();
    extract_file_ops_from_message(&AgentMessage::Assistant(Box::new(message)), &mut file_ops);

    assert_eq!(
        file_ops.read,
        IndexSet::from([JsString::from("a.ts"), JsString::from("b.ts")])
    );
}

#[test]
fn computes_sorted_read_only_and_modified_lists() {
    let mut file_ops = FileOperations::default();
    file_ops.read.extend([
        JsString::from("z.ts"),
        JsString::from("a.ts"),
        JsString::from("shared.ts"),
    ]);
    file_ops.written.insert(JsString::from("shared.ts"));
    file_ops.edited.extend([JsString::from("m.ts"), JsString::from("b.ts")]);

    let lists = compute_file_lists(&file_ops);
    assert_eq!(lists.read_files, ["a.ts", "z.ts"]);
    assert_eq!(lists.modified_files, ["b.ts", "m.ts", "shared.ts"]);
}

#[test]
fn sorts_file_lists_by_utf16_code_unit_order() {
    let mut file_ops = FileOperations::default();
    file_ops
        .read
        .extend([JsString::from("\u{e000}.ts"), JsString::from("😀.ts")]);

    let lists = compute_file_lists(&file_ops);
    assert_eq!(lists.read_files, ["😀.ts", "\u{e000}.ts"]);
}

#[test]
fn formats_file_operation_sections() {
    assert_eq!(format_file_operations(&[], &[]), "");
    assert_eq!(
        format_file_operations(&[JsString::from("a.ts")], &[JsString::from("b.ts")]),
        "\n\n<read-files>\na.ts\n</read-files>\n\n<modified-files>\nb.ts\n</modified-files>"
    );
}

#[test]
fn serializes_user_and_assistant_text() {
    let messages = vec![
        user("hello"),
        assistant(vec![
            AssistantContent::Text(TextContent::new("first")),
            AssistantContent::Text(TextContent::new("second")),
        ]),
    ];

    assert_eq!(
        serialize_conversation(&messages),
        "[User]: hello\n\n[Assistant]: first\nsecond"
    );
}

#[test]
fn serializes_tool_calls_with_json_arguments_in_insertion_order() {
    let mut arguments = Map::new();
    arguments.insert(JsString::from("path"), json!("src/a.ts"));
    arguments.insert(JsString::from("limit"), json!(10));
    arguments.insert(JsString::from("flag"), json!(true));
    let messages = vec![assistant(vec![
        AssistantContent::Text(TextContent::new("reading")),
        AssistantContent::ToolCall(ToolCall::new(
            "call-1",
            "read",
            arguments.into_iter().collect::<pi_js_compat::js_json::JsObject>(),
        )),
    ])];

    assert_eq!(
        serialize_conversation(&messages),
        "[Assistant]: reading\n\n[Assistant tool calls]: read(path=\"src/a.ts\", limit=10, flag=true)"
    );
}

#[test]
fn serializes_tool_results_with_truncation_marker() {
    let long_content = "x".repeat(5000);
    let messages = vec![tool_result(&long_content, None)];
    let serialized = serialize_conversation(&messages);

    assert!(serialized.starts_with("[Tool result]: "));
    assert!(serialized.contains("[... 3000 more characters truncated]"));
    assert!(!serialized.contains(&long_content));
}

#[test]
fn appends_full_output_path_only_when_truncated_or_omitted() {
    let path = "/tmp/full.log";
    let long_content = "x".repeat(5000);
    let details = Some(json!({"fullOutputPath": path, "truncation": {"truncated": true}}));

    let truncated = serialize_conversation(&[tool_result(&long_content, details.clone())]);
    assert!(truncated.ends_with(format!("\n\n[Output truncated. Full output: {path}]")));

    let short_truncated = serialize_conversation(&[tool_result("short", details)]);
    assert_eq!(
        short_truncated,
        format!("[Tool result]: short\n\n[Output truncated. Full output: {path}]")
    );

    let short_untruncated = serialize_conversation(&[tool_result(
        "short",
        Some(json!({"fullOutputPath": path, "truncation": {"truncated": false}})),
    )]);
    assert_eq!(short_untruncated, "[Tool result]: short");

    let already_contains_path = serialize_conversation(&[tool_result(
        &format!("see {path} for output"),
        Some(json!({"fullOutputPath": path})),
    )]);
    assert_eq!(already_contains_path, format!("[Tool result]: see {path} for output"));
}

#[test]
fn skips_empty_parts_and_joins_with_blank_lines() {
    let messages = vec![user(""), tool_result("", None), user("kept")];
    assert_eq!(serialize_conversation(&messages), "[User]: kept");
}

#[test]
fn ignores_image_blocks_in_user_content() {
    let messages = vec![Message::User(UserMessage {
        role: UserRole::Tag,
        content: UserContent::Blocks(vec![
            TextOrImageContent::Text(TextContent::new("caption")),
            TextOrImageContent::Image(pi_ai::types::ImageContent::new("abc", "image/png")),
        ]),
        input_source: None,
        provider_native: None,
        timestamp: 1.0,
    })];

    assert_eq!(serialize_conversation(&messages), "[User]: caption");
}

#[test]
fn serializes_numeric_keys_and_nested_values_in_js_order() {
    let arguments: pi_js_compat::js_json::JsObject = pi_js_compat::js_json::parse_lossless(
        r#"{"nested":{"2":2.0,"1":1.0,"01":3},"2":2.0,"1":1.0,"4294967295":4,"01":5}"#,
    )
    .expect("JSON tool arguments")
    .as_object()
    .expect("object tool arguments")
    .clone();
    let messages = [assistant(vec![AssistantContent::ToolCall(ToolCall::new(
        "id",
        "inspect",
        arguments.into_iter().collect::<pi_js_compat::js_json::JsObject>(),
    ))])];
    assert_eq!(
        serialize_conversation(&messages),
        r#"[Assistant tool calls]: inspect(1=1, 2=2, nested={"1":1,"2":2,"01":3}, 4294967295=4, 01=5)"#
    );
}
