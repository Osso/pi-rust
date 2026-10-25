//! Behavioral tests for `coding-agent/src/core/compaction/branch-summarization.ts`.

use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use std::sync::{Arc, Mutex};

use pi_agent_core::agent::{AgentMessage, StreamFn};
use pi_ai::request::SimpleStreamOptions;
use pi_ai::types::{
    AssistantContent, AssistantMessage, Context, Model, StopReason, TextContent, Usage, UsageCost, UserContent,
    UserMessage, UserRole,
};
use pi_coding_agent::core::compaction::branch_summarization::{
    GenerateBranchSummaryOptions, collect_entries_for_branch_summary, generate_branch_summary, prepare_branch_entries,
};
use pi_coding_agent::core::session_manager::{SessionEntry, SessionManager};
use serde_json::{Value, json};

fn user(text: &str) -> AgentMessage {
    AgentMessage::User(UserMessage {
        role: UserRole::Tag,
        content: UserContent::Text(JsString::from(text.to_owned())),
        input_source: None,
        provider_native: None,
        timestamp: 0.0,
    })
}

fn assistant_with(content: Vec<AssistantContent>) -> AgentMessage {
    let mut message = AssistantMessage::empty("anthropic-messages", "anthropic", "claude-sonnet-4-5", 0.0);
    message.content = content;
    message.usage = Usage {
        cost: UsageCost::default(),
        ..Usage::default()
    };
    message.stop_reason = StopReason::Stop;
    AgentMessage::Assistant(Box::new(message))
}

fn assistant_text(text: &str) -> AgentMessage {
    assistant_with(vec![AssistantContent::Text(TextContent::new(text))])
}

fn tool_call_message(name: &str, id: &str, arguments: Value) -> AgentMessage {
    assistant_with(vec![AssistantContent::ToolCall(pi_ai::types::ToolCall::new(
        id,
        name,
        arguments
            .as_object()
            .cloned()
            .expect("arguments must be an object")
            .into_iter()
            .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
            .collect::<pi_js_compat::js_json::JsObject>(),
    ))])
}

fn tool_result_message(tool_call_id: &str, text: &str) -> AgentMessage {
    AgentMessage::ToolResult(pi_ai::types::ToolResultMessage {
        role: pi_ai::types::ToolResultRole::Tag,
        tool_call_id: tool_call_id.to_owned(),
        tool_name: "read".to_owned(),
        content: vec![pi_ai::types::TextOrImageContent::Text(TextContent::new(text))],
        details: None,
        is_error: false,
        timestamp: 0.0,
    })
}

struct EntryFactory {
    counter: usize,
    last_id: Option<String>,
}

impl EntryFactory {
    fn new() -> Self {
        Self {
            counter: 0,
            last_id: None,
        }
    }

    fn entry(&mut self, value: Value) -> SessionEntry {
        let id = format!("entry-{}", self.counter);
        self.counter += 1;
        let mut object = value.as_object().expect("entry fixture must be an object").clone();
        object.insert("id".to_owned(), Value::String(id.clone()));
        object.insert(
            "parentId".to_owned(),
            self.last_id.clone().map(Value::String).unwrap_or(Value::Null),
        );
        object.insert(
            "timestamp".to_owned(),
            Value::String("2026-01-01T00:00:00.000Z".to_owned()),
        );
        self.last_id = Some(id);
        serde_json::from_value(Value::Object(object)).expect("entry fixture must deserialize")
    }

    fn message(&mut self, message: &AgentMessage) -> SessionEntry {
        self.entry(json!({ "type": "message", "message": message }))
    }

    fn branch_summary(&mut self, summary: &str, details: Option<Value>) -> SessionEntry {
        let mut value = json!({
            "type": "branch_summary",
            "fromId": "from",
            "summary": summary,
        });
        if let Some(details) = details {
            value
                .as_object_mut()
                .expect("object")
                .insert("details".to_owned(), details);
        }
        self.entry(value)
    }

