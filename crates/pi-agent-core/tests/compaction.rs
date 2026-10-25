use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use pi_agent_core::agent::AgentMessage;
use pi_agent_core::harness::compaction::{
    CompactOptions, CompactionDetails, CompactionPreparation, CompactionSettings, ContextUsageEstimate, CutPointResult,
    DEFAULT_COMPACTION_SETTINGS, GenerateSummaryOptions, calculate_context_tokens, compact, estimate_context_tokens,
    estimate_tokens, find_cut_point, find_turn_start_index, generate_summary, get_last_assistant_usage,
    prepare_compaction, should_compact,
};
use pi_agent_core::harness::session::{
    KnownSessionEntry, SessionTreeEntry as Entry, SessionTreeEntryBase, build_session_context,
};
use pi_agent_core::harness::types::ThinkingLevel;
use pi_ai::auth::resolve::ModelsError;
use pi_ai::auth::types::AuthResult;
use pi_ai::models::{CreateModelsOptions, Models, MutableModels, create_models};
use pi_ai::providers::faux::{
    FauxAssistantMessageOptions, FauxModelDefinition, FauxProviderHandle, FauxProviderStateSnapshot, FauxResponseStep,
    RegisterFauxProviderOptions, faux_assistant_message, faux_provider,
};
use pi_ai::request::{BoxFuture, SimpleStreamOptions, StreamOptions};
use pi_ai::types::{
    AssistantContent, AssistantMessage, CacheRetention, Context, Message, Model, StopReason, TextContent,
    TextOrImageContent, ThinkingContent, ToolCall, ToolResultMessage, ToolResultRole, Usage, UsageCost, UserContent,
    UserMessage, UserRole,
};
use pi_ai::utils::event_stream::{AssistantMessageEventStream, StreamFailure};
use serde_json::{Map, Value, json};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

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

fn user_message(text: &str) -> AgentMessage {
    AgentMessage::User(UserMessage {
        role: UserRole::Tag,
        content: UserContent::Blocks(vec![TextOrImageContent::Text(TextContent::new(text))]),
        input_source: None,
        provider_native: None,
        timestamp: 1.0,
    })
}

fn assistant_message_with(content: Vec<AssistantContent>, usage: Usage) -> AgentMessage {
    let mut message = AssistantMessage::empty("anthropic-messages", "anthropic", "claude-sonnet-4-5", 1.0);
    message.content = content;
    message.usage = usage;
    message.stop_reason = StopReason::Stop;
    AgentMessage::Assistant(Box::new(message))
}

fn assistant_message(text: &str, usage: Usage) -> AgentMessage {
    assistant_message_with(vec![AssistantContent::Text(TextContent::new(text))], usage)
}

fn tool_call_message(name: &str, id: &str, arguments: Value, usage: Usage) -> AgentMessage {
    assistant_message_with(
        vec![AssistantContent::ToolCall(ToolCall::new(
            id,
            name,
            arguments
                .as_object()
                .cloned()
                .expect("arguments must be an object")
                .into_iter()
                .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
                .collect::<pi_js_compat::js_json::JsObject>(),
        ))],
        usage,
    )
}

fn tool_result_message(tool_call_id: &str, tool_name: &str, text: &str) -> AgentMessage {
    AgentMessage::ToolResult(ToolResultMessage {
        role: ToolResultRole::Tag,
        tool_call_id: tool_call_id.to_owned(),
        tool_name: tool_name.to_owned(),
        content: vec![TextOrImageContent::Text(TextContent::new(text))],
        details: None,
        is_error: false,
        timestamp: 1.0,
    })
}

struct EntryFactory {
    next_id: i64,
}

impl EntryFactory {
    fn new() -> Self {
        Self { next_id: 0 }
    }

    fn base(&mut self, parent_id: Option<&JsString>) -> SessionTreeEntryBase {
        let id = format!("entry-{}", self.next_id).into();
        self.next_id += 1;
        SessionTreeEntryBase {
            id,
            parent_id: parent_id.cloned(),
            timestamp: "2026-10-22T00:00:00.000Z".into(),
        }
    }

    fn message(&mut self, message: AgentMessage, parent_id: Option<&JsString>) -> Entry {
        Entry::from_known(KnownSessionEntry::Message {
            base: self.base(parent_id),
            message,
        })
    }

    fn compaction(&mut self, summary: &str, parent_id: Option<&JsString>, first_kept_entry_id: &JsString) -> Entry {
        Entry::from_known(KnownSessionEntry::Compaction {
            base: self.base(parent_id),
            summary: summary.into(),
            first_kept_entry_id: first_kept_entry_id.clone(),
            tokens_before: 1234.0,
            duration_ms: None,
            details: None,
            from_hook: None,
        })
    }

    fn thinking_level(&mut self, level: &str, parent_id: Option<&JsString>) -> Entry {
        Entry::from_known(KnownSessionEntry::ThinkingLevelChange {
            base: self.base(parent_id),
            thinking_level: level.into(),
        })
    }

    fn model_change(&mut self, provider: &str, model_id: &str, parent_id: Option<&JsString>) -> Entry {
        Entry::from_known(KnownSessionEntry::ModelChange {
            base: self.base(parent_id),
            provider: provider.into(),
            model_id: model_id.into(),
        })
    }

    fn branch_summary(&mut self, from_id: &str, summary: &str, parent_id: Option<&JsString>) -> Entry {
        Entry::from_known(KnownSessionEntry::BranchSummary {
            base: self.base(parent_id),
            from_id: from_id.into(),
            summary: summary.into(),
            details: None,
            from_hook: None,
        })
    }
}

fn settings(reserve_tokens: f64, keep_recent_tokens: f64) -> CompactionSettings {
    CompactionSettings {
        enabled: true,
        reserve_tokens,
        keep_recent_tokens,
    }
}

fn create_repeated_tool_cycle_entries(factory: &mut EntryFactory, cycle_count: usize, result_text: &str) -> Vec<Entry> {
    let mut entries = Vec::new();
    let user = factory.message(user_message("continue the work"), None);
    let mut parent_id = user.id().to_owned();
    entries.push(user);

    for cycle in 0..cycle_count {
        let tool_call_id = format!("tool-{cycle}");
        let assistant = factory.message(
            tool_call_message(
                "ordinary_tool",
                &tool_call_id,
                json!({"cycle": cycle}),
                usage(100.0, 50.0, 0.0, 0.0),
            ),
            Some(&parent_id),
        );
        parent_id = assistant.id().to_owned();
        entries.push(assistant);
        let tool_result = factory.message(
            tool_result_message(&tool_call_id, "ordinary_tool", result_text),
            Some(&parent_id),
        );
        parent_id = tool_result.id().to_owned();
        entries.push(tool_result);
    }

    entries
}

