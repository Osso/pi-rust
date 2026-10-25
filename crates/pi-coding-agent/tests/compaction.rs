//! Behavioral tests for `coding-agent/src/core/compaction/{compaction,utils}.ts`.

use pi_agent_core::agent::AgentMessage;
use pi_ai::types::{
    AssistantContent, AssistantMessage, AssistantRole, InputSource, StopReason, TextContent, Usage, UsageCost,
    UserContent, UserMessage, UserRole,
};
use pi_coding_agent::core::compaction::{
    CompactionSettings, DEFAULT_COMPACTION_SETTINGS, calculate_context_tokens, estimate_context_tokens,
    estimate_tokens, find_cut_point, find_turn_start_index, get_last_assistant_usage, prepare_compaction,
    should_compact,
};
use pi_coding_agent::core::session_manager::{
    LeafSelection, SessionEntry, build_session_context, migrate_session_entries, parse_session_entries,
};
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use serde_json::{Value, json};

const TIMESTAMP: &str = "2026-01-01T00:00:00.000Z";

fn usage(input: f64, output: f64, cache_read: f64, cache_write: f64) -> Usage {
    Usage {
        input,
        output,
        cache_read,
        cache_write,
        cache_write_1h: None,
        reasoning: None,
        total_tokens: input + output + cache_read + cache_write,
        cost: UsageCost::default(),
    }
}

fn user(text: &str) -> AgentMessage {
    AgentMessage::User(UserMessage {
        role: UserRole::Tag,
        content: UserContent::Text(JsString::from(text.to_owned())),
        input_source: None,
        provider_native: None,
        timestamp: 0.0,
    })
}

fn extension_user(text: &str) -> AgentMessage {
    let AgentMessage::User(mut message) = user(text) else {
        unreachable!()
    };
    message.input_source = Some(InputSource::Extension);
    AgentMessage::User(message)
}

fn assistant(text: &str, usage: Usage) -> AgentMessage {
    AgentMessage::Assistant(Box::new(AssistantMessage {
        role: AssistantRole::Tag,
        content: vec![AssistantContent::Text(TextContent::new(text))],
        api: "anthropic-messages".to_owned(),
        provider: "anthropic".to_owned(),
        model: "claude-sonnet-4-5".to_owned(),
        response_model: None,
        response_id: None,
        image_generation_result: None,
        diagnostics: None,
        usage,
        stop_reason: StopReason::Stop,
        error_message: None,
        timestamp: 0.0,
    }))
}

fn default_assistant(text: &str) -> AgentMessage {
    assistant(text, usage(100.0, 50.0, 0.0, 0.0))
}

struct EntryBuilder {
    counter: usize,
    last_id: Option<String>,
}

impl EntryBuilder {
    fn new() -> Self {
        Self {
            counter: 0,
            last_id: None,
        }
    }

    fn entry(&mut self, value: Value) -> SessionEntry {
        let id = format!("test-id-{}", self.counter);
        self.counter += 1;
        let mut object = value.as_object().expect("entry fixture must be an object").clone();
        object.insert("id".to_owned(), Value::String(id.clone()));
        object.insert(
            "parentId".to_owned(),
            self.last_id.clone().map(Value::String).unwrap_or(Value::Null),
        );
        object.insert("timestamp".to_owned(), Value::String(TIMESTAMP.to_owned()));
        self.last_id = Some(id);
        serde_json::from_value(Value::Object(object)).expect("entry fixture must deserialize")
    }

    fn message(&mut self, message: &AgentMessage) -> SessionEntry {
        self.entry(json!({ "type": "message", "message": message }))
    }

    fn compaction(&mut self, summary: &str, first_kept_entry_id: &JsString) -> SessionEntry {
        self.entry(json!({
            "type": "compaction",
            "summary": summary,
            "firstKeptEntryId": first_kept_entry_id,
            "tokensBefore": 10_000,
        }))
    }

