use std::collections::HashMap;
use std::sync::Arc;

use pi_agent_core::agent::AgentMessage;
use pi_agent_core::harness::session::{
    Entry, EntryBody, OptionalValue, SessionContextBuildOptions, SessionModel, build_session_context,
    default_context_entry_transform,
};
use pi_ai::types::{AssistantMessage, StopReason, TextContent, TextOrImageContent, UserContent, UserMessage, UserRole};
use serde_json::json;

fn user_message(text: &str) -> AgentMessage {
    AgentMessage::User(UserMessage {
        role: UserRole::Tag,
        content: UserContent::Blocks(vec![TextOrImageContent::Text(TextContent::new(text))]),
        input_source: None,
        provider_native: None,
        timestamp: 1,
    })
}

fn assistant_message(text: &str) -> AgentMessage {
    let mut message = AssistantMessage::empty("anthropic-messages", "anthropic", "claude-sonnet-4-5", 1);
    message.content = vec![pi_ai::types::AssistantContent::Text(TextContent::new(text))];
    message.stop_reason = StopReason::Stop;
    AgentMessage::Assistant(Box::new(message))
}

fn entry(body: EntryBody, seq: i64) -> Entry {
    Entry {
        body,
        parent_id: None,
        seq,
        timestamp: seq,
    }
}

fn roles(context: &pi_agent_core::harness::session::SessionContext) -> Vec<String> {
    context
        .messages
        .iter()
        .map(|message| message.role().to_owned())
        .collect()
}

#[test]
fn starts_at_the_latest_compaction_and_materializes_its_retained_tail() {
    let entries = vec![
        entry(
            EntryBody::Message {
                id: "old".to_owned(),
                message: user_message("old"),
                terminate: None,
            },
            1,
        ),
        entry(
            EntryBody::Compaction {
                id: "compact".to_owned(),
                summary: "summary".to_owned(),
                tokens_before: 100.0,
                retained_tail: vec![user_message("retained"), assistant_message("answer")],
                duration_ms: None,
                details: OptionalValue::Missing,
                usage: None,
            },
            2,
        ),
        entry(
            EntryBody::ModelChange {
                id: "model".to_owned(),
                provider: "openai".to_owned(),
                model_id: "gpt-5".to_owned(),
            },
            3,
        ),
        entry(
            EntryBody::ThinkingLevelChange {
                id: "thinking".to_owned(),
                thinking_level: "high".to_owned(),
            },
            4,
        ),
        entry(
            EntryBody::Message {
                id: "tail".to_owned(),
                message: user_message("tail"),
                terminate: None,
            },
            5,
        ),
    ];

    let context = build_session_context(&entries, &SessionContextBuildOptions::default());
    assert_eq!(roles(&context), vec!["compactionSummary", "user", "assistant", "user"]);
    assert_eq!(
        context.model,
        Some(SessionModel {
            provider: "openai".to_owned(),
            model_id: "gpt-5".to_owned()
        })
    );
    assert_eq!(context.thinking_level, "high");
    assert_eq!(context.active_tool_names, None);
}

#[test]
fn applies_caller_transforms_after_the_compaction_boundary() {
    let entries = vec![
        entry(
            EntryBody::Message {
                id: "old".to_owned(),
                message: user_message("old"),
                terminate: None,
            },
            1,
        ),
        entry(
            EntryBody::Compaction {
                id: "compact".to_owned(),
                summary: "summary".to_owned(),
                tokens_before: 100.0,
                retained_tail: Vec::new(),
                duration_ms: None,
                details: OptionalValue::Missing,
                usage: None,
            },
            2,
        ),
        entry(
            EntryBody::BranchSummary {
                id: "branch".to_owned(),
                from_id: "abandoned".to_owned(),
                summary: "branch summary".to_owned(),
                details: OptionalValue::Missing,
                usage: None,
            },
            3,
        ),
        entry(
            EntryBody::Message {
                id: "tail".to_owned(),
                message: user_message("tail"),
                terminate: None,
            },
            4,
        ),
    ];

    let options = SessionContextBuildOptions {
        entry_transforms: vec![Arc::new(|entries: &[Entry]| {
            entries
                .iter()
                .filter(|entry| entry.entry_type() != pi_agent_core::harness::session::EntryType::Compaction)
                .cloned()
                .collect()
        })],
        entry_projectors: HashMap::new(),
    };
    let context = build_session_context(&entries, &options);
    assert_eq!(roles(&context), vec!["branchSummary", "user"]);
    match &context.messages[0] {
        AgentMessage::Custom(object) => {
            assert_eq!(object["role"], json!("branchSummary"));
            assert_eq!(object["summary"], json!("branch summary"));
            assert_eq!(object["fromId"], json!("abandoned"));
            assert_eq!(object["timestamp"], json!(3));
        }
        other => panic!("expected custom branch summary, got {other:?}"),
    }
}