fn expect_kept_tool_results_follow_calls(entries: &[Entry], first_kept_index: usize, expected_result_text: &str) {
    let mut kept_tool_call_ids = std::collections::BTreeSet::new();
    for entry in &entries[first_kept_index..] {
        let Some(message) = entry.message() else {
            continue;
        };
        match message {
            AgentMessage::Assistant(assistant) => {
                for block in &assistant.content {
                    if let AssistantContent::ToolCall(call) = block {
                        kept_tool_call_ids.insert(call.id.clone());
                    }
                }
            }
            AgentMessage::ToolResult(result) => {
                assert!(
                    kept_tool_call_ids.contains(&result.tool_call_id),
                    "kept tool result {} must follow its tool call",
                    result.tool_call_id
                );
                assert_eq!(
                    result.content,
                    vec![TextOrImageContent::Text(TextContent::new(expected_result_text))]
                );
            }
            _ => {}
        }
    }
}

fn role(message: &AgentMessage) -> JsString {
    message.role()
}

// ---------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------

#[test]
fn calculates_total_context_tokens_from_usage() {
    assert_eq!(calculate_context_tokens(&usage(1000.0, 500.0, 200.0, 100.0)), 1800.0);
    assert_eq!(calculate_context_tokens(&usage(0.0, 0.0, 0.0, 0.0)), 0.0);
}

#[test]
fn checks_compaction_threshold() {
    let configured = settings(10000.0, 20000.0);
    assert!(should_compact(95000.0, 100000.0, &configured));
    assert!(!should_compact(89000.0, 100000.0, &configured));
    assert!(!should_compact(
        95000.0,
        100000.0,
        &CompactionSettings {
            enabled: false,
            ..configured
        }
    ));
}

#[test]
fn finds_a_cut_point_based_on_token_differences() {
    let mut factory = EntryFactory::new();
    let mut entries = Vec::new();
    let mut parent_id: Option<JsString> = None;
    for index in 0..10 {
        let user = factory.message(user_message(&format!("User {index}")), parent_id.as_ref());
        parent_id = Some(user.id().to_owned());
        entries.push(user);
        let assistant = factory.message(
            assistant_message(
                &format!("Assistant {index}"),
                usage(0.0, 100.0, ((index + 1) as f64) * 1000.0, 0.0),
            ),
            parent_id.as_ref(),
        );
        parent_id = Some(assistant.id().to_owned());
        entries.push(assistant);
    }

    let result = find_cut_point(&entries, 0.0, entries.len() as f64, 2500.0).unwrap();
    assert_eq!(entries[result.first_kept_entry_index as usize].kind(), "message");
}

#[test]
fn limits_the_retained_suffix_to_the_last_three_context_messages() {
    let mut factory = EntryFactory::new();
    let entries = vec![
        factory.message(user_message("old user"), None),
        factory.message(assistant_message("old assistant", usage(100.0, 50.0, 0.0, 0.0)), None),
        factory.message(user_message("recent user 1"), None),
        factory.message(
            assistant_message("recent assistant 1", usage(100.0, 50.0, 0.0, 0.0)),
            None,
        ),
        factory.message(user_message("recent user 2"), None),
        factory.message(
            assistant_message("recent assistant 2", usage(100.0, 50.0, 0.0, 0.0)),
            None,
        ),
    ];

    let result = find_cut_point(&entries, 0.0, entries.len() as f64, 20_000.0).unwrap();
    assert_eq!(result.first_kept_entry_index, 3.0);
}

#[test]
fn uses_the_ten_thousand_token_suffix_when_the_last_three_context_messages_are_larger() {
    let huge_text = "x".repeat(40_000);
    let mut factory = EntryFactory::new();
    let entries = vec![
        factory.message(user_message("old user"), None),
        factory.message(assistant_message("old assistant", usage(100.0, 50.0, 0.0, 0.0)), None),
        factory.message(user_message(&huge_text), None),
        factory.message(assistant_message(&huge_text, usage(100.0, 50.0, 0.0, 0.0)), None),
        factory.message(user_message("latest user"), None),
    ];

    let result = find_cut_point(&entries, 0.0, entries.len() as f64, 20_000.0).unwrap();
    assert_eq!(result.first_kept_entry_index, 4.0);
}

#[test]
fn retains_latest_user_verbatim_when_active_assistant_suffix_exceeds_ten_thousand_tokens() {
    let huge_text = "x".repeat(40_000);
    let mut factory = EntryFactory::new();
    let old_user = factory.message(user_message("old user"), None);
    let old_assistant = factory.message(
        assistant_message("old assistant", usage(100.0, 50.0, 0.0, 0.0)),
        Some(old_user.id()),
    );
    let latest_user = factory.message(user_message("latest user must stay verbatim"), Some(old_assistant.id()));
    let assistant = factory.message(
        assistant_message(&huge_text, usage(100.0, 50.0, 0.0, 0.0)),
        Some(latest_user.id()),
    );
    let entries = vec![old_user, old_assistant, latest_user, assistant];

    let result = find_cut_point(&entries, 0.0, entries.len() as f64, 20_000.0).unwrap();
    assert_eq!(result.first_kept_entry_index, 2.0);
    assert_eq!(entries[2].id(), entries[2].id());
    assert_eq!(
        role(entries[result.first_kept_entry_index as usize].message().unwrap()),
        "user"
    );
    assert!(!result.is_split_turn);
}

#[test]
fn retains_latest_user_verbatim_when_active_tool_suffix_exceeds_ten_thousand_tokens() {
    let mut factory = EntryFactory::new();
    let old_user = factory.message(user_message("old user"), None);
    let old_assistant = factory.message(
        assistant_message("old assistant", usage(100.0, 50.0, 0.0, 0.0)),
        Some(old_user.id()),
    );
    let latest_user = factory.message(user_message("latest user before tool result"), Some(old_assistant.id()));
    let assistant = factory.message(
        tool_call_message(
            "read",
            "tool-1",
            json!({"path": "large.txt"}),
            usage(100.0, 50.0, 0.0, 0.0),
        ),
        Some(latest_user.id()),
    );
    let tool_result = factory.message(
        tool_result_message("tool-1", "read", &"x".repeat(40_000)),
        Some(assistant.id()),
    );
    let entries = vec![old_user, old_assistant, latest_user, assistant, tool_result];

    let result = find_cut_point(&entries, 0.0, entries.len() as f64, 20_000.0).unwrap();
    assert_eq!(result.first_kept_entry_index, 2.0);
    assert_eq!(role(entries[2].message().unwrap()), "user");
}