    fn model_change(&mut self, provider: &str, model_id: &str) -> SessionEntry {
        self.entry(json!({
            "type": "model_change",
            "provider": provider,
            "modelId": model_id,
        }))
    }

    fn thinking_level(&mut self, thinking_level: &str) -> SessionEntry {
        self.entry(json!({
            "type": "thinking_level_change",
            "thinkingLevel": thinking_level,
        }))
    }

    fn custom_message(&mut self, content: &str) -> SessionEntry {
        self.entry(json!({
            "type": "custom_message",
            "customType": "test",
            "content": content,
            "display": true,
        }))
    }
}

fn entry_id(entry: &SessionEntry) -> &JsString {
    entry.id().expect("fixture entry must have an id")
}

fn message_text(messages: &[AgentMessage]) -> JsString {
    let texts = messages.iter().map(|message| match message {
        AgentMessage::User(user) => match &user.content {
            UserContent::Text(text) => text.clone(),
            UserContent::Blocks(blocks) => JsString::join(
                blocks.iter().filter_map(|block| match block {
                    pi_ai::types::TextOrImageContent::Text(text) => Some(&text.text),
                    pi_ai::types::TextOrImageContent::Image(_) => None,
                }),
                " ",
            ),
        },
        AgentMessage::Assistant(assistant) => JsString::join(
            assistant.content.iter().filter_map(|block| match block {
                AssistantContent::Text(text) => Some(&text.text),
                _ => None,
            }),
            " ",
        ),
        AgentMessage::ToolResult(result) => JsString::join(
            result.content.iter().filter_map(|block| match block {
                pi_ai::types::TextOrImageContent::Text(text) => Some(&text.text),
                pi_ai::types::TextOrImageContent::Image(_) => None,
            }),
            " ",
        ),
        AgentMessage::Custom(object) => object
            .get("summary")
            .or_else(|| object.get("content"))
            .and_then(JsValue::as_string)
            .cloned()
            .unwrap_or_default(),
    });
    JsString::join(texts, "\n")
}

#[test]
fn calculates_total_context_tokens_from_usage() {
    assert_eq!(calculate_context_tokens(&usage(1000.0, 500.0, 200.0, 100.0)), 1800.0);
    assert_eq!(calculate_context_tokens(&usage(0.0, 0.0, 0.0, 0.0)), 0.0);
}

#[test]
fn finds_last_non_aborted_assistant_usage() {
    let mut builder = EntryBuilder::new();
    let mut aborted = default_assistant("aborted");
    if let AgentMessage::Assistant(message) = &mut aborted {
        message.stop_reason = StopReason::Aborted;
    }
    let entries = vec![
        builder.message(&user("Hello")),
        builder.message(&default_assistant("Hi")),
        builder.message(&user("How are you?")),
        builder.message(&aborted),
    ];
    let found = get_last_assistant_usage(&entries).expect("usage must be found");
    assert_eq!(found.input, 100.0);
}

#[test]
fn skips_all_zero_assistant_usage() {
    let mut builder = EntryBuilder::new();
    let entries = vec![
        builder.message(&user("Hello")),
        builder.message(&default_assistant("Hi")),
        builder.message(&user("continue")),
        builder.message(&assistant("Partial", usage(0.0, 0.0, 0.0, 0.0))),
    ];
    let found = get_last_assistant_usage(&entries).expect("usage must be found");
    assert_eq!(found.input, 100.0);
}

#[test]
fn returns_no_usage_without_assistant_messages() {
    let mut builder = EntryBuilder::new();
    let entries = vec![builder.message(&user("Hello"))];
    assert!(get_last_assistant_usage(&entries).is_none());
}