    fn branch_summary_from_hook(&mut self, summary: &str) -> SessionEntry {
        self.entry(json!({
            "type": "branch_summary",
            "fromId": "from",
            "summary": summary,
            "fromHook": true,
        }))
    }

    fn compaction(&mut self, summary: &str, duration_ms: Option<f64>) -> SessionEntry {
        let mut value = json!({
            "type": "compaction",
            "summary": summary,
            "firstKeptEntryId": "entry-0",
            "tokensBefore": 12,
        });
        if let Some(duration_ms) = duration_ms {
            value
                .as_object_mut()
                .expect("object")
                .insert("durationMs".to_owned(), json!(duration_ms));
        }
        self.entry(value)
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

fn model(context_window: f64) -> Model {
    Model {
        properties: Default::default(),
        id: "claude-sonnet-4-5".to_owned(),
        name: "Claude".into(),
        api: "anthropic-messages".to_owned(),
        provider: "anthropic".to_owned(),
        base_url: JsString::from("https://example.invalid"),
        reasoning: false,
        thinking_level_map: None,
        input: vec![pi_ai::types::Modality::Text],
        cost: pi_ai::types::ModelCost::default(),
        context_window,
        auto_compaction_threshold: None,
        max_tokens: 8192.0,
        headers: None,
        compat: None,
    }
}

#[derive(Debug, Clone, PartialEq)]
struct CapturedCall {
    system_prompt: Option<JsString>,
    prompt: JsString,
    max_tokens: Option<f64>,
    cache_retention: Option<pi_ai::types::CacheRetention>,
}

fn first_user_text(context: &Context) -> JsString {
    match context.messages.first() {
        Some(pi_ai::types::Message::User(user)) => match &user.content {
            UserContent::Text(text) => text.clone(),
            UserContent::Blocks(blocks) => JsString::join(
                blocks.iter().filter_map(|block| match block {
                    pi_ai::types::TextOrImageContent::Text(text) => Some(&text.text),
                    pi_ai::types::TextOrImageContent::Image(_) => None,
                }),
                "",
            ),
        },
        _ => JsString::default(),
    }
}

fn stream_fn(log: Arc<Mutex<Vec<CapturedCall>>>, message: AssistantMessage) -> StreamFn {
    Arc::new(move |_model, context, options: SimpleStreamOptions| {
        log.lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(CapturedCall {
                system_prompt: context.system_prompt.clone(),
                prompt: first_user_text(&context),
                max_tokens: options.stream.max_tokens,
                cache_retention: options.stream.cache_retention,
            });
        let message = message.clone();
        Box::pin(async move {
            let (sender, stream) = pi_ai::utils::event_stream::create_assistant_message_event_stream();
            sender.push(pi_ai::types::AssistantMessageEvent::Start {
                partial: message.clone(),
            });
            sender.push(pi_ai::types::AssistantMessageEvent::Done {
                reason: pi_ai::types::DoneReason::Stop,
                message,
            });
            Ok(stream)
        })
    })
}

fn response(text: &str) -> AssistantMessage {
    let mut message = AssistantMessage::empty("anthropic-messages", "anthropic", "claude-sonnet-4-5", 0.0);
    message.content = vec![AssistantContent::Text(TextContent::new(text))];
    message.usage = Usage {
        input: 3.0,
        output: 4.0,
        total_tokens: 7.0,
        cost: UsageCost::default(),
        ..Usage::default()
    };
    message.stop_reason = StopReason::Stop;
    message
}

fn failure(stop_reason: StopReason, error_message: &str) -> AssistantMessage {
    let mut message = AssistantMessage::empty("anthropic-messages", "anthropic", "claude-sonnet-4-5", 0.0);
    message.stop_reason = stop_reason;
    message.error_message = Some(error_message.into());
    message
}

fn options<'a>(model: &'a Model, stream_fn: Option<&'a StreamFn>) -> GenerateBranchSummaryOptions<'a> {
    GenerateBranchSummaryOptions {
        model,
        api_key: None,
        headers: None,
        env: None,
        signal: None,
        custom_instructions: None,
        replace_instructions: false,
        reserve_tokens: None,
        stream_fn,
    }
}

// ---------------------------------------------------------------------------
// Branch collection
// ---------------------------------------------------------------------------

#[test]
fn collects_the_abandoned_side_of_a_branch_in_chronological_order() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("session.jsonl");
    std::fs::write(
        &path,
        concat!(
            "{\"type\":\"session\",\"version\":3,\"id\":\"s\",\"timestamp\":\"2026-01-01T00:00:00.000Z\",\"cwd\":\"/tmp\"}\n",
            "{\"type\":\"message\",\"id\":\"1\",\"parentId\":null,\"timestamp\":\"2026-01-01T00:00:00.000Z\",\"message\":{\"role\":\"user\",\"content\":\"root\",\"timestamp\":0}}\n",
            "{\"type\":\"message\",\"id\":\"2\",\"parentId\":\"1\",\"timestamp\":\"2026-01-01T00:00:00.000Z\",\"message\":{\"role\":\"user\",\"content\":\"common\",\"timestamp\":0}}\n",
            "{\"type\":\"message\",\"id\":\"3\",\"parentId\":\"2\",\"timestamp\":\"2026-01-01T00:00:00.000Z\",\"message\":{\"role\":\"user\",\"content\":\"abandoned 1\",\"timestamp\":0}}\n",
            "{\"type\":\"message\",\"id\":\"4\",\"parentId\":\"3\",\"timestamp\":\"2026-01-01T00:00:00.000Z\",\"message\":{\"role\":\"user\",\"content\":\"abandoned 2\",\"timestamp\":0}}\n",
            "{\"type\":\"message\",\"id\":\"5\",\"parentId\":\"2\",\"timestamp\":\"2026-01-01T00:00:00.000Z\",\"message\":{\"role\":\"user\",\"content\":\"target\",\"timestamp\":0}}\n",
        ),
    )
    .expect("write session");

    let session = SessionManager::open(&JsString::from(&path.display().to_string()), None, None).expect("open session");
    let result = collect_entries_for_branch_summary(&session, Some(&JsString::from("4")), &JsString::from("5"));

    assert_eq!(result.common_ancestor_id.as_ref(), Some(&JsString::from("2")));
    assert_eq!(
        result.entries.iter().filter_map(SessionEntry::id).collect::<Vec<_>>(),
        ["3", "4"]
    );
}

#[test]
fn returns_no_entries_when_there_was_no_previous_leaf() {
    let session = SessionManager::in_memory(Some(&JsString::from("/tmp")), None).expect("session");
    let result = collect_entries_for_branch_summary(&session, None, &JsString::from("target"));
    assert!(result.entries.is_empty());
    assert_eq!(result.common_ancestor_id, None);
}

// ---------------------------------------------------------------------------
// Branch preparation
// ---------------------------------------------------------------------------

#[test]
fn prepares_all_branch_messages_without_a_budget() {
    let mut factory = EntryFactory::new();
    let entries = vec![
        factory.message(&user("explore")),
        factory.message(&tool_call_message("read", "call-1", json!({"path": "src/a.ts"}))),
        factory.message(&tool_result_message("call-1", "contents")),
    ];

    let preparation = prepare_branch_entries(&entries, 0.0);
    assert_eq!(
        preparation.messages.iter().map(AgentMessage::role).collect::<Vec<_>>(),
        ["user", "assistant"]
    );
    assert_eq!(preparation.total_tokens, 2.0 + 6.0);
    assert!(preparation.file_ops.read.contains("src/a.ts"));
}

#[test]
fn carries_branch_summary_details_and_compaction_entries_into_preparation() {
    let mut factory = EntryFactory::new();
    let entries = vec![
        factory.branch_summary(
            "earlier branch",
            Some(json!({"readFiles": ["old-read.ts"], "modifiedFiles": ["old-edit.ts"]})),
        ),
        factory.compaction("compacted", Some(12.5)),
    ];

    let preparation = prepare_branch_entries(&entries, 0.0);
    assert_eq!(
        preparation.messages.iter().map(AgentMessage::role).collect::<Vec<_>>(),
        ["branchSummary", "compactionSummary"]
    );
    assert!(preparation.file_ops.read.contains("old-read.ts"));
    assert!(preparation.file_ops.edited.contains("old-edit.ts"));

    let AgentMessage::Custom(summary) = &preparation.messages[0] else {
        panic!("branch summary message must be custom");
    };
    assert_eq!(
        summary.get("summary").and_then(JsValue::as_string),
        Some(&JsString::from("earlier branch"))
    );
    let AgentMessage::Custom(compaction_message) = &preparation.messages[1] else {
        panic!("compaction summary message must be custom");
    };
    assert_eq!(
        compaction_message.get("durationMs").and_then(JsValue::as_f64),
        Some(12.5)
    );
}

#[test]
fn ignores_hook_generated_branch_summary_details() {
    let mut factory = EntryFactory::new();
    let entries = vec![factory.branch_summary_from_hook("hook summary")];
    let preparation = prepare_branch_entries(&entries, 0.0);
    assert!(preparation.file_ops.read.is_empty());
    assert!(preparation.file_ops.edited.is_empty());
}

#[test]
fn includes_custom_message_entries_in_branch_messages() {
    let mut factory = EntryFactory::new();
    let entries = vec![factory.custom_message("custom context")];
    let preparation = prepare_branch_entries(&entries, 0.0);
    assert_eq!(preparation.messages.len(), 1);
    let AgentMessage::Custom(message) = &preparation.messages[0] else {
        panic!("custom message must be custom");
    };
    assert_eq!(
        message.get("customType").and_then(JsValue::as_string),
        Some(&JsString::from("test"))
    );
}

#[test]
fn respects_the_token_budget_when_selecting_branch_messages() {
    let mut factory = EntryFactory::new();
    let entries = vec![
        factory.message(&user("old user")),
        factory.message(&assistant_text("old assistant")),
        factory.message(&user(&"x".repeat(40_000))),
        factory.message(&assistant_text(&"x".repeat(40_000))),
        factory.message(&user("latest user")),
    ];

    let preparation = prepare_branch_entries(&entries, 20_000.0);
    assert_eq!(preparation.messages.len(), 2);
    assert_eq!(preparation.messages[0].role(), "assistant");
    assert_eq!(preparation.messages[1].role(), "user");
    assert_eq!(preparation.total_tokens, 10_003.0);
}

#[test]
fn keeps_a_summary_entry_when_it_fits_below_ninety_percent_of_the_budget() {
    let mut factory = EntryFactory::new();
    let entries = vec![
        factory.branch_summary(&"x".repeat(400), None),
        factory.message(&user("a")),
    ];

    let preparation = prepare_branch_entries(&entries, 10.0);
    assert_eq!(
        preparation.messages.iter().map(AgentMessage::role).collect::<Vec<_>>(),
        ["branchSummary", "user"]
    );
    assert_eq!(preparation.total_tokens, 101.0);
}

// ---------------------------------------------------------------------------
// Branch summary generation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn returns_no_content_when_there_are_no_messages() {
    let model = model(128_000.0);
    let result = generate_branch_summary(&[], &options(&model, None))
        .await
        .expect("branch request");
    assert_eq!(
        result.summary.as_ref(),
        Some(&JsString::from("No content to summarize"))
    );
    assert_eq!(
        serde_json::to_value(&result).unwrap(),
        json!({"summary": "No content to summarize"})
    );
}