#[test]
fn prepares_compaction_without_summarizing_the_latest_active_user_turn_start() {
    let mut factory = EntryFactory::new();
    let old_user = factory.message(user_message("old user"), None);
    let old_assistant = factory.message(
        assistant_message("old assistant", usage(100.0, 50.0, 0.0, 0.0)),
        Some(old_user.id()),
    );
    let latest_user = factory.message(user_message("latest user must stay verbatim"), Some(old_assistant.id()));
    let assistant = factory.message(
        assistant_message(&"x".repeat(40_000), usage(100.0, 50.0, 0.0, 0.0)),
        Some(latest_user.id()),
    );
    let preparation = prepare_compaction(
        &[old_user, old_assistant, latest_user.clone(), assistant],
        &DEFAULT_COMPACTION_SETTINGS,
    )
    .expect("preparation must not fail")
    .expect("preparation must be defined");

    assert!(
        !preparation
            .messages_to_summarize
            .contains(latest_user.message().unwrap())
    );
    assert!(preparation.turn_prefix_messages.is_empty());
    assert_eq!(preparation.first_kept_entry_id, *latest_user.id());
}

#[test]
fn prepares_a_split_turn_for_repeated_ordinary_sized_tool_cycles() {
    let result_text = "x".repeat(4_000);
    let mut factory = EntryFactory::new();
    let entries = create_repeated_tool_cycle_entries(&mut factory, 24, &result_text);
    let preparation = prepare_compaction(&entries, &DEFAULT_COMPACTION_SETTINGS)
        .expect("preparation must not fail")
        .expect("preparation must be defined");

    assert!(preparation.is_split_turn);
    assert_eq!(role(&preparation.turn_prefix_messages[0]), "user");
    assert!(preparation.turn_prefix_messages.len() > 1);

    let first_kept = entries
        .iter()
        .position(|entry| entry.id() == &preparation.first_kept_entry_id)
        .expect("retained tail must come from an entry");
    assert!(first_kept > 0);
    assert_eq!(role(entries[first_kept].message().unwrap()), "assistant");
    expect_kept_tool_results_follow_calls(&entries, first_kept, &result_text);
}

#[test]
fn does_not_prepare_a_short_single_active_turn() {
    let mut factory = EntryFactory::new();
    let user = factory.message(user_message("short turn"), None);
    let assistant = factory.message(
        tool_call_message("ordinary_tool", "short-tool", json!({}), usage(100.0, 50.0, 0.0, 0.0)),
        Some(user.id()),
    );
    let tool_result = factory.message(
        tool_result_message("short-tool", "ordinary_tool", "short result"),
        Some(assistant.id()),
    );

    assert!(
        prepare_compaction(&[user, assistant, tool_result], &DEFAULT_COMPACTION_SETTINGS)
            .expect("preparation must not fail")
            .is_none()
    );
}

#[test]
fn covers_cut_point_and_turn_start_edge_cases() {
    let mut factory = EntryFactory::new();
    let thinking = factory.thinking_level("high", None);
    let model_change = factory.model_change("openai", "gpt-4", Some(thinking.id()));
    assert_eq!(
        find_cut_point(&[thinking.clone(), model_change.clone()], 0.0, 2.0, 1.0).unwrap(),
        CutPointResult {
            first_kept_entry_index: 0.0,
            turn_start_index: -1.0,
            is_split_turn: false,
        }
    );

    let branch_summary = factory.branch_summary("branch", "branch summary", Some(model_change.id()));
    assert_eq!(
        find_turn_start_index(&[thinking.clone(), branch_summary.clone()], 1.0, 0.0).unwrap(),
        1.0
    );

    let custom_message = factory.message(
        AgentMessage::Custom(
            (Map::from_iter([
                ("role".to_owned(), json!("custom")),
                ("customType".to_owned(), json!("note")),
                ("content".to_owned(), json!("custom content")),
                ("display".to_owned(), json!(true)),
                ("timestamp".to_owned(), json!(1)),
            ]))
            .into_iter()
            .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
            .collect::<pi_js_compat::js_json::JsObject>(),
        ),
        Some(branch_summary.id()),
    );
    assert_eq!(
        find_turn_start_index(&[thinking.clone(), custom_message], 1.0, 0.0).unwrap(),
        1.0
    );
    assert_eq!(
        find_turn_start_index(&[thinking.clone(), model_change], 1.0, 0.0).unwrap(),
        -1.0
    );

    let result = find_cut_point(&[thinking, branch_summary], 0.0, 2.0, 1.0).unwrap();
    assert_eq!(result.first_kept_entry_index, 0.0);

    let tool_result = factory.message(tool_result_message("call-1", "read", "tool output"), None);
    assert_eq!(
        find_cut_point(&[tool_result], 0.0, 1.0, 1.0).unwrap(),
        CutPointResult {
            first_kept_entry_index: 0.0,
            turn_start_index: -1.0,
            is_split_turn: false,
        }
    );

    let user = factory.message(user_message("user"), None);
    let compaction = factory.compaction("summary", Some(user.id()), &"missing".into());
    let assistant = factory.message(
        assistant_message("assistant", usage(100.0, 50.0, 0.0, 0.0)),
        Some(compaction.id()),
    );
    assert_eq!(
        find_cut_point(&[user, compaction, assistant], 0.0, 3.0, 1.0)
            .unwrap()
            .first_kept_entry_index,
        2.0
    );
}

