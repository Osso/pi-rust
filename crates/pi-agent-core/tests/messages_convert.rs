use pi_agent_core::agent::AgentMessage;
use pi_agent_core::harness::messages::{
    BRANCH_SUMMARY_PREFIX, BRANCH_SUMMARY_SUFFIX, COMPACTION_SUMMARY_PREFIX, COMPACTION_SUMMARY_SUFFIX,
    bash_execution_to_text, convert_to_llm,
};
use pi_ai::types::{
    AssistantContent, AssistantMessage, Message, StopReason, TextContent, TextOrImageContent, ToolResultMessage,
    ToolResultRole, UserContent, UserMessage, UserRole,
};
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use serde_json::{Value, json};

#[test]
fn custom_conversion_keeps_fractional_nonfinite_and_signed_zero_times() {
    for timestamp in [12.75, -0.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let message = AgentMessage::Custom(
            [
                ("role".into(), JsValue::String("custom".into())),
                ("content".into(), JsValue::String("hi".into())),
                ("timestamp".into(), JsValue::Number(timestamp)),
            ]
            .into_iter()
            .collect(),
        );
        let converted = convert_to_llm(&[message]).unwrap();
        assert_eq!(message_timestamp(&converted[0]).to_bits(), timestamp.to_bits());
    }
}

fn user(text: &str) -> AgentMessage {
    AgentMessage::User(UserMessage {
        role: UserRole::Tag,
        content: UserContent::Blocks(vec![TextOrImageContent::Text(TextContent::new(text))]),
        input_source: None,
        provider_native: None,
        timestamp: 1.0,
    })
}

fn assistant(text: &str) -> AgentMessage {
    let mut message = AssistantMessage::empty("anthropic-messages", "anthropic", "claude-sonnet-4-5", 1.0);
    message.content = vec![AssistantContent::Text(TextContent::new(text))];
    message.stop_reason = StopReason::Stop;
    AgentMessage::Assistant(Box::new(message))
}

fn tool_result(text: &str) -> AgentMessage {
    AgentMessage::ToolResult(ToolResultMessage {
        role: ToolResultRole::Tag,
        tool_call_id: "call-1".to_owned(),
        tool_name: "read".to_owned(),
        content: vec![TextOrImageContent::Text(TextContent::new(text))],
        details: None,
        is_error: false,
        timestamp: 1.0,
    })
}

fn custom(object: Value) -> AgentMessage {
    AgentMessage::Custom(
        object
            .as_object()
            .cloned()
            .expect("custom message must be an object")
            .into_iter()
            .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
            .collect::<pi_js_compat::js_json::JsObject>(),
    )
}

fn message_timestamp(message: &Message) -> f64 {
    match message {
        Message::User(user) => user.timestamp,
        Message::Assistant(assistant) => assistant.timestamp,
        Message::ToolResult(result) => result.timestamp,
    }
}

fn user_text(message: &Message) -> JsString {
    match message {
        Message::User(user) => match &user.content {
            UserContent::Text(text) => text.clone(),
            UserContent::Blocks(blocks) => JsString::join(
                blocks.iter().map(|block| match block {
                    TextOrImageContent::Text(text) => text.text.clone(),
                    TextOrImageContent::Image(image) => pi_js_compat::js_format!("[image:{}]", image.mime_type),
                }),
                "",
            ),
        },
        other => panic!("expected a user message, got {other:?}"),
    }
}

#[test]
fn passes_standard_roles_through_unchanged() {
    let messages = vec![user("hi"), assistant("hello"), tool_result("done")];
    let converted = convert_to_llm(&messages).expect("convert fixture messages");

    assert_eq!(converted.len(), 3);
    assert_eq!(converted[0], Message::User(user("hi").as_user().expect("user").clone()));
    assert_eq!(
        converted[1],
        Message::Assistant(assistant("hello").as_assistant().expect("assistant").clone().into())
    );
    assert_eq!(
        converted[2],
        Message::ToolResult(tool_result("done").as_tool_result().expect("tool result").clone())
    );
}

#[test]
fn formats_bash_execution_text_exactly() {
    let with_output = custom(json!({
        "role": "bashExecution",
        "command": "ls -la",
        "output": "a\nb",
        "exitCode": 0,
        "cancelled": false,
        "truncated": false,
        "timestamp": 1
    }));
    assert_eq!(
        user_text(&convert_to_llm(&[with_output]).expect("convert fixture messages")[0]),
        "Ran `ls -la`\n```\na\nb\n```"
    );

    let no_output = custom(json!({
        "role": "bashExecution",
        "command": "true",
        "output": "",
        "exitCode": null,
        "cancelled": false,
        "truncated": false,
        "timestamp": 1
    }));
    assert_eq!(
        user_text(&convert_to_llm(&[no_output]).expect("convert fixture messages")[0]),
        "Ran `true`\n(no output)"
    );

    let cancelled = custom(json!({
        "role": "bashExecution",
        "command": "sleep 10",
        "output": "",
        "exitCode": null,
        "cancelled": true,
        "truncated": false,
        "timestamp": 1
    }));
    assert_eq!(
        user_text(&convert_to_llm(&[cancelled]).expect("convert fixture messages")[0]),
        "Ran `sleep 10`\n(no output)\n\n(command cancelled)"
    );

    let failed = custom(json!({
        "role": "bashExecution",
        "command": "false",
        "output": "boom",
        "exitCode": 2,
        "cancelled": false,
        "truncated": false,
        "timestamp": 1
    }));
    assert_eq!(
        user_text(&convert_to_llm(&[failed]).expect("convert fixture messages")[0]),
        "Ran `false`\n```\nboom\n```\n\nCommand exited with code 2"
    );

    let truncated = custom(json!({
        "role": "bashExecution",
        "command": "yes",
        "output": "y",
        "exitCode": 0,
        "cancelled": false,
        "truncated": true,
        "fullOutputPath": "/tmp/full.log",
        "timestamp": 1
    }));
    assert_eq!(
        user_text(&convert_to_llm(&[truncated]).expect("convert fixture messages")[0]),
        "Ran `yes`\n```\ny\n```\n\n[Output truncated. Full output: /tmp/full.log]"
    );
}