#[test]
fn estimates_context_from_last_non_zero_usage_anchor() {
    let messages = vec![
        user("Hello"),
        default_assistant("Hi"),
        user("continue"),
        assistant("Partial thinking", usage(0.0, 0.0, 0.0, 0.0)),
    ];
    let estimate = estimate_context_tokens(&messages);
    assert_eq!(estimate.usage_tokens, 150.0);
    assert_eq!(estimate.last_usage_index, Some(1.0));
    assert!(estimate.trailing_tokens > 0.0);
    assert_eq!(estimate.tokens, 150.0 + estimate.trailing_tokens);
}

#[test]
fn estimates_message_tokens_with_chars_over_four() {
    assert_eq!(estimate_tokens(&user("12345678")), 2.0);
    assert_eq!(estimate_tokens(&user("123456789")), 3.0);
    assert_eq!(estimate_tokens(&default_assistant("12345678")), 2.0);
}

#[test]
fn decides_compaction_from_settings_and_explicit_threshold() {
    let settings = CompactionSettings {
        threshold_percent: None,
        enabled: true,
        reserve_tokens: 10_000.0,
        keep_recent_tokens: 20_000.0,
    };
    assert!(should_compact(95_000.0, 100_000.0, &settings, None).unwrap());
    assert!(!should_compact(89_000.0, 100_000.0, &settings, None).unwrap());
    assert!(!should_compact(271_999.0, 372_000.0, &settings, Some(272_000.0)).unwrap());
    assert!(should_compact(272_000.0, 372_000.0, &settings, Some(272_000.0)).unwrap());
    let disabled = CompactionSettings {
        enabled: false,
        ..settings
    };
    assert!(!should_compact(95_000.0, 100_000.0, &disabled, None).unwrap());
}

#[test]
fn finds_cut_point_at_a_valid_message() {
    let mut builder = EntryBuilder::new();
    let mut entries = Vec::new();
    for index in 0..10 {
        entries.push(builder.message(&user(&format!("User {index}"))));
        entries.push(builder.message(&assistant(
            &format!("Assistant {index}"),
            usage(0.0, 100.0, (index + 1) as f64 * 1000.0, 0.0),
        )));
    }
    let result = find_cut_point(&entries, 0, entries.len(), 2500.0);
    let kept = &entries[result.first_kept_entry_index];
    assert_eq!(kept.entry_type(), "message");
    let role = kept.as_message().expect("message entry").message.role();
    assert!(role == "user" || role == "assistant");
}

#[test]
fn returns_start_index_without_valid_cut_points() {
    let mut builder = EntryBuilder::new();
    let entries = vec![builder.message(&default_assistant("a"))];
    let result = find_cut_point(&entries, 0, entries.len(), 1000.0);
    assert_eq!(result.first_kept_entry_index, 0);
}

#[test]
fn retains_shorter_protected_suffix_when_all_messages_fit_budget() {
    let mut builder = EntryBuilder::new();
    let entries = vec![
        builder.message(&user("1")),
        builder.message(&assistant("a", usage(0.0, 50.0, 500.0, 0.0))),
        builder.message(&user("2")),
        builder.message(&assistant("b", usage(0.0, 50.0, 1000.0, 0.0))),
    ];
    let result = find_cut_point(&entries, 0, entries.len(), 50_000.0);
    assert_eq!(result.first_kept_entry_index, 1);
}

#[test]
fn retains_latest_user_when_active_assistant_suffix_exceeds_token_cap() {
    let mut builder = EntryBuilder::new();
    let old_user = builder.message(&user("old user"));
    let old_assistant = builder.message(&default_assistant("old assistant"));
    let latest_user = builder.message(&user("latest user must stay verbatim"));
    let huge = builder.message(&default_assistant(&"x".repeat(40_000)));
    let entries = vec![old_user, old_assistant, latest_user.clone(), huge];

    let result = find_cut_point(&entries, 0, entries.len(), 20_000.0);
    assert_eq!(result.first_kept_entry_index, 2);
    assert_eq!(
        entry_id(&entries[result.first_kept_entry_index]),
        entry_id(&latest_user)
    );
    assert!(!result.is_split_turn);
}