#[test]
fn estimates_tokens_and_context_usage_across_supported_message_roles() {
    let mock_usage = usage(10.0, 5.0, 3.0, 2.0);
    let assistant = assistant_message("assistant", mock_usage.clone());
    let assistant_with_thinking_and_tool = assistant_message_with(
        vec![
            AssistantContent::Thinking(ThinkingContent::new("thinking")),
            AssistantContent::ToolCall(ToolCall::new(
                "call-1",
                "read",
                Map::from_iter([("path".to_owned(), json!("file.ts"))])
                    .into_iter()
                    .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
                    .collect::<pi_js_compat::js_json::JsObject>(),
            )),
        ],
        mock_usage.clone(),
    );
    let custom_string = AgentMessage::Custom(
        (Map::from_iter([
            ("role".to_owned(), json!("custom")),
            ("customType".to_owned(), json!("note")),
            ("content".to_owned(), json!("custom text")),
            ("display".to_owned(), json!(true)),
            ("timestamp".to_owned(), json!(1)),
        ]))
        .into_iter()
        .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
        .collect::<pi_js_compat::js_json::JsObject>(),
    );
    let tool_result_with_image = AgentMessage::ToolResult(ToolResultMessage {
        role: ToolResultRole::Tag,
        tool_call_id: "call-1".to_owned(),
        tool_name: "read".to_owned(),
        content: vec![
            TextOrImageContent::Text(TextContent::new("tool text")),
            TextOrImageContent::Image(pi_ai::types::ImageContent::new("abc", "image/png")),
        ],
        details: None,
        is_error: false,
        timestamp: 1.0,
    });
    let bash_execution = AgentMessage::Custom(
        (Map::from_iter([
            ("role".to_owned(), json!("bashExecution")),
            ("command".to_owned(), json!("npm run check")),
            ("output".to_owned(), json!("ok")),
            ("exitCode".to_owned(), json!(0)),
            ("cancelled".to_owned(), json!(false)),
            ("truncated".to_owned(), json!(false)),
            ("timestamp".to_owned(), json!(1)),
        ]))
        .into_iter()
        .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
        .collect::<pi_js_compat::js_json::JsObject>(),
    );
    let branch_summary = AgentMessage::Custom(
        (Map::from_iter([
            ("role".to_owned(), json!("branchSummary")),
            ("summary".to_owned(), json!("branch")),
            ("fromId".to_owned(), json!("x")),
            ("timestamp".to_owned(), json!(1)),
        ]))
        .into_iter()
        .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
        .collect::<pi_js_compat::js_json::JsObject>(),
    );
    let compaction_summary = AgentMessage::Custom(
        (Map::from_iter([
            ("role".to_owned(), json!("compactionSummary")),
            ("summary".to_owned(), json!("compact")),
            ("tokensBefore".to_owned(), json!(123)),
            ("timestamp".to_owned(), json!(1)),
        ]))
        .into_iter()
        .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
        .collect::<pi_js_compat::js_json::JsObject>(),
    );

    assert_eq!(estimate_tokens(&user_message("plain user")), 3.0);
    assert_eq!(estimate_tokens(&assistant_with_thinking_and_tool), 8.0);
    assert_eq!(estimate_tokens(&custom_string), 3.0);
    assert_eq!(estimate_tokens(&tool_result_with_image), 1203.0);
    assert_eq!(estimate_tokens(&bash_execution), 4.0);
    assert_eq!(estimate_tokens(&branch_summary), 2.0);
    assert_eq!(estimate_tokens(&compaction_summary), 2.0);
    assert_eq!(
        estimate_tokens(&AgentMessage::Custom(
            (Map::from_iter([
                ("role".to_owned(), json!("unknown")),
                ("timestamp".to_owned(), json!(1)),
            ]))
            .into_iter()
            .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
            .collect::<pi_js_compat::js_json::JsObject>()
        )),
        0.0
    );

    let mut factory = EntryFactory::new();
    assert_eq!(
        get_last_assistant_usage(&[
            factory.message(user_message("user"), None),
            factory.message(assistant.clone(), None),
        ]),
        Some(&mock_usage)
    );

    let aborted = assistant_message_with(
        vec![AssistantContent::Text(TextContent::new("assistant"))],
        mock_usage.clone(),
    );
    let mut aborted_message = aborted.as_assistant().unwrap().clone();
    aborted_message.stop_reason = StopReason::Aborted;
    let mut error_message = aborted.as_assistant().unwrap().clone();
    error_message.stop_reason = StopReason::Error;
    assert_eq!(
        get_last_assistant_usage(&[
            factory.message(AgentMessage::Assistant(Box::new(aborted_message)), None),
            factory.message(AgentMessage::Assistant(Box::new(error_message)), None),
        ]),
        None
    );

    assert_eq!(
        get_last_assistant_usage(&[
            factory.message(user_message("user"), None),
            factory.message(assistant.clone(), None),
            factory.message(assistant_message("partial", usage(0.0, 0.0, 0.0, 0.0)), None),
        ]),
        Some(&mock_usage)
    );

    assert_eq!(
        estimate_context_tokens(&[user_message("no usage")]).last_usage_index,
        None
    );
    let estimate = estimate_context_tokens(&[assistant.clone(), user_message("tail")]);
    assert_eq!(estimate.usage_tokens, 20.0);
    assert_eq!(estimate.last_usage_index, Some(0.0));
    assert_eq!(estimate.tokens, 21.0);

    let estimate = estimate_context_tokens(&[
        user_message("Hello"),
        assistant,
        user_message("continue"),
        assistant_message("Partial thinking", usage(0.0, 0.0, 0.0, 0.0)),
    ]);
    assert_eq!(estimate.usage_tokens, 20.0);
    assert_eq!(estimate.last_usage_index, Some(1.0));
    assert_eq!(estimate.trailing_tokens, 6.0);
    assert_eq!(estimate.tokens, 26.0);
}

#[test]
fn builds_session_context_with_a_compaction_entry() {
    let mut factory = EntryFactory::new();
    let u1 = factory.message(user_message("1"), None);
    let a1 = factory.message(assistant_message("a", usage(100.0, 50.0, 0.0, 0.0)), Some(u1.id()));
    let u2 = factory.message(user_message("2"), Some(a1.id()));
    let a2 = factory.message(assistant_message("b", usage(100.0, 50.0, 0.0, 0.0)), Some(u2.id()));
    let compaction = factory.compaction("Summary of 1,a,2,b", Some(a2.id()), u2.id());
    let u3 = factory.message(user_message("3"), Some(compaction.id()));
    let a3 = factory.message(assistant_message("c", usage(100.0, 50.0, 0.0, 0.0)), Some(u3.id()));

    let loaded = build_session_context(&[u1, a1, u2, a2, compaction, u3, a3]).unwrap();
    assert_eq!(loaded.messages.len(), 5);
    assert_eq!(
        loaded.messages.iter().map(|message| message.role()).collect::<Vec<_>>(),
        ["compactionSummary", "user", "assistant", "user", "assistant"]
    );
}

