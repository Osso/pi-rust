use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use pi_agent_core::agent::AgentMessage;
use pi_agent_core::harness::BranchSummaryErrorCode;
use pi_agent_core::harness::compaction::branch_summarization::{
    GenerateBranchSummaryOptions, collect_entries_for_branch_summary, generate_branch_summary, prepare_branch_entries,
};
use pi_agent_core::harness::compaction::utils::{FileLists, compute_file_lists, format_file_operations};
use pi_agent_core::harness::compaction::{
    CompactOptions, CompactionPreparation, CompactionSettings, GenerateSummaryOptions, calculate_context_tokens,
    compact, estimate_context_tokens, estimate_tokens, find_cut_point, find_turn_start_index, generate_summary,
    get_last_assistant_usage, prepare_compaction, serialize_conversation,
};
use pi_agent_core::harness::session::{
    InMemorySessionStorage, InMemorySessionStorageOptions, KnownSessionEntry, Session, SessionTreeEntry as Entry,
    SessionTreeEntryBase,
};
use pi_agent_core::harness::types::ThinkingLevel;
use pi_ai::models::{CreateModelsOptions, MutableModels, create_models};
use pi_ai::providers::faux::{
    FauxAssistantMessageOptions, FauxModelDefinition, FauxProviderHandle, FauxProviderStateSnapshot, FauxResponseStep,
    RegisterFauxProviderOptions, faux_assistant_message, faux_provider,
};
use pi_ai::types::{
    AssistantContent, AssistantMessage, Context, Message, Model, StopReason, TextContent, TextOrImageContent,
    ThinkingContent, ToolCall, ToolResultMessage, ToolResultRole, Usage, UsageCost, UserContent, UserMessage, UserRole,
};
use serde_json::{Value, json};

#[path = "support.rs"]
mod support;

use support::TsOracle;

const COMPACTION_MODULE: &str = "packages/agent-core/src/harness/compaction/compaction.ts";
const BRANCH_MODULE: &str = "packages/agent-core/src/harness/compaction/branch-summarization.ts";
const MESSAGES_MODULE: &str = "packages/agent-core/src/harness/messages.ts";
const SESSION_MODULE: &str = "packages/agent-core/src/harness/session/session.ts";

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

fn assistant_blocks(blocks: Vec<AssistantContent>, usage: Usage) -> AgentMessage {
    let mut message = AssistantMessage::empty("anthropic-messages", "anthropic", "claude-sonnet-4-5", 1.0);
    message.content = blocks;
    message.usage = usage;
    message.stop_reason = StopReason::Stop;
    AgentMessage::Assistant(Box::new(message))
}

fn assistant_message(text: &str, usage: Usage) -> AgentMessage {
    assistant_blocks(vec![AssistantContent::Text(TextContent::new(text))], usage)
}