#[test]
fn retains_latest_user_when_active_tool_suffix_exceeds_token_cap() {
    let mut builder = EntryBuilder::new();
    let old_user = builder.message(&user("old user"));
    let old_assistant = builder.message(&default_assistant("old assistant"));
    let latest_user = builder.message(&user("latest user before tool result"));
    let tool_call = builder.message(&AgentMessage::Assistant(Box::new(AssistantMessage {
        content: vec![AssistantContent::ToolCall(pi_ai::types::ToolCall::new(
            "tool-1",
            "read",
            (json!({ "path": "large.txt" }).as_object().expect("object").clone())
                .into_iter()
                .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
                .collect::<pi_js_compat::js_json::JsObject>(),
        ))],
        ..match default_assistant("calling tool") {
            AgentMessage::Assistant(message) => *message,
            _ => unreachable!(),
        }
    })));
    let tool_result = builder.message(&AgentMessage::ToolResult(pi_ai::types::ToolResultMessage {
        role: pi_ai::types::ToolResultRole::Tag,
        tool_call_id: "tool-1".to_owned(),
        tool_name: "read".to_owned(),
        content: vec![pi_ai::types::ToolResultContent::Text(TextContent::new(
            "x".repeat(40_000),
        ))],
        details: None,
        is_error: false,
        timestamp: 0.0,
    }));
    let entries = vec![old_user, old_assistant, latest_user.clone(), tool_call, tool_result];

    let result = find_cut_point(&entries, 0, entries.len(), 20_000.0);
    assert_eq!(result.first_kept_entry_index, 2);
    assert_eq!(
        entry_id(&entries[result.first_kept_entry_index]),
        entry_id(&latest_user)
    );
}

#[test]
fn prepares_compaction_without_summarizing_latest_active_user_turn_start() {
    let mut builder = EntryBuilder::new();
    let old_user = builder.message(&user("old user"));
    let old_assistant = builder.message(&default_assistant("old assistant"));
    let latest_user = builder.message(&user("latest user must stay verbatim"));
    let huge = builder.message(&default_assistant(&"x".repeat(40_000)));
    let entries = vec![old_user, old_assistant, latest_user.clone(), huge];

    let preparation = prepare_compaction(&entries, DEFAULT_COMPACTION_SETTINGS).expect("compaction prepared");
    assert_eq!(&preparation.first_kept_entry_id, entry_id(&latest_user));
    assert!(!message_text(&preparation.messages_to_summarize).contains("latest user must stay verbatim"));
    assert!(preparation.turn_prefix_messages.is_empty());
}

#[test]
fn splits_boundary_start_turn_when_only_non_context_entries_precede_it() {
    let mut builder = EntryBuilder::new();
    let model_change = builder.model_change("openai", "gpt-5.6-sol");
    let active_user = builder.message(&user("one cumulative turn"));
    let mut entries = vec![model_change, active_user];
    for index in 0..4 {
        entries.push(builder.message(&default_assistant(&format!(
            "small result {index} {}",
            "x".repeat(4_000)
        ))));
    }

    let preparation = prepare_compaction(&entries, DEFAULT_COMPACTION_SETTINGS).expect("compaction prepared");
    assert!(preparation.is_split_turn);
}

#[test]
fn does_not_compact_short_single_active_turn() {
    let mut builder = EntryBuilder::new();
    let entries = vec![
        builder.message(&user("short request")),
        builder.message(&default_assistant("short response")),
    ];
    assert!(prepare_compaction(&entries, DEFAULT_COMPACTION_SETTINGS).is_none());
}