#[tokio::test]
async fn generates_a_preambled_summary_with_file_lists() {
    let mut factory = EntryFactory::new();
    let entries = vec![
        factory.message(&user("explore")),
        factory.message(&tool_call_message("edit", "call-1", json!({"path": "src/a.ts"}))),
    ];
    let log = Arc::new(Mutex::new(Vec::new()));
    let model = model(128_000.0);
    let stream_fn = stream_fn(Arc::clone(&log), response("## Goal\nExplore"));

    let result = generate_branch_summary(&entries, &options(&model, Some(&stream_fn)))
        .await
        .expect("branch request");

    assert_eq!(
        result.summary.as_ref(),
        Some(JsString::from("The user explored a different conversation branch before returning here.\nSummary of that exploration:\n\n## Goal\nExplore\n\n<modified-files>\nsrc/a.ts\n</modified-files>")).as_ref()
    );
    assert!(serde_json::to_value(&result).unwrap().get("usage").is_none());
    assert_eq!(result.read_files, Some(Vec::new()));
    assert_eq!(result.modified_files, Some(vec![JsString::from("src/a.ts")]));

    let calls = log.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].max_tokens, Some(2048.0));
    assert_eq!(calls[0].cache_retention, Some(pi_ai::types::CacheRetention::None));
    assert_eq!(
        calls[0].system_prompt.as_ref(),
        Some(&JsString::from(
            pi_coding_agent::core::compaction::SUMMARIZATION_SYSTEM_PROMPT
        ))
    );
    assert!(calls[0].prompt.starts_with("<conversation>\n[User]: explore\n\n[Assistant tool calls]: edit(path=\"src/a.ts\")\n</conversation>\n\nCreate a structured summary"));
}