#[test]
fn tracks_model_and_thinking_level_changes_in_built_context() {
    let mut factory = EntryFactory::new();
    let user = factory.message(user_message("1"), None);
    let model_change = factory.model_change("openai", "gpt-4", Some(user.id()));
    let assistant = factory.message(
        assistant_message("a", usage(100.0, 50.0, 0.0, 0.0)),
        Some(model_change.id()),
    );
    let thinking_change = factory.thinking_level("high", Some(assistant.id()));

    let loaded = build_session_context(&[user, model_change, assistant, thinking_change]).unwrap();
    assert_eq!(
        loaded.model,
        Some(pi_agent_core::harness::session::SessionModel {
            provider: "anthropic".into(),
            model_id: "claude-sonnet-4-5".into(),
        })
    );
    assert_eq!(loaded.thinking_level, "high");
}

#[test]
fn prepares_compaction_using_the_latest_compaction_summary_as_previous_summary() {
    let mut factory = EntryFactory::new();
    let u1 = factory.message(user_message("user msg 1"), None);
    let a1 = factory.message(
        assistant_message("assistant msg 1", usage(100.0, 50.0, 0.0, 0.0)),
        Some(u1.id()),
    );
    let u2 = factory.message(user_message("user msg 2"), Some(a1.id()));
    let a2 = factory.message(
        assistant_message("assistant msg 2", usage(5000.0, 1000.0, 0.0, 0.0)),
        Some(u2.id()),
    );
    let compaction1 = factory.compaction("First summary", Some(a2.id()), &"missing".into());
    let u3 = factory.message(user_message("user msg 3"), Some(compaction1.id()));
    let a3 = factory.message(
        assistant_message("assistant msg 3", usage(8000.0, 2000.0, 0.0, 0.0)),
        Some(u3.id()),
    );
    let u4 = factory.message(user_message("user msg 4"), Some(a3.id()));
    let a4 = factory.message(
        assistant_message("assistant msg 4", usage(100.0, 50.0, 0.0, 0.0)),
        Some(u4.id()),
    );
    let path_entries = vec![u1, a1, u2, a2, compaction1, u3, a3, u4, a4];

    let preparation = prepare_compaction(&path_entries, &DEFAULT_COMPACTION_SETTINGS)
        .expect("preparation must not fail")
        .expect("preparation must be defined");
    assert_eq!(
        preparation.previous_summary.as_ref(),
        Some(&JsString::from("First summary"))
    );
    assert!(
        path_entries
            .iter()
            .any(|entry| entry.id() == &preparation.first_kept_entry_id)
    );
    let context = build_session_context(&path_entries).unwrap();
    assert_eq!(
        preparation.tokens_before,
        estimate_context_tokens(&context.messages).tokens
    );
}

#[test]
fn carries_previous_compactions_kept_entries_into_the_next_preparation() {
    let mut factory = EntryFactory::new();
    let retained_user = user_message("retained user");
    let retained_assistant = assistant_message("retained assistant", usage(100.0, 50.0, 0.0, 0.0));
    let retained_user_entry = factory.message(retained_user.clone(), None);
    let retained_assistant_entry = factory.message(retained_assistant.clone(), Some(retained_user_entry.id()));
    let compaction = factory.compaction(
        "previous summary",
        Some(retained_assistant_entry.id()),
        retained_user_entry.id(),
    );
    let user = factory.message(user_message("new user"), Some(compaction.id()));
    let assistant = factory.message(
        assistant_message("new assistant", usage(100.0, 50.0, 0.0, 0.0)),
        Some(user.id()),
    );

    let path = [
        retained_user_entry,
        retained_assistant_entry,
        compaction,
        user.clone(),
        assistant.clone(),
    ];
    let preparation = prepare_compaction(&path, &settings(100.0, 1.0))
        .expect("preparation must not fail")
        .expect("preparation must be defined");
    assert_eq!(
        preparation.previous_summary.as_ref(),
        Some(&JsString::from("previous summary"))
    );

    let mut all = preparation.messages_to_summarize.clone();
    all.extend(preparation.turn_prefix_messages.clone());
    let first_kept = path
        .iter()
        .position(|entry| entry.id() == &preparation.first_kept_entry_id)
        .unwrap();
    all.extend(path[first_kept..].iter().filter_map(Entry::message).cloned());
    assert_eq!(
        all,
        vec![
            retained_user,
            retained_assistant,
            user.message().unwrap().clone(),
            assistant.message().unwrap().clone(),
        ]
    );
}

#[test]
fn prepares_split_turn_compaction_with_prior_file_operation_details() {
    let mut factory = EntryFactory::new();
    let u1 = factory.message(user_message("user msg 1"), None);
    let a1 = factory.message(
        tool_call_message(
            "write",
            "tool-1",
            json!({"path": "written.ts"}),
            usage(100.0, 50.0, 0.0, 0.0),
        ),
        Some(u1.id()),
    );
    let mut compaction1 = factory.compaction("First summary", Some(a1.id()), &"missing".into());
    let mut raw = compaction1.value();
    raw.as_object_mut().unwrap().insert(
        "details".into(),
        JsValue::from(json!({
            "readFiles": ["old-read.ts"], "modifiedFiles": ["old-edit.ts", "written.ts"]
        })),
    );
    compaction1 = Entry::from_value(raw).unwrap();
    let u2 = factory.message(user_message("large turn"), Some(compaction1.id()));
    let prefix_assistant = factory.message(
        assistant_message("early assistant work", usage(100.0, 50.0, 0.0, 0.0)),
        Some(u2.id()),
    );
    let tool_result = factory.message(
        tool_result_message("tool-1", "write", "tool result"),
        Some(prefix_assistant.id()),
    );
    let suffix_assistant = factory.message(
        assistant_message("recent assistant work", usage(100.0, 50.0, 0.0, 0.0)),
        Some(tool_result.id()),
    );

    let preparation = prepare_compaction(
        &[u1, a1, compaction1, u2, prefix_assistant, tool_result, suffix_assistant],
        &settings(100.0, 1.0),
    )
    .expect("preparation must not fail")
    .expect("preparation must be defined");

    assert_eq!(
        preparation.previous_summary.as_ref(),
        Some(&JsString::from("First summary"))
    );
    assert!(preparation.is_split_turn);
    assert_eq!(
        preparation.turn_prefix_messages.iter().map(role).collect::<Vec<_>>(),
        ["user"]
    );
    assert!(preparation.file_ops.read.contains("old-read.ts"));
    assert!(preparation.file_ops.edited.contains("old-edit.ts"));
    assert!(preparation.file_ops.edited.contains("written.ts"));
}

#[test]
fn does_not_prepare_compaction_when_there_is_nothing_valid_to_compact() {
    let mut factory = EntryFactory::new();
    let compaction = factory.compaction("already compacted", None, &"missing".into());
    assert!(
        prepare_compaction(&[compaction], &DEFAULT_COMPACTION_SETTINGS)
            .expect("preparation must not fail")
            .is_none()
    );
    assert!(
        prepare_compaction(&[], &DEFAULT_COMPACTION_SETTINGS)
            .expect("preparation must not fail")
            .is_none()
    );
}