#[test]
fn excludes_goal_reminders_but_preserves_other_extension_messages() {
    let mut builder = EntryBuilder::new();
    let entries = vec![
        builder.message(&extension_user(
            "Work toward this objective until it is achieved: fix compaction",
        )),
        builder.message(&default_assistant("working")),
        builder.message(&extension_user("Extension context that must remain")),
        builder.message(&default_assistant("still working")),
        builder.message(&extension_user(
            "Continue working toward this objective until it is achieved: fix compaction",
        )),
        builder.message(&default_assistant(&"x".repeat(40_000))),
    ];

    let preparation = prepare_compaction(&entries, DEFAULT_COMPACTION_SETTINGS).expect("compaction prepared");
    let mut messages = preparation.messages_to_summarize.clone();
    messages.extend(preparation.turn_prefix_messages.clone());
    let text = message_text(&messages);
    assert!(text.contains("Extension context that must remain"));
    assert!(!text.contains("Work toward this objective until it is achieved:"));
    assert!(!text.contains("Continue working toward this objective until it is achieved:"));
}

#[test]
fn indicates_split_turn_when_cutting_at_assistant_message() {
    let mut builder = EntryBuilder::new();
    let mut entries = Vec::new();
    for index in 0..3 {
        entries.push(builder.message(&user(&format!("Turn {index}"))));
        entries.push(builder.message(&default_assistant(&format!("Response {index}"))));
    }
    let result = find_cut_point(&entries, 0, entries.len(), 1.0);
    if result.first_kept_entry_index == 3 {
        assert!(result.is_split_turn);
        assert_eq!(result.turn_start_index, 2);
    }
}

#[test]
fn protected_suffix_ignores_old_budget_for_context_visible_custom_message_entries() {
    let mut builder = EntryBuilder::new();
    let entries = vec![
        builder.message(&user("hi")),
        builder.message(&default_assistant("hello")),
        builder.custom_message(&"x".repeat(4000)),
        builder.message(&default_assistant("ok")),
    ];

    let tiny = find_cut_point(&entries, 0, entries.len(), 1.0);
    assert_eq!(tiny.first_kept_entry_index, 1);
    assert!(tiny.is_split_turn);
    assert_eq!(tiny.turn_start_index, 0);

    let fits = find_cut_point(&entries, 0, entries.len(), 2.0);
    assert_eq!(fits.first_kept_entry_index, 1);
    assert!(fits.is_split_turn);
    assert_eq!(fits.turn_start_index, 0);
    assert_eq!(tiny, fits);
}

#[test]
fn finds_turn_start_index_and_returns_minus_one_when_missing() {
    let mut builder = EntryBuilder::new();
    let entries = vec![
        builder.message(&user("turn")),
        builder.message(&default_assistant("response")),
        builder.model_change("openai", "gpt-4"),
    ];
    assert_eq!(find_turn_start_index(&entries, 2, 0), 0);
    assert_eq!(find_turn_start_index(&entries, 0, 1), -1);
}

#[test]
fn loads_all_messages_when_no_compaction() {
    let mut builder = EntryBuilder::new();
    let entries = vec![
        builder.message(&user("1")),
        builder.message(&default_assistant("a")),
        builder.message(&user("2")),
        builder.message(&default_assistant("b")),
    ];
    let loaded = build_session_context(&entries, LeafSelection::Last);
    assert_eq!(loaded.messages.len(), 4);
    assert_eq!(loaded.thinking_level, "off");
    assert_eq!(
        loaded.model,
        Some(pi_coding_agent::core::session_manager::SessionModel {
            provider: JsString::from("anthropic"),
            model_id: JsString::from("claude-sonnet-4-5"),
        })
    );
}

#[test]
fn handles_single_compaction_context() {
    let mut builder = EntryBuilder::new();
    let u1 = builder.message(&user("1"));
    let a1 = builder.message(&default_assistant("a"));
    let u2 = builder.message(&user("2"));
    let a2 = builder.message(&default_assistant("b"));
    let compaction = builder.compaction("Summary of 1,a,2,b", entry_id(&u2));
    let u3 = builder.message(&user("3"));
    let a3 = builder.message(&default_assistant("c"));
    let entries = vec![u1, a1, u2, a2, compaction, u3, a3];

    let loaded = build_session_context(&entries, LeafSelection::Last);
    assert_eq!(loaded.messages.len(), 5);
    assert_eq!(loaded.messages[0].role(), "compactionSummary");
}

