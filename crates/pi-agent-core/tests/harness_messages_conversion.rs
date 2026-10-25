//! Behavioral port of `packages/agent/src/harness/messages.ts` conversion helpers.

use pi_agent_core::agent::AgentMessage;
use pi_agent_core::harness::messages::{
    BashExecutionMessage, BashExecutionRole, CustomMessageContent, bash_execution_to_text, convert_to_llm,
    create_custom_message,
};
use pi_ai::types::{Message, TextContent, TextOrImageContent, UserContent};
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use serde_json::json;

fn custom(value: serde_json::Value) -> AgentMessage {
    AgentMessage::Custom(
        value
            .as_object()
            .unwrap()
            .clone()
            .into_iter()
            .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
            .collect::<pi_js_compat::js_json::JsObject>(),
    )
}

fn bash_message(command: &str, output: &str) -> BashExecutionMessage {
    BashExecutionMessage {
        role: BashExecutionRole::BashExecution,
        command: JsString::from(command.to_owned()),
        output: JsString::from(output.to_owned()),
        exit_code: None,
        cancelled: false,
        truncated: false,
        full_output_path: None,
        timestamp: 1000.0,
        exclude_from_context: None,
    }
}

#[test]
fn bash_execution_to_text_formats_command_output_and_terminal_notes() {
    assert_eq!(
        bash_execution_to_text(&bash_message("echo hi", "hi\n")),
        "Ran `echo hi`\n```\nhi\n\n```"
    );
    assert_eq!(
        bash_execution_to_text(&bash_message("true", "")),
        "Ran `true`\n(no output)"
    );

    let cancelled = BashExecutionMessage {
        cancelled: true,
        exit_code: Some(130.0),
        ..bash_message("sleep 10", "partial")
    };
    assert_eq!(
        bash_execution_to_text(&cancelled),
        "Ran `sleep 10`\n```\npartial\n```\n\n(command cancelled)"
    );

    let failed = BashExecutionMessage {
        exit_code: Some(17.0),
        ..bash_message("false", "")
    };
    assert_eq!(
        bash_execution_to_text(&failed),
        "Ran `false`\n(no output)\n\nCommand exited with code 17"
    );

    let success = BashExecutionMessage {
        exit_code: Some(0.0),
        ..bash_message("true", "")
    };
    assert_eq!(bash_execution_to_text(&success), "Ran `true`\n(no output)");

    let truncated = BashExecutionMessage {
        truncated: true,
        full_output_path: Some(JsString::from("/tmp/full.log")),
        ..bash_message("seq 100", "1\n2\n")
    };
    assert_eq!(
        bash_execution_to_text(&truncated),
        "Ran `seq 100`\n```\n1\n2\n\n```\n\n[Output truncated. Full output: /tmp/full.log]"
    );

    let missing_path = BashExecutionMessage {
        truncated: true,
        full_output_path: None,
        ..bash_message("seq 100", "")
    };
    assert_eq!(bash_execution_to_text(&missing_path), "Ran `seq 100`\n(no output)");

    let empty_path = BashExecutionMessage {
        truncated: true,
        full_output_path: Some(JsString::default()),
        ..bash_message("seq 100", "")
    };
    assert_eq!(bash_execution_to_text(&empty_path), "Ran `seq 100`\n(no output)");
}

#[test]
fn convert_to_llm_skips_excluded_bash_messages_and_maps_custom_roles() {
    let excluded = custom(json!({
        "role": "bashExecution",
        "command": "echo excluded",
        "output": "excluded",
        "exitCode": null,
        "cancelled": false,
        "truncated": false,
        "timestamp": 7,
        "excludeFromContext": true
    }));
    let included = custom(json!({
        "role": "bashExecution",
        "command": "echo kept",
        "output": "kept",
        "exitCode": 0,
        "cancelled": false,
        "truncated": false,
        "timestamp": 8
    }));
    let custom_text = custom(json!({
        "role": "custom",
        "customType": "note",
        "content": "plain note",
        "display": false,
        "timestamp": 9
    }));
    let custom_blocks = custom(json!({
        "role": "custom",
        "customType": "note",
        "content": [{"type": "text", "text": "block note"}],
        "display": true,
        "timestamp": 10
    }));
    let branch = custom(json!({
        "role": "branchSummary",
        "summary": "branch text",
        "fromId": "entry-1",
        "timestamp": 11
    }));
    let compaction = custom(json!({
        "role": "compactionSummary",
        "summary": "compacted text",
        "tokensBefore": 12,
        "providerNative": {"provider": "openai", "api": "openai-responses", "format": "openai.responses.input", "value": {"id": "x"}},
        "timestamp": 13
    }));
    let unknown = custom(json!({"role": "mystery", "timestamp": 14}));

    let converted = convert_to_llm(&[
        excluded,
        included,
        custom_text,
        custom_blocks,
        branch,
        compaction,
        unknown,
    ])
    .expect("convert fixture messages");

    let json: Vec<_> = converted
        .iter()
        .map(|message| serde_json::to_value(message).unwrap())
        .collect();
    assert_eq!(
        json,
        vec![
            json!({
                "role": "user",
                "content": [{"type": "text", "text": "Ran `echo kept`\n```\nkept\n```"}],
                "timestamp": 8
            }),
            json!({
                "role": "user",
                "content": [{"type": "text", "text": "plain note"}],
                "timestamp": 9
            }),
            json!({
                "role": "user",
                "content": [{"type": "text", "text": "block note"}],
                "timestamp": 10
            }),
            json!({
                "role": "user",
                "content": [{
                    "type": "text",
                    "text": "The following is a summary of a branch that this conversation came back from:\n\n<summary>\nbranch text</summary>"
                }],
                "timestamp": 11
            }),
            json!({
                "role": "user",
                "content": [{
                    "type": "text",
                    "text": "The conversation history before this point was compacted into the following summary:\n\n<summary>\ncompacted text\n</summary>"
                }],
                "providerNative": {"provider": "openai", "api": "openai-responses", "format": "openai.responses.input", "value": {"id": "x"}},
                "timestamp": 13
            }),
        ]
    );
}