#[test]
fn default_compaction_settings_match_the_source_values() {
    assert_eq!(
        DEFAULT_COMPACTION_SETTINGS,
        CompactionSettings {
            enabled: true,
            reserve_tokens: 16384.0,
            keep_recent_tokens: 20000.0,
        }
    );
}

#[test]
fn context_usage_estimate_fields_remain_publicly_mutable() {
    let mut estimate = ContextUsageEstimate {
        tokens: 1.0,
        usage_tokens: 2.0,
        trailing_tokens: 3.0,
        last_usage_index: None,
    };
    estimate.last_usage_index = Some(4.0);
    assert_eq!(estimate.last_usage_index, Some(4.0));
}

#[test]
fn compaction_details_fields_remain_publicly_mutable() {
    let mut details = CompactionDetails {
        read_files: vec![JsString::from("a")],
        modified_files: Vec::new(),
    };
    details.modified_files.push(JsString::from("b"));
    assert_eq!(details.modified_files, ["b"]);
}

// ---------------------------------------------------------------------------
// Faux-provider helpers
// ---------------------------------------------------------------------------

static FAUX_COUNT: AtomicUsize = AtomicUsize::new(0);

fn create_faux_model(reasoning: bool, max_tokens: f64) -> (FauxProviderHandle, Model) {
    let count = FAUX_COUNT.fetch_add(1, Ordering::SeqCst) + 1;
    let faux = faux_provider(RegisterFauxProviderOptions {
        provider: Some(format!("faux-{count}")),
        models: Some(vec![FauxModelDefinition {
            id: if reasoning {
                "reasoning-model".to_owned()
            } else {
                "non-reasoning-model".to_owned()
            },
            reasoning: Some(reasoning),
            context_window: Some(200000.0),
            max_tokens: Some(max_tokens),
            ..Default::default()
        }]),
        ..Default::default()
    });
    let model = faux.get_model(None).expect("faux model must exist");
    (faux, model)
}

fn registered_models(faux: &FauxProviderHandle) -> pi_ai::models::ModelsCollection {
    let models = create_models(CreateModelsOptions::default());
    models.set_provider(Arc::clone(&faux.provider));
    models
}

#[derive(Debug, Clone, Default, PartialEq)]
struct CapturedCall {
    prompt: JsString,
    max_tokens: Option<f64>,
    reasoning: Option<pi_ai::types::ThinkingLevel>,
    cache_retention: Option<CacheRetention>,
    session_id: Option<JsString>,
}

type CallLog = Arc<Mutex<Vec<CapturedCall>>>;

fn capture_call(context: &Context, options: Option<&SimpleStreamOptions>) -> CapturedCall {
    CapturedCall {
        prompt: first_user_text(context),
        max_tokens: options.and_then(|options| options.stream.max_tokens),
        reasoning: options.and_then(|options| options.reasoning),
        cache_retention: options.and_then(|options| options.stream.cache_retention),
        session_id: options.and_then(|options| options.stream.session_id.clone()),
    }
}

fn first_user_text(context: &Context) -> JsString {
    match context.messages.first() {
        Some(Message::User(user)) => match &user.content {
            UserContent::Text(text) => text.clone(),
            UserContent::Blocks(blocks) => JsString::join(
                blocks.iter().filter_map(|block| match block {
                    TextOrImageContent::Text(text) => Some(text.text.clone()),
                    TextOrImageContent::Image(_) => None,
                }),
                "",
            ),
        },
        _ => JsString::default(),
    }
}

fn scripted_responses(log: CallLog, texts: Vec<&'static str>) -> Vec<FauxResponseStep> {
    texts
        .into_iter()
        .map(|text| {
            let log = Arc::clone(&log);
            FauxResponseStep::Factory(Arc::new(
                move |context: Context,
                      options: Option<SimpleStreamOptions>,
                      _snapshot: FauxProviderStateSnapshot,
                      _model: Model| {
                    log.lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .push(capture_call(&context, options.as_ref()));
                    Box::pin(async move { Ok(faux_assistant_message(text, FauxAssistantMessageOptions::default())) })
                },
            ))
        })
        .collect()
}

fn scripted_message(text: &str) -> FauxResponseStep {
    FauxResponseStep::message(faux_assistant_message(text, FauxAssistantMessageOptions::default()))
}

fn scripted_failure(stop_reason: StopReason, error_message: &str) -> FauxResponseStep {
    FauxResponseStep::message(faux_assistant_message(
        "",
        FauxAssistantMessageOptions {
            stop_reason: Some(stop_reason),
            error_message: Some(JsString::from(error_message.to_owned())),
            ..Default::default()
        },
    ))
}

fn summary_options<'a>(
    messages: &'a [AgentMessage],
    models: &'a dyn Models,
    model: &'a Model,
    reserve_tokens: f64,
) -> GenerateSummaryOptions<'a> {
    GenerateSummaryOptions {
        current_messages: messages,
        models,
        model,
        reserve_tokens,
        signal: None,
        custom_instructions: None,
        previous_summary: None,
        thinking_level: None,
    }
}

fn preparation(messages: Vec<AgentMessage>, settings: CompactionSettings) -> CompactionPreparation {
    CompactionPreparation {
        messages_to_summarize: messages.clone(),
        turn_prefix_messages: Vec::new(),
        first_kept_entry_id: "kept-entry".into(),
        is_split_turn: false,
        tokens_before: 100.0,
        previous_summary: None,
        file_ops: Default::default(),
        settings,
    }
}