#[test]
fn skips_bash_execution_excluded_from_context() {
    let excluded = custom(json!({
        "role": "bashExecution",
        "command": "ls",
        "output": "a",
        "exitCode": 0,
        "cancelled": false,
        "truncated": false,
        "excludeFromContext": true,
        "timestamp": 1
    }));
    let included = custom(json!({
        "role": "bashExecution",
        "command": "ls",
        "output": "a",
        "exitCode": 0,
        "cancelled": false,
        "truncated": false,
        "excludeFromContext": false,
        "timestamp": 1
    }));

    assert!(
        convert_to_llm(&[excluded])
            .expect("convert fixture messages")
            .is_empty()
    );
    assert_eq!(convert_to_llm(&[included]).expect("convert fixture messages").len(), 1);
}

#[test]
fn converts_custom_string_and_block_content_to_user_messages() {
    let string_content = custom(json!({
        "role": "custom",
        "customType": "note",
        "content": "custom content",
        "display": true,
        "timestamp": 7
    }));
    let block_content = custom(json!({
        "role": "custom",
        "customType": "note",
        "content": [
            {"type": "text", "text": "first"},
            {"type": "image", "mimeType": "image/png", "data": "abc"}
        ],
        "display": false,
        "timestamp": 8
    }));

    let converted = convert_to_llm(&[string_content, block_content]).expect("convert fixture messages");
    assert_eq!(converted.len(), 2);
    // The source wraps string content as `[{ type: "text", text }]`.
    assert_eq!(
        converted[0],
        Message::User(UserMessage {
            role: UserRole::Tag,
            content: UserContent::Blocks(vec![TextOrImageContent::Text(TextContent::new("custom content"))]),
            input_source: None,
            provider_native: None,
            timestamp: 7.0,
        })
    );
    assert_eq!(
        converted[1],
        Message::User(UserMessage {
            role: UserRole::Tag,
            content: UserContent::Blocks(vec![
                TextOrImageContent::Text(TextContent::new("first")),
                TextOrImageContent::Image(pi_ai::types::ImageContent::new("abc", "image/png")),
            ]),
            input_source: None,
            provider_native: None,
            timestamp: 8.0,
        })
    );
}

#[test]
fn frames_branch_and_compaction_summaries_as_user_messages() {
    let branch = custom(json!({
        "role": "branchSummary",
        "summary": "branch text",
        "fromId": "entry-1",
        "timestamp": 3
    }));
    let compaction = custom(json!({
        "role": "compactionSummary",
        "summary": "compact text",
        "tokensBefore": 123,
        "timestamp": 4
    }));

    let converted = convert_to_llm(&[branch, compaction]).expect("convert fixture messages");
    assert_eq!(
        user_text(&converted[0]),
        pi_js_compat::js_format!("{}branch text{}", BRANCH_SUMMARY_PREFIX, BRANCH_SUMMARY_SUFFIX)
    );
    assert_eq!(
        user_text(&converted[1]),
        pi_js_compat::js_format!("{}compact text{}", COMPACTION_SUMMARY_PREFIX, COMPACTION_SUMMARY_SUFFIX)
    );
    assert_eq!(message_timestamp(&converted[1]), 4.0);
}

#[test]
fn skips_unknown_roles_and_preserves_order() {
    let unknown = custom(json!({"role": "unknown", "timestamp": 1}));
    let messages = vec![
        unknown,
        user("one"),
        custom(json!({"role": "unknown2", "timestamp": 1})),
        user("two"),
    ];

    let converted = convert_to_llm(&messages).expect("convert fixture messages");
    assert_eq!(converted.len(), 2);
    assert_eq!(user_text(&converted[0]), "one");
    assert_eq!(user_text(&converted[1]), "two");
}

#[test]
fn formats_bash_execution_text_from_the_typed_record() {
    let message = pi_agent_core::harness::messages::BashExecutionMessage {
        role: pi_agent_core::harness::messages::BashExecutionRole::BashExecution,
        command: JsString::from("echo hi"),
        output: JsString::from("hi"),
        exit_code: Some(0.0),
        cancelled: false,
        truncated: false,
        full_output_path: None,
        timestamp: 1.0,
        exclude_from_context: None,
    };

    assert_eq!(bash_execution_to_text(&message), "Ran `echo hi`\n```\nhi\n```");
}