#[test]
fn handles_multiple_compactions_using_latest() {
    let mut builder = EntryBuilder::new();
    let u1 = builder.message(&user("1"));
    let a1 = builder.message(&default_assistant("a"));
    let compact1 = builder.compaction("First summary", entry_id(&u1));
    let u2 = builder.message(&user("2"));
    let b = builder.message(&default_assistant("b"));
    let u3 = builder.message(&user("3"));
    let c = builder.message(&default_assistant("c"));
    let compact2 = builder.compaction("Second summary", entry_id(&u3));
    let u4 = builder.message(&user("4"));
    let d = builder.message(&default_assistant("d"));
    let entries = vec![u1, a1, compact1, u2, b, u3, c, compact2, u4, d];

    let loaded = build_session_context(&entries, LeafSelection::Last);
    assert_eq!(loaded.messages.len(), 5);
    let AgentMessage::Custom(object) = &loaded.messages[0] else {
        panic!("summary message must be a custom message");
    };
    let summary = object
        .get("summary")
        .and_then(JsValue::as_string)
        .expect("summary field");
    assert!(summary.contains("Second summary"));
}

#[test]
fn keeps_all_messages_when_first_kept_entry_is_first_entry() {
    let mut builder = EntryBuilder::new();
    let u1 = builder.message(&user("1"));
    let a1 = builder.message(&default_assistant("a"));
    let compact1 = builder.compaction("First summary", entry_id(&u1));
    let u2 = builder.message(&user("2"));
    let b = builder.message(&default_assistant("b"));
    let entries = vec![u1, a1, compact1, u2, b];

    let loaded = build_session_context(&entries, LeafSelection::Last);
    assert_eq!(loaded.messages.len(), 5);
}

#[test]
fn caps_kept_pre_compaction_context_to_20k_bytes() {
    let mut builder = EntryBuilder::new();
    let older = builder.message(&default_assistant(&format!("older output {}", "x".repeat(80_000))));
    let latest = builder.message(&default_assistant(&format!("latest output {}", "y".repeat(80_000))));
    let compaction = builder.compaction("summary", entry_id(&older));
    let after = builder.message(&user("continue"));
    let entries = vec![older, latest, compaction, after];

    let loaded = build_session_context(&entries, LeafSelection::Last);
    let kept = &loaded.messages[1..loaded.messages.len() - 1];
    let bytes = serde_json::to_vec(kept).expect("kept messages serialize");
    assert!(bytes.len() <= 20_000, "kept {} bytes", bytes.len());
    let text = String::from_utf8(bytes).expect("json is utf-8");
    assert!(text.contains("latest output"));
    assert!(!text.contains("older output"));
}

#[test]
fn tracks_model_and_thinking_level_changes() {
    let mut builder = EntryBuilder::new();
    let entries = vec![
        builder.message(&user("1")),
        builder.model_change("openai", "gpt-4"),
        builder.message(&default_assistant("a")),
        builder.thinking_level("high"),
    ];
    let loaded = build_session_context(&entries, LeafSelection::Last);
    assert_eq!(
        loaded.model,
        Some(pi_coding_agent::core::session_manager::SessionModel {
            provider: JsString::from("anthropic"),
            model_id: JsString::from("claude-sonnet-4-5"),
        })
    );
    assert_eq!(loaded.thinking_level, "high");
}