// ---------------------------------------------------------------------------
// Summary generation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ignores_reasoning_for_summary_generation_like_frozen() {
    let messages = vec![user_message("Summarize this.")];
    let log: CallLog = Arc::default();

    let (reasoning_faux, reasoning_model) = create_faux_model(true, 8192.0);
    let models = registered_models(&reasoning_faux);
    reasoning_faux.set_responses(scripted_responses(Arc::clone(&log), vec!["## Goal\nTest summary"]));
    let mut options = summary_options(&messages, &models, &reasoning_model, 2000.0);
    options.thinking_level = Some(ThinkingLevel::Medium);
    generate_summary(&options).await.expect("summary must succeed");
    assert_eq!(log.lock().unwrap()[0].reasoning, None);

    let (off_faux, off_model) = create_faux_model(true, 8192.0);
    let models = registered_models(&off_faux);
    off_faux.set_responses(scripted_responses(Arc::clone(&log), vec!["## Goal\nTest summary"]));
    let mut options = summary_options(&messages, &models, &off_model, 2000.0);
    options.thinking_level = Some(ThinkingLevel::Off);
    generate_summary(&options).await.expect("summary must succeed");
    assert_eq!(log.lock().unwrap()[1].reasoning, None);

    let (plain_faux, plain_model) = create_faux_model(false, 8192.0);
    let models = registered_models(&plain_faux);
    plain_faux.set_responses(scripted_responses(Arc::clone(&log), vec!["## Goal\nTest summary"]));
    let mut options = summary_options(&messages, &models, &plain_model, 2000.0);
    options.thinking_level = Some(ThinkingLevel::Medium);
    generate_summary(&options).await.expect("summary must succeed");
    assert_eq!(log.lock().unwrap()[2].reasoning, None);
}

#[tokio::test]
async fn includes_previous_summaries_and_custom_instructions_in_generate_summary_prompts() {
    let messages = vec![user_message("Summarize this.")];
    let log: CallLog = Arc::default();
    let (faux, model) = create_faux_model(false, 8192.0);
    let models = registered_models(&faux);
    faux.set_responses(scripted_responses(Arc::clone(&log), vec!["## Goal\nTest summary"]));

    let mut options = summary_options(&messages, &models, &model, 2000.0);
    let options_custom_instructions = JsString::from("focus");
    options.custom_instructions = Some(&options_custom_instructions);
    let options_previous_summary = JsString::from("old summary");
    options.previous_summary = Some(&options_previous_summary);
    let summary = generate_summary(&options).await.expect("summary must succeed");
    assert!(summary.contains("Test summary"));
    let prompt = &log.lock().unwrap()[0].prompt;
    assert!(prompt.contains("<previous-summary>\nold summary\n</previous-summary>"));
    assert!(prompt.contains("Additional focus: focus"));
}

#[tokio::test]
async fn preserves_the_string_result_from_generate_summary() {
    let messages = vec![user_message("Summarize this.")];
    let (faux, model) = create_faux_model(false, 8192.0);
    let models = registered_models(&faux);
    faux.set_responses(vec![scripted_message("## Goal\nTest summary")]);

    let options = summary_options(&messages, &models, &model, 2000.0);
    assert_eq!(
        generate_summary(&options).await.expect("summary must succeed"),
        "## Goal\nTest summary"
    );
}

#[tokio::test]
async fn returns_error_results_for_failed_or_aborted_summary_generations() {
    let messages = vec![user_message("Summarize this.")];

    let (error_faux, error_model) = create_faux_model(false, 8192.0);
    let models = registered_models(&error_faux);
    error_faux.set_responses(vec![scripted_failure(StopReason::Error, "boom")]);
    let error = generate_summary(&summary_options(&messages, &models, &error_model, 2000.0))
        .await
        .expect_err("summary must fail");
    assert_eq!(
        error.code,
        pi_agent_core::harness::CompactionErrorCode::SummarizationFailed
    );
    assert_eq!(error.message, "Summarization failed: boom");

    let (aborted_faux, aborted_model) = create_faux_model(false, 8192.0);
    let models = registered_models(&aborted_faux);
    aborted_faux.set_responses(vec![scripted_failure(StopReason::Aborted, "stopped")]);
    let error = generate_summary(&summary_options(&messages, &models, &aborted_model, 2000.0))
        .await
        .expect_err("summary must fail");
    assert_eq!(error.code, pi_agent_core::harness::CompactionErrorCode::Aborted);
    assert_eq!(error.message, "stopped");
}

#[tokio::test]
async fn clamps_compaction_summary_max_tokens_to_the_model_output_cap() {
    let messages = vec![user_message("Summarize this.")];
    let log: CallLog = Arc::default();
    let (faux, model) = create_faux_model(false, 128000.0);
    let models = registered_models(&faux);
    faux.set_responses(scripted_responses(
        Arc::clone(&log),
        vec!["## Goal\nTest summary", "## Goal\nTest summary"],
    ));

    let mut preparation = preparation(messages, settings(500000.0, 20000.0));
    preparation.turn_prefix_messages = preparation.messages_to_summarize.clone();
    preparation.is_split_turn = true;
    preparation.tokens_before = 600000.0;

    compact(&preparation, &models, &model, &CompactOptions::default())
        .await
        .expect("compaction must succeed");

    let calls = log.lock().unwrap().clone();
    assert_eq!(
        calls.iter().map(|call| call.max_tokens).collect::<Vec<_>>(),
        [Some(128000.0), Some(128000.0)]
    );
    assert_eq!(
        calls.iter().map(|call| call.cache_retention).collect::<Vec<_>>(),
        [None, None]
    );
    assert_eq!(calls[0].session_id, None);
    assert_eq!(calls[1].session_id, None);
}

#[tokio::test]
async fn returns_compaction_error_results_without_throwing() {
    let messages = vec![user_message("Summarize this.")];
    let (faux, model) = create_faux_model(false, 8192.0);
    let models = registered_models(&faux);
    faux.set_responses(vec![scripted_failure(StopReason::Error, "history failed")]);

    let error = compact(
        &preparation(messages, settings(2000.0, 20.0)),
        &models,
        &model,
        &CompactOptions::default(),
    )
    .await
    .expect_err("compaction must fail");
    assert_eq!(
        error.code,
        pi_agent_core::harness::CompactionErrorCode::SummarizationFailed
    );
    assert_eq!(error.message, "Summarization failed: history failed");
}

#[tokio::test]
async fn combines_text_for_split_turn_compaction_summaries() {
    let messages = vec![user_message("Summarize this.")];
    let (_, model) = create_faux_model(false, 8192.0);

    let history_usage = usage(1.0, 2.0, 3.0, 4.0);
    let turn_prefix_usage = usage(5.0, 6.0, 7.0, 8.0);
    let mut history_response = faux_assistant_message("history summary", FauxAssistantMessageOptions::default());
    history_response.usage = history_usage;
    let mut turn_prefix_response =
        faux_assistant_message("turn prefix summary", FauxAssistantMessageOptions::default());
    turn_prefix_response.usage = turn_prefix_usage;
    let models = ScriptedModels::new(vec![history_response, turn_prefix_response]);

    let mut preparation = preparation(messages, settings(2000.0, 20.0));
    preparation.turn_prefix_messages = preparation.messages_to_summarize.clone();
    preparation.is_split_turn = true;

    let result = compact(&preparation, &models, &model, &CompactOptions::default())
        .await
        .expect("compaction must succeed");
    assert_eq!(
        result.summary,
        "history summary\n\n---\n\n**Turn Context (split turn):**\n\nturn prefix summary"
    );
}