fn tool_call_message(name: &str, id: &str, arguments: Value, usage: Usage) -> AgentMessage {
    assistant_blocks(
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

fn tool_result_message(text: &str, details: Option<Value>) -> AgentMessage {
    AgentMessage::ToolResult(ToolResultMessage {
        role: ToolResultRole::Tag,
        tool_call_id: "call-1".to_owned(),
        tool_name: "read".to_owned(),
        content: vec![TextOrImageContent::Text(TextContent::new(text))],
        details: details.map(JsValue::from),
        is_error: false,
        timestamp: 1.0,
    })
}

fn custom_message(value: Value) -> AgentMessage {
    AgentMessage::Custom(
        value
            .as_object()
            .cloned()
            .expect("custom message must be an object")
            .into_iter()
            .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
            .collect::<pi_js_compat::js_json::JsObject>(),
    )
}

struct EntryFactory {
    next_id: usize,
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
    fn branch_summary(&mut self, summary: &str, details: Option<Value>) -> Entry {
        Entry::from_known(KnownSessionEntry::BranchSummary {
            base: self.base(None),
            from_id: "branch".into(),
            summary: summary.into(),
            details: details.map(JsValue::from),
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

fn message_json(message: &AgentMessage) -> Value {
    serde_json::to_value(message).expect("agent message must serialize")
}

fn entries_json(entries: &[Entry]) -> Value {
    Value::Array(
        entries
            .iter()
            .map(|entry| serde_json::to_value(entry).expect("entry must serialize"))
            .collect(),
    )
}

fn usage_json(usage: &Usage) -> Value {
    serde_json::to_value(usage).expect("usage must serialize")
}

fn file_ops_json(file_ops: &pi_agent_core::harness::compaction::FileOperations) -> Value {
    json!({
        "read": file_ops.read.iter().collect::<Vec<_>>(),
        "written": file_ops.written.iter().collect::<Vec<_>>(),
        "edited": file_ops.edited.iter().collect::<Vec<_>>(),
    })
}

fn settings_json(settings: &CompactionSettings) -> Value {
    json!({
        "enabled": settings.enabled,
        "reserveTokens": settings.reserve_tokens,
        "keepRecentTokens": settings.keep_recent_tokens,
    })
}

fn preparation_json(preparation: &CompactionPreparation) -> Value {
    let mut value = json!({
        "messagesToSummarize": preparation.messages_to_summarize.iter().map(message_json).collect::<Vec<_>>(),
        "turnPrefixMessages": preparation.turn_prefix_messages.iter().map(message_json).collect::<Vec<_>>(),
        "firstKeptEntryId": preparation.first_kept_entry_id,
        "isSplitTurn": preparation.is_split_turn,
        "tokensBefore": preparation.tokens_before,
        "fileOps": file_ops_json(&preparation.file_ops),
        "settings": settings_json(&preparation.settings),
    });
    if let Some(previous_summary) = &preparation.previous_summary {
        value["previousSummary"] = json!(previous_summary);
    }
    value
}

// ---------------------------------------------------------------------------
// Oracle plumbing
// ---------------------------------------------------------------------------

fn case<'a>(response: &'a Value, name: &str) -> &'a Value {
    response["output"]["results"]
        .as_array()
        .unwrap_or_else(|| panic!("oracle response must carry results: {response}"))
        .iter()
        .find(|case| case["name"] == name)
        .unwrap_or_else(|| panic!("oracle response is missing case {name}: {response}"))
}

fn normalize(value: Value) -> Value {
    match value {
        Value::Number(number) => number
            .as_f64()
            .and_then(serde_json::Number::from_f64)
            .map_or(Value::Null, Value::Number),
        Value::Array(items) => Value::Array(items.into_iter().map(normalize).collect()),
        Value::Object(map) => Value::Object(map.into_iter().map(|(key, value)| (key, normalize(value))).collect()),
        other => other,
    }
}

fn expect_output(response: &Value, name: &str, expected: &Value) {
    let case = case(response, name);
    assert_eq!(case["ok"], json!(true), "oracle case {name} failed: {case}");
    assert_eq!(
        normalize(case["output"].clone()),
        normalize(expected.clone()),
        "differential mismatch for case {name}: oracle={} rust={expected}",
        case["output"]
    );
}

fn expect_error(response: &Value, name: &str, code: &str, message: &JsString) {
    let case = case(response, name);
    assert_eq!(
        case["ok"],
        json!(false),
        "oracle case {name} unexpectedly succeeded: {case}"
    );
    assert_eq!(
        case["error"]["code"],
        json!(code),
        "error code mismatch for {name}: {case}"
    );
    assert_eq!(
        case["error"]["message"],
        json!(message),
        "error message mismatch for {name}: {case}"
    );
}

// ---------------------------------------------------------------------------
// Faux provider plumbing
// ---------------------------------------------------------------------------

static FAUX_COUNT: AtomicUsize = AtomicUsize::new(0);

type CallLog = Arc<Mutex<Vec<Value>>>;

fn create_model(spec: &Value) -> (FauxProviderHandle, pi_ai::models::ModelsCollection, Model) {
    let count = FAUX_COUNT.fetch_add(1, Ordering::SeqCst) + 1;
    let faux = faux_provider(RegisterFauxProviderOptions {
        provider: Some(format!("faux-oracle-{count}")),
        models: Some(vec![FauxModelDefinition {
            id: spec["id"].as_str().unwrap_or("oracle-model").to_owned(),
            reasoning: spec["reasoning"].as_bool(),
            context_window: spec["contextWindow"].as_f64(),
            max_tokens: spec["maxTokens"].as_f64(),
            ..Default::default()
        }]),
        ..Default::default()
    });
    let models = create_models(CreateModelsOptions::default());
    models.set_provider(Arc::clone(&faux.provider));
    let model = faux.get_model(None).expect("faux model must exist");
    (faux, models, model)
}

fn model_spec(reasoning: bool, max_tokens: f64, context_window: f64) -> Value {
    json!({
        "id": "oracle-model",
        "reasoning": reasoning,
        "maxTokens": max_tokens,
        "contextWindow": context_window,
    })
}

fn scripted_responses(log: &CallLog, responses: &[Value]) -> Vec<FauxResponseStep> {
    responses
        .iter()
        .map(|response| {
            let log = Arc::clone(log);
            let response = response.clone();
            FauxResponseStep::Factory(Arc::new(
                move |context: Context,
                      options: Option<pi_ai::request::SimpleStreamOptions>,
                      _snapshot: FauxProviderStateSnapshot,
                      _model: Model| {
                    log.lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .push(call_json(&context, options.as_ref()));
                    let text = response["text"].as_str().unwrap_or_default().to_owned();
                    let stop_reason = match response["stopReason"].as_str() {
                        Some("error") => Some(StopReason::Error),
                        Some("aborted") => Some(StopReason::Aborted),
                        _ => None,
                    };
                    let error_message = response["errorMessage"].as_str().map(str::to_owned);
                    Box::pin(async move {
                        Ok(faux_assistant_message(
                            text,
                            FauxAssistantMessageOptions {
                                stop_reason,
                                error_message: error_message.map(JsString::from),
                                ..Default::default()
                            },
                        ))
                    })
                },
            ))
        })
        .collect()
}

fn call_json(context: &Context, options: Option<&pi_ai::request::SimpleStreamOptions>) -> Value {
    json!({
        "prompt": first_user_text(context),
        "maxTokens": options.and_then(|options| options.stream.max_tokens),
        "reasoning": options
            .and_then(|options| options.reasoning)
            .map(|reasoning| serde_json::to_value(reasoning).expect("thinking level must serialize")),
        "cacheRetention": options
            .and_then(|options| options.stream.cache_retention)
            .map(|retention| serde_json::to_value(retention).expect("cache retention must serialize")),
    })
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

fn take_calls(log: &CallLog) -> Value {
    let mut calls = log.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    Value::Array(std::mem::take(&mut *calls))
}

// ---------------------------------------------------------------------------
// Sync operations
// ---------------------------------------------------------------------------

fn compare_estimate_tokens(oracle: &mut TsOracle) {
    let messages = vec![
        ("user text", user_message("plain user")),
        (
            "user blocks",
            AgentMessage::User(UserMessage {
                role: UserRole::Tag,
                content: UserContent::Blocks(vec![
                    TextOrImageContent::Text(TextContent::new("tool text")),
                    TextOrImageContent::Image(pi_ai::types::ImageContent::new("abc", "image/png")),
                ]),
                input_source: None,
                provider_native: None,
                timestamp: 1.0,
            }),
        ),
        (
            "assistant blocks",
            assistant_blocks(
                vec![
                    AssistantContent::Thinking(ThinkingContent::new("thinking")),
                    AssistantContent::ToolCall(ToolCall::new(
                        "call-1",
                        "read",
                        serde_json::Map::from_iter([("path".to_owned(), json!("file.ts"))])
                            .into_iter()
                            .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
                            .collect::<pi_js_compat::js_json::JsObject>(),
                    )),
                ],
                usage(10.0, 5.0, 3.0, 2.0),
            ),
        ),
        ("tool result", tool_result_message("tool text", None)),
        (
            "custom string",
            custom_message(
                json!({"role": "custom", "customType": "note", "content": "custom text", "display": true, "timestamp": 1}),
            ),
        ),
        (
            "bash execution",
            custom_message(json!({
                "role": "bashExecution", "command": "npm run check", "output": "ok",
                "exitCode": 0, "cancelled": false, "truncated": false, "timestamp": 1
            })),
        ),
        (
            "branch summary",
            custom_message(json!({"role": "branchSummary", "summary": "branch", "fromId": "x", "timestamp": 1})),
        ),
        (
            "compaction summary",
            custom_message(
                json!({"role": "compactionSummary", "summary": "compact", "tokensBefore": 123, "timestamp": 1}),
            ),
        ),
        (
            "unknown role",
            custom_message(json!({"role": "unknown", "timestamp": 1})),
        ),
    ];
    let request = json!({
        "id": "estimate_tokens",
        "operation": "estimate_tokens",
        "cases": messages.iter().map(|(name, message)| json!({"name": name, "message": message_json(message)})).collect::<Vec<_>>(),
    });
    let response = oracle.request(&request);
    for (name, message) in &messages {
        expect_output(&response, name, &json!(estimate_tokens(message)));
    }
}

fn compare_calculate_context_tokens(oracle: &mut TsOracle) {
    let cases = [
        ("full", usage(1000.0, 500.0, 200.0, 100.0)),
        ("zero", usage(0.0, 0.0, 0.0, 0.0)),
        (
            "total wins",
            Usage {
                input: 1.0,
                output: 2.0,
                cache_read: 3.0,
                cache_write: 4.0,
                cache_write_1h: None,
                reasoning: None,
                total_tokens: 99.0,
                cost: UsageCost::default(),
            },
        ),
    ];
    let request = json!({
        "id": "calculate_context_tokens",
        "operation": "calculate_context_tokens",
        "cases": cases.iter().map(|(name, usage)| json!({"name": name, "usage": usage_json(usage)})).collect::<Vec<_>>(),
    });
    let response = oracle.request(&request);
    for (name, usage) in &cases {
        expect_output(&response, name, &json!(calculate_context_tokens(usage)));
    }
}

fn compare_get_last_assistant_usage(oracle: &mut TsOracle) {
    let mut factory = EntryFactory::new();
    let valid = assistant_message("assistant", usage(10.0, 5.0, 3.0, 2.0));
    let mut aborted = valid.as_assistant().unwrap().clone();
    aborted.stop_reason = StopReason::Aborted;
    let mut errored = valid.as_assistant().unwrap().clone();
    errored.stop_reason = StopReason::Error;

    let cases: Vec<(&str, Vec<Entry>)> = vec![
        (
            "valid",
            vec![
                factory.message(user_message("user"), None),
                factory.message(valid.clone(), None),
            ],
        ),
        (
            "invalid reasons",
            vec![
                factory.message(AgentMessage::Assistant(Box::new(aborted)), None),
                factory.message(AgentMessage::Assistant(Box::new(errored)), None),
            ],
        ),
        (
            "zero usage falls back",
            vec![
                factory.message(user_message("user"), None),
                factory.message(valid, None),
                factory.message(assistant_message("partial", usage(0.0, 0.0, 0.0, 0.0)), None),
            ],
        ),
        ("none", vec![factory.message(user_message("user"), None)]),
    ];
    let request = json!({
        "id": "get_last_assistant_usage",
        "operation": "get_last_assistant_usage",
        "cases": cases.iter().map(|(name, entries)| json!({"name": name, "entries": entries_json(entries)})).collect::<Vec<_>>(),
    });
    let response = oracle.request(&request);
    for (name, entries) in &cases {
        let expected = get_last_assistant_usage(entries).map_or(Value::Null, usage_json);
        expect_output(&response, name, &expected);
    }
}

fn compare_estimate_context_tokens(oracle: &mut TsOracle) {
    let mut factory = EntryFactory::new();
    let _ = factory.message(user_message("unused"), None);
    let assistant = assistant_message("assistant", usage(10.0, 5.0, 3.0, 2.0));
    let cases: Vec<(&str, Vec<AgentMessage>)> = vec![
        ("no usage", vec![user_message("no usage")]),
        ("trailing", vec![assistant.clone(), user_message("tail")]),
        (
            "partial",
            vec![
                user_message("Hello"),
                assistant,
                user_message("continue"),
                assistant_message("Partial thinking", usage(0.0, 0.0, 0.0, 0.0)),
            ],
        ),
    ];
    let request = json!({
        "id": "estimate_context_tokens",
        "operation": "estimate_context_tokens",
        "cases": cases.iter().map(|(name, messages)| json!({
            "name": name,
            "messages": messages.iter().map(message_json).collect::<Vec<_>>()
        })).collect::<Vec<_>>(),
    });
    let response = oracle.request(&request);
    for (name, messages) in &cases {
        let estimate = estimate_context_tokens(messages);
        let expected = json!({
            "tokens": estimate.tokens,
            "usageTokens": estimate.usage_tokens,
            "trailingTokens": estimate.trailing_tokens,
            "lastUsageIndex": estimate.last_usage_index,
        });
        expect_output(&response, name, &expected);
    }
}

fn repeated_tool_cycle_entries(factory: &mut EntryFactory, cycle_count: usize, result_text: &str) -> Vec<Entry> {
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
        let tool_result = factory.message(tool_result_message(result_text, None), Some(&parent_id));
        parent_id = tool_result.id().to_owned();
        entries.push(tool_result);
    }
    entries
}

fn compare_find_cut_point(oracle: &mut TsOracle) {
    let mut factory = EntryFactory::new();
    let alternating: Vec<Entry> = (0..6)
        .map(|index| {
            if index % 2 == 0 {
                factory.message(user_message(&format!("user {index}")), None)
            } else {
                factory.message(
                    assistant_message(&format!("assistant {index}"), usage(100.0, 50.0, 0.0, 0.0)),
                    None,
                )
            }
        })
        .collect();
    let huge = "x".repeat(40_000);
    let huge_entries = vec![
        factory.message(user_message("old user"), None),
        factory.message(assistant_message("old assistant", usage(100.0, 50.0, 0.0, 0.0)), None),
        factory.message(user_message(&huge), None),
        factory.message(assistant_message(&huge, usage(100.0, 50.0, 0.0, 0.0)), None),
        factory.message(user_message("latest user"), None),
    ];
    let cycle_entries = repeated_tool_cycle_entries(&mut factory, 24, &"x".repeat(4_000));
    let thinking = factory.message(user_message("user"), None);
    let compaction = factory.compaction("summary", Some(thinking.id()), &"missing".into());
    let assistant = factory.message(
        assistant_message("assistant", usage(100.0, 50.0, 0.0, 0.0)),
        Some(compaction.id()),
    );
    let edge_entries = vec![thinking, compaction, assistant];

    let cases: Vec<(&str, Vec<Entry>, usize, usize, f64)> = vec![
        ("alternating", alternating, 0, 6, 20_000.0),
        ("huge suffix", huge_entries, 0, 5, 20_000.0),
        ("tool cycles", cycle_entries.clone(), 0, cycle_entries.len(), 20_000.0),
        ("edge", edge_entries, 0, 3, 1.0),
    ];
    let request = json!({
        "id": "find_cut_point",
        "operation": "find_cut_point",
        "cases": cases.iter().map(|(name, entries, start, end, keep)| json!({
            "name": name,
            "entries": entries_json(entries),
            "startIndex": start,
            "endIndex": end,
            "keepRecentTokens": keep,
        })).collect::<Vec<_>>(),
    });
    let response = oracle.request(&request);
    for (name, entries, start, end, keep) in &cases {
        let result = find_cut_point(entries, *start as f64, *end as f64, *keep).unwrap();
        expect_output(
            &response,
            name,
            &json!({
                "firstKeptEntryIndex": result.first_kept_entry_index,
                "turnStartIndex": result.turn_start_index,
                "isSplitTurn": result.is_split_turn,
            }),
        );
    }
}

fn compare_find_turn_start_index(oracle: &mut TsOracle) {
    let mut factory = EntryFactory::new();
    let thinking = factory.message(user_message("user"), None);
    let branch = factory.branch_summary("branch summary", None);
    let assistant = factory.message(assistant_message("assistant", usage(100.0, 50.0, 0.0, 0.0)), None);
    let entries = vec![thinking, branch, assistant];

    let cases = [("branch", 1_usize, 0_usize), ("assistant", 2, 0), ("missing", 2, 2)];
    let request = json!({
        "id": "find_turn_start_index",
        "operation": "find_turn_start_index",
        "cases": cases.iter().map(|(name, entry_index, start_index)| json!({
            "name": name,
            "entries": entries_json(&entries),
            "entryIndex": entry_index,
            "startIndex": start_index,
        })).collect::<Vec<_>>(),
    });
    let response = oracle.request(&request);
    for (name, entry_index, start_index) in cases {
        expect_output(
            &response,
            name,
            &json!(find_turn_start_index(&entries, entry_index as f64, start_index as f64).unwrap()),
        );
    }
}

fn compare_prepare_compaction(oracle: &mut TsOracle) {
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
    let mut compaction = factory.compaction("First summary", Some(a2.id()), &"missing".into());
    let mut raw = compaction.value();
    raw.as_object_mut().unwrap().insert(
        "details".into(),
        JsValue::from(json!({"readFiles": ["old-read.ts"], "modifiedFiles": ["old-edit.ts"]})),
    );
    compaction = Entry::from_value(raw).unwrap();
    let u3 = factory.message(user_message("user msg 3"), Some(compaction.id()));
    let a3 = factory.message(
        tool_call_message(
            "write",
            "tool-1",
            json!({"path": "written.ts"}),
            usage(8000.0, 2000.0, 0.0, 0.0),
        ),
        Some(u3.id()),
    );
    let tool_result = factory.message(tool_result_message("tool result", None), Some(a3.id()));
    let a4 = factory.message(
        assistant_message("assistant msg 4", usage(100.0, 50.0, 0.0, 0.0)),
        Some(tool_result.id()),
    );
    let full_path = vec![u1, a1, u2, a2, compaction, u3, a3, tool_result, a4];

    let retained_user = factory.message(user_message("retained user"), None);
    let retained_assistant = factory.message(
        assistant_message("retained assistant", usage(100.0, 50.0, 0.0, 0.0)),
        Some(retained_user.id()),
    );
    let retained_compaction = factory.compaction("previous summary", Some(retained_assistant.id()), retained_user.id());
    let new_user = factory.message(user_message("new user"), Some(retained_compaction.id()));
    let new_assistant = factory.message(
        assistant_message("new assistant", usage(100.0, 50.0, 0.0, 0.0)),
        Some(new_user.id()),
    );
    let retained_path = vec![
        retained_user,
        retained_assistant,
        retained_compaction,
        new_user,
        new_assistant,
    ];

    let cycle_entries = repeated_tool_cycle_entries(&mut factory, 24, &"x".repeat(4_000));
    let already_compacted = factory.compaction("already compacted", None, &"missing".into());

    let cases: Vec<(&str, Vec<Entry>, CompactionSettings)> = vec![
        ("default", full_path.clone(), settings(16384.0, 20000.0)),
        ("split", full_path.clone(), settings(100.0, 1.0)),
        ("retained tail", retained_path, settings(100.0, 1.0)),
        ("tool cycles", cycle_entries, settings(16384.0, 20000.0)),
        ("compaction last", vec![already_compacted], settings(16384.0, 20000.0)),
        ("empty", Vec::new(), settings(16384.0, 20000.0)),
    ];
    let request = json!({
        "id": "prepare_compaction",
        "operation": "prepare_compaction",
        "cases": cases.iter().map(|(name, entries, settings)| json!({
            "name": name,
            "pathEntries": entries_json(entries),
            "settings": settings_json(settings),
        })).collect::<Vec<_>>(),
    });
    let response = oracle.request(&request);
    for (name, entries, settings) in &cases {
        let expected = prepare_compaction(entries, settings)
            .expect("preparation must not fail")
            .map_or(Value::Null, |preparation| preparation_json(&preparation));
        expect_output(&response, name, &expected);
    }
}

fn compare_serialize_conversation(oracle: &mut TsOracle) {
    let cases: Vec<(&str, Vec<AgentMessage>)> = vec![
        (
            "text and tool calls",
            vec![
                user_message("hello"),
                assistant_blocks(
                    vec![
                        AssistantContent::Text(TextContent::new("reading")),
                        AssistantContent::ToolCall(ToolCall::new(
                            "call-1",
                            "read",
                            (serde_json::Map::from_iter([
                                ("path".to_owned(), json!("src/a.ts")),
                                ("limit".to_owned(), json!(10)),
                                ("flag".to_owned(), json!(true)),
                                ("nested".to_owned(), json!({"2": 2.0, "1": 1.0})),
                                ("2".to_owned(), json!(2.0)),
                                ("1".to_owned(), json!(1.0)),
                            ]))
                            .into_iter()
                            .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
                            .collect::<pi_js_compat::js_json::JsObject>(),
                        )),
                    ],
                    usage(100.0, 50.0, 0.0, 0.0),
                ),
            ],
        ),
        (
            "truncated tool result",
            vec![tool_result_message(
                &"x".repeat(5000),
                Some(json!({"fullOutputPath": "/tmp/full.log", "truncation": {"truncated": true}})),
            )],
        ),
        (
            "custom and bash",
            vec![
                custom_message(
                    json!({"role": "custom", "customType": "note", "content": "custom content", "display": true, "timestamp": 1}),
                ),
                custom_message(json!({
                    "role": "bashExecution", "command": "ls", "output": "a\nb",
                    "exitCode": 2, "cancelled": false, "truncated": false, "timestamp": 1
                })),
                custom_message(json!({"role": "branchSummary", "summary": "branch", "fromId": "x", "timestamp": 1})),
                custom_message(
                    json!({"role": "compactionSummary", "summary": "compact", "tokensBefore": 1, "timestamp": 1}),
                ),
            ],
        ),
    ];
    let request = json!({
        "id": "serialize_conversation",
        "operation": "serialize_conversation",
        "cases": cases.iter().map(|(name, messages)| json!({
            "name": name,
            "messages": messages.iter().map(message_json).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    });
    let response = oracle.request(&request);
    for (name, messages) in &cases {
        let converted = pi_agent_core::harness::messages::convert_to_llm(messages).expect("convert fixture messages");
        expect_output(&response, name, &json!(serialize_conversation(&converted)));
    }
}

fn compare_prepare_branch_entries(oracle: &mut TsOracle) {
    let mut factory = EntryFactory::new();
    let branch_entries = vec![
        factory.message(user_message("explore the branch"), None),
        factory.message(
            tool_call_message(
                "read",
                "call-1",
                json!({"path": "src/a.ts"}),
                usage(100.0, 50.0, 0.0, 0.0),
            ),
            None,
        ),
        factory.message(tool_result_message("contents", None), None),
        factory.branch_summary(
            "earlier branch",
            Some(json!({"readFiles": ["old-read.ts"], "modifiedFiles": ["old-edit.ts"]})),
        ),
    ];
    let huge = "x".repeat(40_000);
    let budget_entries = vec![
        factory.message(user_message("old user"), None),
        factory.message(assistant_message("old assistant", usage(100.0, 50.0, 0.0, 0.0)), None),
        factory.message(user_message(&huge), None),
        factory.message(assistant_message(&huge, usage(100.0, 50.0, 0.0, 0.0)), None),
        factory.message(user_message("latest user"), None),
    ];

    let cases: Vec<(&str, Vec<Entry>, f64)> = vec![
        ("no budget", branch_entries.clone(), 0.0),
        ("budget", budget_entries, 20_000.0),
        ("summary entry", branch_entries, 10.0),
    ];
    let request = json!({
        "id": "prepare_branch_entries",
        "operation": "prepare_branch_entries",
        "cases": cases.iter().map(|(name, entries, budget)| json!({
            "name": name,
            "entries": entries_json(entries),
            "tokenBudget": budget,
        })).collect::<Vec<_>>(),
    });
    let response = oracle.request(&request);
    for (name, entries, budget) in &cases {
        let preparation = prepare_branch_entries(entries, Some(*budget));
        let expected = json!({
            "messages": preparation.messages.iter().map(message_json).collect::<Vec<_>>(),
            "fileOps": file_ops_json(&preparation.file_ops),
            "totalTokens": preparation.total_tokens,
        });
        expect_output(&response, name, &expected);
    }
}

fn compare_file_lists(oracle: &mut TsOracle) {
    let mut file_ops = pi_agent_core::harness::compaction::FileOperations::default();
    file_ops.read.extend(
        ["z.ts".to_owned(), "a.ts".to_owned(), "shared.ts".to_owned()]
            .into_iter()
            .map(JsString::from),
    );
    file_ops.written.insert(JsString::from("shared.ts"));
    file_ops
        .edited
        .extend(["m.ts".to_owned(), "b.ts".to_owned()].into_iter().map(JsString::from));
    let empty = pi_agent_core::harness::compaction::FileOperations::default();

    let cases = [("populated", &file_ops), ("empty", &empty)];
    let request = json!({
        "id": "compute_file_lists",
        "operation": "compute_file_lists",
        "cases": cases.iter().map(|(name, file_ops)| json!({
            "name": name,
            "fileOps": file_ops_json(file_ops),
        })).collect::<Vec<_>>(),
    });
    let response = oracle.request(&request);
    for (name, file_ops) in cases {
        let FileLists {
            read_files,
            modified_files,
        } = compute_file_lists(file_ops);
        expect_output(
            &response,
            name,
            &json!({"readFiles": read_files, "modifiedFiles": modified_files}),
        );
    }

    let format_cases: Vec<(&str, Vec<String>, Vec<String>)> = vec![
        ("empty", Vec::new(), Vec::new()),
        ("both", vec!["src/read.ts".to_owned()], vec!["src/write.ts".to_owned()]),
        (
            "unicode",
            vec!["é/技能/😀\0<&>\"'".to_owned()],
            vec!["changed\n\n".to_owned()],
        ),
    ];
    let request = json!({
        "id": "format_file_operations",
        "operation": "format_file_operations",
        "cases": format_cases.iter().map(|(name, read, modified)| json!({
            "name": name,
            "readFiles": read,
            "modifiedFiles": modified,
        })).collect::<Vec<_>>(),
    });
    let response = oracle.request(&request);
    for (name, read, modified) in &format_cases {
        expect_output(
            &response,
            name,
            &json!(format_file_operations(
                &read.iter().map(JsString::from).collect::<Vec<_>>(),
                &modified.iter().map(JsString::from).collect::<Vec<_>>()
            )),
        );
    }
}

// ---------------------------------------------------------------------------
// Async operations
// ---------------------------------------------------------------------------

// (name, model, reserveTokens, customInstructions, previousSummary, thinkingLevel, response)
type SummaryCase<'a> = (
    &'a str,
    Value,
    f64,
    Option<&'a str>,
    Option<&'a str>,
    Option<&'a str>,
    Value,
);

async fn compare_generate_summary(oracle: &mut TsOracle) {
    let messages = vec![user_message("Summarize this.")];
    let cases: Vec<SummaryCase> = vec![
        (
            "basic",
            model_spec(false, 8192.0, 200000.0),
            2000.0,
            None,
            None,
            None,
            json!({"text": "## Goal\nTest summary"}),
        ),
        (
            "reasoning",
            model_spec(true, 8192.0, 200000.0),
            2000.0,
            None,
            None,
            Some("medium"),
            json!({"text": "## Goal\nTest summary"}),
        ),
        (
            "off",
            model_spec(true, 8192.0, 200000.0),
            2000.0,
            None,
            None,
            Some("off"),
            json!({"text": "## Goal\nTest summary"}),
        ),
        (
            "previous and custom",
            model_spec(false, 8192.0, 200000.0),
            2000.0,
            Some("focus"),
            Some("old summary"),
            None,
            json!({"text": "## Goal\nTest summary"}),
        ),
        (
            "clamp",
            model_spec(false, 128000.0, 200000.0),
            500000.0,
            None,
            None,
            None,
            json!({"text": "## Goal\nTest summary"}),
        ),
        (
            "error",
            model_spec(false, 8192.0, 200000.0),
            2000.0,
            None,
            None,
            None,
            json!({"text": "", "stopReason": "error", "errorMessage": "boom"}),
        ),
        (
            "aborted",
            model_spec(false, 8192.0, 200000.0),
            2000.0,
            None,
            None,
            None,
            json!({"text": "", "stopReason": "aborted", "errorMessage": "stopped"}),
        ),
    ];

    let request = json!({
        "id": "generate_summary",
        "operation": "generate_summary",
        "cases": cases.iter().map(|(name, model, reserve, custom, previous, thinking, response)| {
            let mut case = json!({
                "name": name,
                "messages": messages.iter().map(message_json).collect::<Vec<_>>(),
                "model": model,
                "reserveTokens": reserve,
                "response": response,
            });
            if let Some(custom) = custom { case["customInstructions"] = json!(custom); }
            if let Some(previous) = previous { case["previousSummary"] = json!(previous); }
            if let Some(thinking) = thinking { case["thinkingLevel"] = json!(thinking); }
            case
        }).collect::<Vec<_>>(),
    });
    let response = oracle.request(&request);

    for (name, model, reserve, custom, previous, thinking, scripted) in &cases {
        let (faux, models, model) = create_model(model);
        let log: CallLog = Arc::default();
        faux.set_responses(scripted_responses(&log, std::slice::from_ref(scripted)));
        let native_custom = (*custom).map(JsString::from);
        let native_previous = (*previous).map(JsString::from);
        let mut options = GenerateSummaryOptions {
            current_messages: &messages,
            models: &models,
            model: &model,
            reserve_tokens: *reserve,
            signal: None,
            custom_instructions: native_custom.as_ref(),
            previous_summary: native_previous.as_ref(),
            thinking_level: (*thinking).map(parse_thinking_level),
        };
        let _ = &mut options;
        match generate_summary(&options).await {
            Ok(summary) => expect_output(&response, name, &json!({"text": summary, "calls": take_calls(&log)})),
            Err(error) => expect_error(&response, name, error.code.as_str(), &error.message),
        }
    }
}

fn parse_thinking_level(value: &str) -> ThinkingLevel {
    match value {
        "minimal" => ThinkingLevel::Minimal,
        "low" => ThinkingLevel::Low,
        "medium" => ThinkingLevel::Medium,
        "high" => ThinkingLevel::High,
        "xhigh" => ThinkingLevel::XHigh,
        "max" => ThinkingLevel::Max,
        "ultra" => ThinkingLevel::Ultra,
        _ => ThinkingLevel::Off,
    }
}

fn direct_preparation(
    messages: Vec<AgentMessage>,
    turn_prefix_messages: Vec<AgentMessage>,
    first_kept_entry_id: &str,
    is_split_turn: bool,
    settings: CompactionSettings,
) -> CompactionPreparation {
    CompactionPreparation {
        messages_to_summarize: messages,
        turn_prefix_messages,
        first_kept_entry_id: first_kept_entry_id.into(),
        is_split_turn,
        tokens_before: 100.0,
        previous_summary: None,
        file_ops: Default::default(),
        settings,
    }
}

// (name, preparation, model, responses, thinkingLevel)
type CompactCase<'a> = (&'a str, &'a CompactionPreparation, Value, Vec<Value>, Option<&'a str>);

async fn compare_compact(oracle: &mut TsOracle) {
    let messages = vec![user_message("Summarize this.")];
    let split_preparation = direct_preparation(
        messages.clone(),
        messages.clone(),
        "kept-entry",
        true,
        settings(2000.0, 20.0),
    );
    let prefix_only = direct_preparation(Vec::new(), messages.clone(), "kept-entry", true, settings(2000.0, 20.0));
    let history_only = direct_preparation(
        messages.clone(),
        Vec::new(),
        "kept-entry",
        false,
        settings(2000.0, 20.0),
    );
    let clamp_preparation = direct_preparation(
        messages.clone(),
        messages.clone(),
        "kept-entry",
        true,
        settings(500000.0, 20.0),
    );

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
    let file_preparation = prepare_compaction(&[u1, a1, u2, a2], &settings(16384.0, 20000.0))
        .expect("preparation must not fail")
        .expect("preparation must be defined");

    let cases: Vec<CompactCase> = vec![
        (
            "split usage",
            &split_preparation,
            model_spec(false, 8192.0, 200000.0),
            vec![
                json!({"text": "history summary"}),
                json!({"text": "turn prefix summary"}),
            ],
            None,
        ),
        (
            "file details",
            &file_preparation,
            model_spec(false, 8192.0, 200000.0),
            vec![json!({"text": "## Goal\nTest summary"})],
            None,
        ),
        (
            "history error",
            &history_only,
            model_spec(false, 8192.0, 200000.0),
            vec![json!({"text": "", "stopReason": "error", "errorMessage": "history failed"})],
            None,
        ),
        (
            "prefix error",
            &prefix_only,
            model_spec(false, 8192.0, 200000.0),
            vec![json!({"text": "", "stopReason": "error", "errorMessage": "prefix failed"})],
            None,
        ),
        (
            "prefix aborted",
            &prefix_only,
            model_spec(false, 8192.0, 200000.0),
            vec![json!({"text": "", "stopReason": "aborted", "errorMessage": "prefix stopped"})],
            None,
        ),
        (
            "reasoning",
            &prefix_only,
            model_spec(true, 8192.0, 200000.0),
            vec![json!({"text": "## Original Request\nTest summary"})],
            Some("high"),
        ),
        (
            "clamp",
            &clamp_preparation,
            model_spec(false, 128000.0, 200000.0),
            vec![json!({"text": "history"}), json!({"text": "prefix"})],
            None,
        ),
    ];

    let request = json!({
        "id": "compact",
        "operation": "compact",
        "cases": cases.iter().map(|(name, preparation, model, responses, thinking)| {
            let mut case = json!({
                "name": name,
                "preparation": preparation_json(preparation),
                "model": model,
                "responses": responses,
            });
            if let Some(thinking) = thinking { case["thinkingLevel"] = json!(thinking); }
            case
        }).collect::<Vec<_>>(),
    });
    let response = oracle.request(&request);

    for (name, preparation, model, responses, thinking) in &cases {
        let (faux, models, model) = create_model(model);
        let log: CallLog = Arc::default();
        faux.set_responses(scripted_responses(&log, responses));
        let options = CompactOptions {
            custom_instructions: None,
            signal: None,
            thinking_level: (*thinking).map(parse_thinking_level),
        };
        match compact(preparation, &models, &model, &options).await {
            Ok(result) => expect_output(
                &response,
                name,
                &json!({
                    "summary": result.summary,
                    "tokensBefore": result.tokens_before,
                    "firstKeptEntryId": result.first_kept_entry_id,
                    "details": result.details.as_ref().map(|details| json!({
                        "readFiles": details.read_files,
                        "modifiedFiles": details.modified_files,
                    })),
                    "calls": take_calls(&log),
                }),
            ),
            Err(error) => expect_error(&response, name, error.code.as_str(), &error.message),
        }
    }
}

// (name, entries, model, customInstructions, replaceInstructions, reserveTokens, responses)
type BranchCase<'a> = (
    &'a str,
    Vec<Entry>,
    Value,
    Option<&'a str>,
    bool,
    Option<f64>,
    Vec<Value>,
);

async fn compare_generate_branch_summary(oracle: &mut TsOracle) {
    let mut factory = EntryFactory::new();
    let entries = vec![
        factory.message(user_message("explore the branch"), None),
        factory.message(
            tool_call_message(
                "read",
                "call-1",
                json!({"path": "src/a.ts"}),
                usage(100.0, 50.0, 0.0, 0.0),
            ),
            None,
        ),
        factory.message(tool_result_message("contents", None), None),
        factory.message(assistant_message("found it", usage(100.0, 50.0, 0.0, 0.0)), None),
    ];
    let empty_entries = vec![factory.message(tool_result_message("contents", None), None)];
    let small_entries = vec![
        factory.message(user_message(&"x".repeat(40_000)), None),
        factory.message(
            assistant_message(&"x".repeat(40_000), usage(100.0, 50.0, 0.0, 0.0)),
            None,
        ),
    ];

    let cases: Vec<BranchCase> = vec![
        (
            "basic",
            entries.clone(),
            model_spec(false, 8192.0, 200000.0),
            None,
            false,
            None,
            vec![json!({"text": "## Goal\nExplore"})],
        ),
        (
            "custom",
            entries.clone(),
            model_spec(false, 8192.0, 200000.0),
            Some("focus on files"),
            false,
            None,
            vec![json!({"text": "## Goal\nExplore"})],
        ),
        (
            "replace",
            entries.clone(),
            model_spec(false, 8192.0, 200000.0),
            Some("only this instruction"),
            true,
            None,
            vec![json!({"text": "## Goal\nExplore"})],
        ),
        (
            "empty",
            empty_entries,
            model_spec(false, 8192.0, 200000.0),
            None,
            false,
            None,
            vec![json!({"text": "unused"})],
        ),
        (
            "error",
            entries.clone(),
            model_spec(false, 8192.0, 200000.0),
            None,
            false,
            None,
            vec![json!({"text": "", "stopReason": "error", "errorMessage": "boom"})],
        ),
        (
            "aborted",
            entries.clone(),
            model_spec(false, 8192.0, 200000.0),
            None,
            false,
            None,
            vec![json!({"text": "", "stopReason": "aborted", "errorMessage": "stopped"})],
        ),
        (
            "small context",
            small_entries,
            model_spec(false, 8192.0, 100.0),
            None,
            false,
            Some(200.0),
            vec![json!({"text": "summary"})],
        ),
    ];

    let request = json!({
        "id": "generate_branch_summary",
        "operation": "generate_branch_summary",
        "cases": cases.iter().map(|(name, entries, model, custom, replace, reserve, responses)| {
            let mut case = json!({
                "name": name,
                "entries": entries_json(entries),
                "model": model,
                "replaceInstructions": replace,
                "responses": responses,
            });
            if let Some(custom) = custom { case["customInstructions"] = json!(custom); }
            if let Some(reserve) = reserve { case["reserveTokens"] = json!(reserve); }
            case
        }).collect::<Vec<_>>(),
    });
    let response = oracle.request(&request);

    for (name, entries, model, custom, replace, reserve, responses) in &cases {
        let (faux, models, model) = create_model(model);
        let log: CallLog = Arc::default();
        faux.set_responses(scripted_responses(&log, responses));
        let native_custom = (*custom).map(JsString::from);
        let options = GenerateBranchSummaryOptions {
            models: &models,
            model: &model,
            signal: None,
            custom_instructions: native_custom.as_ref(),
            replace_instructions: *replace,
            reserve_tokens: *reserve,
        };
        match generate_branch_summary(entries, &options).await {
            Ok(result) => expect_output(
                &response,
                name,
                &json!({
                    "summary": result.summary,
                    "readFiles": result.read_files,
                    "modifiedFiles": result.modified_files,
                    "calls": take_calls(&log),
                }),
            ),
            Err(error) => {
                let code = match error.code {
                    BranchSummaryErrorCode::Aborted => "aborted",
                    BranchSummaryErrorCode::SummarizationFailed => "summarization_failed",
                    BranchSummaryErrorCode::InvalidSession => "invalid_session",
                };
                expect_error(&response, name, code, &error.message);
            }
        }
    }
}

async fn compare_collect_entries(oracle: &mut TsOracle) {
    let mut factory = EntryFactory::new();
    let root = factory.message(user_message("root"), None);
    let common = factory.message(user_message("common"), Some(root.id()));
    let old = factory.message(user_message("abandoned 1"), Some(common.id()));
    let old_leaf = factory.message(user_message("abandoned 2"), Some(old.id()));
    let target = factory.message(user_message("target"), Some(common.id()));
    let entries = vec![root, common, old, old_leaf.clone(), target.clone()];
    let cases = [("branch", Some(old_leaf.id().clone())), ("no previous leaf", None)];
    let response = oracle.request(&json!({"operation":"collect_entries_for_branch_summary","cases":cases.iter().map(|(name,old)| json!({"name":name,"entries":entries_json(&entries),"oldLeafId":old,"targetId":target.id()})).collect::<Vec<_>>()}));
    let session = Session::new(Arc::new(
        InMemorySessionStorage::new(InMemorySessionStorageOptions {
            entries,
            ..Default::default()
        })
        .unwrap(),
    ));
    for (name, old) in cases {
        let result = collect_entries_for_branch_summary(&session, old.as_ref(), target.id())
            .await
            .unwrap();
        expect_output(
            &response,
            name,
            &json!({"entryIds":result.entries.iter().map(Entry::id).collect::<Vec<_>>(),"commonAncestorId":result.common_ancestor_id}),
        );
    }
}

pub(crate) async fn run() {
    let mut oracle = TsOracle::spawn(&[COMPACTION_MODULE, BRANCH_MODULE, MESSAGES_MODULE, SESSION_MODULE]);

    compare_estimate_tokens(&mut oracle);
    compare_calculate_context_tokens(&mut oracle);
    compare_get_last_assistant_usage(&mut oracle);
    compare_estimate_context_tokens(&mut oracle);
    compare_find_cut_point(&mut oracle);
    compare_find_turn_start_index(&mut oracle);
    compare_prepare_compaction(&mut oracle);
    compare_serialize_conversation(&mut oracle);
    compare_prepare_branch_entries(&mut oracle);
    compare_file_lists(&mut oracle);
    compare_generate_summary(&mut oracle).await;
    compare_compact(&mut oracle).await;
    compare_generate_branch_summary(&mut oracle).await;
    compare_collect_entries(&mut oracle).await;
}