#[tokio::test]
async fn appends_custom_instructions_to_the_default_prompt() {
    let mut factory = EntryFactory::new();
    let entries = vec![factory.message(&user("explore"))];
    let log = Arc::new(Mutex::new(Vec::new()));
    let model = model(128_000.0);
    let stream_fn = stream_fn(Arc::clone(&log), response("summary"));
    let mut options = options(&model, Some(&stream_fn));
    let instructions = JsString::from("focus on files");
    options.custom_instructions = Some(&instructions);

    let _ = generate_branch_summary(&entries, &options)
        .await
        .expect("branch request");
    let calls = log.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    assert!(calls[0].prompt.ends_with("Keep each section concise. Preserve exact file paths, function names, and error messages.\n\nAdditional focus: focus on files"));
}

#[tokio::test]
async fn replaces_the_default_prompt_when_requested() {
    let mut factory = EntryFactory::new();
    let entries = vec![factory.message(&user("explore"))];
    let log = Arc::new(Mutex::new(Vec::new()));
    let model = model(128_000.0);
    let stream_fn = stream_fn(Arc::clone(&log), response("summary"));
    let mut options = options(&model, Some(&stream_fn));
    let instructions = JsString::from("only this");
    options.custom_instructions = Some(&instructions);
    options.replace_instructions = true;

    let _ = generate_branch_summary(&entries, &options)
        .await
        .expect("branch request");
    let calls = log.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    assert!(calls[0].prompt.ends_with("</conversation>\n\nonly this"));
}

#[tokio::test]
async fn reports_an_aborted_provider_response() {
    let mut factory = EntryFactory::new();
    let entries = vec![factory.message(&user("explore"))];
    let log = Arc::new(Mutex::new(Vec::new()));
    let model = model(128_000.0);
    let stream_fn = stream_fn(Arc::clone(&log), failure(StopReason::Aborted, ""));

    let result = generate_branch_summary(&entries, &options(&model, Some(&stream_fn)))
        .await
        .expect("branch request");
    assert_eq!(result.aborted, Some(true));
    assert_eq!(result.summary, None);
}

#[tokio::test]
async fn reports_provider_errors_with_a_fallback_message() {
    let mut factory = EntryFactory::new();
    let entries = vec![factory.message(&user("explore"))];
    let log = Arc::new(Mutex::new(Vec::new()));
    let model = model(128_000.0);
    let stream_fn = stream_fn(Arc::clone(&log), failure(StopReason::Error, ""));

    let result = generate_branch_summary(&entries, &options(&model, Some(&stream_fn)))
        .await
        .expect("branch request");
    assert_eq!(result.error.as_ref(), Some(&JsString::from("Summarization failed")));
}