#[tokio::test]
async fn ignores_reasoning_for_turn_prefix_summaries_like_frozen() {
    let messages = vec![user_message("Summarize this.")];
    let log: CallLog = Arc::default();
    let (faux, model) = create_faux_model(true, 8192.0);
    let models = registered_models(&faux);
    faux.set_responses(scripted_responses(
        Arc::clone(&log),
        vec!["## Original Request\nTest summary"],
    ));

    let mut preparation = preparation(messages, settings(2000.0, 20.0));
    preparation.turn_prefix_messages = std::mem::take(&mut preparation.messages_to_summarize);
    preparation.is_split_turn = true;

    let options = CompactOptions {
        thinking_level: Some(ThinkingLevel::High),
        ..Default::default()
    };
    compact(&preparation, &models, &model, &options)
        .await
        .expect("compaction must succeed");

    assert_eq!(log.lock().unwrap()[0].reasoning, None);
}

#[tokio::test]
async fn returns_turn_prefix_compaction_errors_without_throwing() {
    let messages = vec![user_message("Summarize this.")];
    let (faux, model) = create_faux_model(false, 8192.0);
    let models = registered_models(&faux);
    faux.set_responses(vec![scripted_failure(StopReason::Error, "prefix failed")]);

    let mut preparation = preparation(messages, settings(2000.0, 20.0));
    preparation.turn_prefix_messages = std::mem::take(&mut preparation.messages_to_summarize);
    preparation.is_split_turn = true;

    let error = compact(&preparation, &models, &model, &CompactOptions::default())
        .await
        .expect_err("compaction must fail");
    assert_eq!(
        error.code,
        pi_agent_core::harness::CompactionErrorCode::SummarizationFailed
    );
    assert_eq!(error.message, "Turn prefix summarization failed: prefix failed");

    let (aborted_faux, aborted_model) = create_faux_model(false, 8192.0);
    let models = registered_models(&aborted_faux);
    aborted_faux.set_responses(vec![scripted_failure(StopReason::Aborted, "prefix stopped")]);
    let error = compact(&preparation, &models, &aborted_model, &CompactOptions::default())
        .await
        .expect_err("compaction must fail");
    assert_eq!(error.code, pi_agent_core::harness::CompactionErrorCode::Aborted);
    assert_eq!(error.message, "prefix stopped");
}

#[tokio::test]
async fn returns_a_compaction_result_with_file_details() {
    let mut factory = EntryFactory::new();
    let u1 = factory.message(user_message("read a file"), None);
    let a1 = factory.message(
        tool_call_message(
            "read",
            "tool-1",
            json!({"path": "src/index.ts"}),
            usage(1000.0, 200.0, 0.0, 0.0),
        ),
        Some(u1.id()),
    );
    let u2 = factory.message(user_message("continue"), Some(a1.id()));
    let a2 = factory.message(assistant_message("done", usage(4000.0, 500.0, 0.0, 0.0)), Some(u2.id()));
    let preparation = prepare_compaction(&[u1, a1, u2, a2], &DEFAULT_COMPACTION_SETTINGS)
        .expect("preparation must not fail")
        .expect("preparation must be defined");

    let (faux, model) = create_faux_model(false, 8192.0);
    let models = registered_models(&faux);
    faux.set_responses(vec![scripted_message("## Goal\nTest summary")]);
    let result = compact(&preparation, &models, &model, &CompactOptions::default())
        .await
        .expect("compaction must succeed");

    assert!(!result.summary.is_empty());
    assert!(result.duration_ms.is_some_and(|duration| duration >= 0.0));
    assert_eq!(result.first_kept_entry_id, preparation.first_kept_entry_id);
    assert_eq!(
        result.details,
        Some(CompactionDetails {
            read_files: Vec::new(),
            modified_files: Vec::new(),
        })
    );
    assert_eq!(result.tokens_before, preparation.tokens_before);
}

// ---------------------------------------------------------------------------
// Scripted Models stub (mirrors the TypeScript `Object.create(models)` stub)
// ---------------------------------------------------------------------------

struct ScriptedModels {
    responses: Mutex<VecDeque<AssistantMessage>>,
}

impl ScriptedModels {
    fn new(responses: Vec<AssistantMessage>) -> Self {
        Self {
            responses: Mutex::new(VecDeque::from(responses)),
        }
    }
}

impl Models for ScriptedModels {
    fn get_providers(&self) -> Vec<Arc<dyn pi_ai::models::Provider>> {
        Vec::new()
    }

    fn get_provider(&self, _id: &str) -> Option<Arc<dyn pi_ai::models::Provider>> {
        None
    }

    fn get_models(&self, _provider: Option<&str>) -> Vec<Model> {
        Vec::new()
    }

    fn get_model(&self, _provider: &str, _id: &str) -> Option<Model> {
        None
    }

    fn refresh(&self, _provider: Option<&str>) -> BoxFuture<'static, Result<(), pi_ai::models::ModelsError>> {
        Box::pin(async { Ok(()) })
    }
    fn get_auth(&self, _model: &Model) -> BoxFuture<'static, Result<Option<AuthResult>, ModelsError>> {
        Box::pin(async { Ok(None) })
    }

    fn stream(&self, model: Model, _context: Context, _options: StreamOptions) -> AssistantMessageEventStream {
        pi_ai::request::error_stream(&model, "scripted models do not stream")
    }

    fn complete(
        &self,
        model: Model,
        _context: Context,
        _options: StreamOptions,
    ) -> BoxFuture<'static, Result<AssistantMessage, StreamFailure>> {
        Box::pin(async move {
            Err(StreamFailure::new(format!(
                "scripted models do not complete {}",
                model.id
            )))
        })
    }

    fn stream_simple(
        &self,
        model: Model,
        _context: Context,
        _options: SimpleStreamOptions,
    ) -> AssistantMessageEventStream {
        pi_ai::request::error_stream(&model, "scripted models do not stream")
    }

    fn complete_simple(
        &self,
        _model: Model,
        _context: Context,
        _options: SimpleStreamOptions,
    ) -> BoxFuture<'static, Result<AssistantMessage, StreamFailure>> {
        let response = self
            .responses
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .pop_front()
            .expect("scripted completeSimple response must be queued");
        Box::pin(async move { Ok(response) })
    }
}