#[test]
fn projects_custom_entries_and_omits_deferred_assistant_handles() {
    let mut deferred = AssistantMessage::empty("openai-responses", "openai", "gpt-5", 2);
    deferred.stop_reason = StopReason::Deferred;
    deferred.deferred = Some(pi_ai::types::DeferredHandle {
        provider: "openai".to_owned(),
        model_id: "gpt-5".to_owned(),
        api: "openai-responses".to_owned(),
        id: "response-1".to_owned(),
        expires_at: None,
        poll_after_ms: None,
        data: None,
    });
    let entries = vec![
        entry(
            EntryBody::Message {
                id: "user".to_owned(),
                message: user_message("hello"),
                terminate: None,
            },
            1,
        ),
        entry(
            EntryBody::Message {
                id: "deferred".to_owned(),
                message: AgentMessage::Assistant(Box::new(deferred)),
                terminate: None,
            },
            2,
        ),
        entry(
            EntryBody::Custom {
                id: "custom".to_owned(),
                custom_type: "note".to_owned(),
                data: json!("project me").into(),
            },
            3,
        ),
    ];

    let mut projectors = HashMap::new();
    projectors.insert(
        "note".to_owned(),
        Arc::new(|entry: &Entry, _index: usize, _entries: &[Entry]| {
            let EntryBody::Custom { data, .. } = &entry.body else {
                return None;
            };
            let value = data.as_value().and_then(|value| value.as_str()).unwrap_or_default();
            Some(vec![user_message(&format!("note: {value}"))])
        }) as pi_agent_core::harness::session::CustomEntryContextMessageProjector,
    );
    let options = SessionContextBuildOptions {
        entry_transforms: Vec::new(),
        entry_projectors: projectors,
    };
    let context = build_session_context(&entries, &options);
    assert_eq!(roles(&context), vec!["user", "user"]);
    match &context.messages[1] {
        AgentMessage::User(message) => {
            let UserContent::Blocks(blocks) = &message.content else {
                panic!("expected blocks");
            };
            let TextOrImageContent::Text(block) = &blocks[0] else {
                panic!("expected text");
            };
            assert_eq!(block.text, "note: project me");
        }
        other => panic!("expected projected user message, got {other:?}"),
    }
}

#[test]
fn derives_model_from_assistant_messages_and_keeps_active_tools() {
    let entries = vec![
        entry(
            EntryBody::ActiveToolsChange {
                id: "tools".to_owned(),
                active_tool_names: vec!["read".to_owned(), "bash".to_owned()],
            },
            1,
        ),
        entry(
            EntryBody::Message {
                id: "assistant".to_owned(),
                message: assistant_message("answer"),
                terminate: None,
            },
            2,
        ),
    ];
    let context = build_session_context(&entries, &SessionContextBuildOptions::default());
    assert_eq!(
        context.model,
        Some(SessionModel {
            provider: "anthropic".to_owned(),
            model_id: "claude-sonnet-4-5".to_owned()
        })
    );
    assert_eq!(
        context.active_tool_names,
        Some(vec!["read".to_owned(), "bash".to_owned()])
    );
    assert_eq!(context.thinking_level, "off");
}

#[test]
fn default_transform_keeps_everything_without_a_compaction() {
    let entries = vec![entry(
        EntryBody::Message {
            id: "only".to_owned(),
            message: user_message("only"),
            terminate: None,
        },
        1,
    )];
    assert_eq!(default_context_entry_transform(&entries), entries);
}

#[test]
fn skips_empty_branch_summaries() {
    let entries = vec![entry(
        EntryBody::BranchSummary {
            id: "branch".to_owned(),
            from_id: "old".to_owned(),
            summary: String::new(),
            details: OptionalValue::Missing,
            usage: None,
        },
        1,
    )];
    let context = build_session_context(&entries, &SessionContextBuildOptions::default());
    assert!(context.messages.is_empty());
}

#[test]
fn custom_message_text_helper_matches_expected_shape() {
    let message = pi_agent_core::harness::messages::create_branch_summary_message("summary", "from", 42);
    assert_eq!(message.role(), "branchSummary");
    assert_eq!(
        serde_json::to_string(&message).unwrap(),
        r#"{"role":"branchSummary","summary":"summary","fromId":"from","timestamp":42}"#
    );
    let compaction = pi_agent_core::harness::messages::create_compaction_summary_message(
        "summary",
        100.0,
        42,
        &pi_agent_core::harness::messages::CompactionSummaryOptions::default(),
    );
    assert_eq!(
        serde_json::to_string(&compaction).unwrap(),
        r#"{"role":"compactionSummary","summary":"summary","tokensBefore":100,"timestamp":42}"#
    );
}