#[test]
fn convert_to_llm_passes_typed_messages_through_unchanged() {
    let user = AgentMessage::User(pi_ai::types::UserMessage {
        role: pi_ai::types::UserRole::Tag,
        content: UserContent::Text(JsString::from("hello")),
        input_source: None,
        provider_native: None,
        timestamp: 21.0,
    });
    let converted = convert_to_llm(std::slice::from_ref(&user)).expect("convert fixture messages");
    assert_eq!(
        converted,
        vec![Message::User(match user {
            AgentMessage::User(message) => message,
            _ => unreachable!(),
        })]
    );
}

#[test]
fn create_custom_message_emits_role_custom_type_and_optional_details() {
    // Frozen harness/messages.ts:122–137 accepts date strings, not epoch-number arguments.
    // frozen_custom_message_date_string_arguments_match_native proves the unchanged 1234/1235 outputs.
    assert_eq!(
        create_custom_message(
            &JsString::from("note"),
            CustomMessageContent::Blocks(vec![TextOrImageContent::Text(TextContent::new("hello"))]),
            true,
            Some(JsValue::from(json!({"level": 2}))),
            "1970-01-01T00:00:01.234Z",
        ),
        AgentMessage::Custom(
            (json!({
                "role": "custom",
                "customType": "note",
                "content": [{"type": "text", "text": "hello"}],
                "display": true,
                "details": {"level": 2},
                "timestamp": 1234
            })
            .as_object()
            .unwrap()
            .clone())
            .into_iter()
            .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
            .collect::<pi_js_compat::js_json::JsObject>()
        )
    );
    assert_eq!(
        create_custom_message(
            &JsString::from("note"),
            CustomMessageContent::Text(JsString::from("plain")),
            false,
            None,
            "1970-01-01T00:00:01.235Z"
        ),
        AgentMessage::Custom(
            (json!({
                "role": "custom",
                "customType": "note",
                "content": "plain",
                "display": false,
                "timestamp": 1235
            })
            .as_object()
            .unwrap()
            .clone())
            .into_iter()
            .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
            .collect::<pi_js_compat::js_json::JsObject>()
        )
    );
}

use crate::shared_oracle as oracle;
#[test]
fn frozen_custom_message_date_string_arguments_match_native() {
    let mut oracle = oracle::TsOracle::spawn(
        "stale-expectations-oracle.ts",
        "packages/agent-core/src/harness/messages.ts",
    );
    let timestamps = [
        JsString::from("1234"),
        JsString::from_utf16(vec![1234]),
        JsString::from("1970-01-01T00:00:01.234Z"),
        JsString::from("1970-01-01T00:00:01.235Z"),
    ];
    let observed = oracle.request(&pi_js_compat::js_json!({"kind":"timestamps","timestamps":timestamps}));
    eprintln!(
        "frozen timestamp proof: {}",
        pi_js_compat::js_json::stringify_lossless(&observed)
    );
    assert_eq!(
        observed[0]["parsed"],
        pi_js_compat::js_json!({"kind":"number","value":-23225875200000.0})
    );
    assert_eq!(observed[1]["custom"], pi_js_compat::js_json!({"kind":"nan"}));
    for (index, timestamp) in timestamps.into_iter().enumerate() {
        let message = create_custom_message(
            &"note".into(),
            CustomMessageContent::Text("plain".into()),
            false,
            None,
            timestamp,
        );
        let AgentMessage::Custom(message) = message else {
            unreachable!()
        };
        let number = message.get("timestamp").unwrap().as_f64().unwrap();
        let actual = if number.is_nan() {
            pi_js_compat::js_json!({"kind":"nan"})
        } else {
            pi_js_compat::js_json!({"kind":"number","value":number})
        };
        assert_eq!(actual, observed[index]["custom"]);
    }
}