#[test]
fn prepares_repeated_compactions_past_previously_kept_messages() {
    let mut builder = EntryBuilder::new();
    let u1 = builder.message(&user("user msg 1 (summarized by compaction1)"));
    let a1 = builder.message(&default_assistant("assistant msg 1"));
    let u2 = builder.message(&user("user msg 2 - kept by compaction1"));
    let a2 = builder.message(&default_assistant("assistant msg 2"));
    let u3 = builder.message(&user("user msg 3 - kept by compaction1"));
    let a3 = builder.message(&assistant("assistant msg 3", usage(5000.0, 1000.0, 0.0, 0.0)));
    let compaction1 = builder.compaction("First summary", entry_id(&u2));
    let u4 = builder.message(&user("user msg 4 (new after compaction1)"));
    let a4 = builder.message(&assistant("assistant msg 4", usage(8000.0, 2000.0, 0.0, 0.0)));
    let entries = vec![u1, a1, u2, a2, u3, a3, compaction1, u4, a4];

    let preparation = prepare_compaction(&entries, DEFAULT_COMPACTION_SETTINGS).expect("compaction prepared");
    assert_eq!(
        preparation.previous_summary.as_ref(),
        Some(&JsString::from("First summary"))
    );
    assert!(message_text(&preparation.messages_to_summarize).contains("user msg 2 - kept by compaction1"));
}

#[test]
fn re_summarizes_previously_kept_messages_when_recent_window_moves_past_them() {
    let mut builder = EntryBuilder::new();
    let u1 = builder.message(&user(&"user msg 1 (summarized by compaction1)".repeat(4)));
    let a1 = builder.message(&default_assistant(&"assistant msg 1".repeat(4)));
    let u2 = builder.message(&user(&"user msg 2 - kept by compaction1 ".repeat(12)));
    let a2 = builder.message(&default_assistant(&"assistant msg 2 ".repeat(12)));
    let u3 = builder.message(&user(&"user msg 3 - kept by compaction1 ".repeat(12)));
    let a3 = builder.message(&assistant(
        &"assistant msg 3 ".repeat(12),
        usage(5000.0, 1000.0, 0.0, 0.0),
    ));
    let compaction1 = builder.compaction("First summary", entry_id(&u2));
    let u4 = builder.message(&user(&"user msg 4 (new after compaction1) ".repeat(12)));
    let a4 = builder.message(&assistant(
        &"assistant msg 4 ".repeat(12),
        usage(8000.0, 2000.0, 0.0, 0.0),
    ));
    let entries = vec![u1, a1, u2, a2, u3, a3, compaction1, u4, a4];
    let settings = CompactionSettings {
        keep_recent_tokens: 100.0,
        ..DEFAULT_COMPACTION_SETTINGS
    };

    let preparation = prepare_compaction(&entries, settings).expect("compaction prepared");
    let summarized = message_text(&preparation.messages_to_summarize);
    assert!(summarized.contains("user msg 2 - kept by compaction1"));
    assert!(summarized.contains("user msg 3 - kept by compaction1"));
    assert!(!summarized.contains("First summary"));
    assert_eq!(
        preparation.previous_summary.as_ref(),
        Some(&JsString::from("First summary"))
    );
}

#[test]
fn finds_cut_point_in_large_session_fixture() {
    let content = include_str!("fixtures/large-session.jsonl");
    let mut parsed = parse_session_entries(content);
    migrate_session_entries(&mut parsed).unwrap();
    let entries: Vec<SessionEntry> = parsed
        .into_iter()
        .filter_map(|entry| match entry {
            pi_coding_agent::core::session_manager::FileEntry::Entry(entry) => Some(entry),
            pi_coding_agent::core::session_manager::FileEntry::Header(_)
            | pi_coding_agent::core::session_manager::FileEntry::Json(_) => None,
        })
        .collect();
    assert!(entries.len() > 100);
    let message_count = entries.iter().filter(|entry| entry.entry_type() == "message").count();
    assert!(message_count > 100);

    let result = find_cut_point(
        &entries,
        0,
        entries.len(),
        DEFAULT_COMPACTION_SETTINGS.keep_recent_tokens,
    );
    assert!(result.first_kept_entry_index < entries.len());
    assert_eq!(entries[result.first_kept_entry_index].entry_type(), "message");
}
