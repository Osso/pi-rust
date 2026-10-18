use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use pi_agent_core::agent::AgentMessage;
use pi_agent_core::harness::BranchSummaryErrorCode;
use pi_agent_core::harness::compaction::branch_summarization::{
    BranchSummaryDetails, CollectEntriesResult, GenerateBranchSummaryOptions, collect_entries_for_branch_summary,
    generate_branch_summary, prepare_branch_entries,
};
use pi_agent_core::harness::session::{
    Entry, EntryBody, IdGenerator, InMemorySessionStorage, OptionalValue, Session, SessionMetadata,
};
use pi_ai::models::{CreateModelsOptions, MutableModels, create_models};
use pi_ai::providers::faux::{
    FauxAssistantMessageOptions, FauxModelDefinition, FauxProviderHandle, FauxProviderStateSnapshot, FauxResponseStep,
    RegisterFauxProviderOptions, faux_assistant_message, faux_provider,
};
use pi_ai::types::{
    AssistantContent, AssistantMessage, CacheRetention, Context, Message, Model, StopReason, TextContent,
    TextOrImageContent, ToolCall, ToolResultMessage, ToolResultRole, Usage, UsageCost, UserContent, UserMessage,
    UserRole,
};
use pi_ai::utils::retry::RetryPolicy;
use serde_json::{Value, json};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn message(text: &str) -> AgentMessage {
    AgentMessage::User(UserMessage {
        role: UserRole::Tag,
        content: UserContent::Blocks(vec![TextOrImageContent::Text(TextContent::new(text))]),
        input_source: None,
        provider_native: None,
        timestamp: 1,
    })
}

fn usage() -> Usage {
    Usage {
        input: 0.0,
        output: 0.0,
        cache_read: 0.0,
        cache_write: 0.0,
        cache_write_1h: None,
        reasoning: None,
        total_tokens: 0.0,
        cost: UsageCost::default(),
    }
}

fn assistant_message_with(content: Vec<AssistantContent>) -> AgentMessage {
    let mut message = AssistantMessage::empty("anthropic-messages", "anthropic", "claude-sonnet-4-5", 1);
    message.content = content;
    message.usage = usage();
    message.stop_reason = StopReason::Stop;
    AgentMessage::Assistant(Box::new(message))
}

fn assistant_text(text: &str) -> AgentMessage {
    assistant_message_with(vec![AssistantContent::Text(TextContent::new(text))])
}

fn tool_call_message(name: &str, id: &str, arguments: Value) -> AgentMessage {
    assistant_message_with(vec![AssistantContent::ToolCall(ToolCall::new(
        id,
        name,
        arguments.as_object().cloned().expect("arguments must be an object"),
    ))])
}

fn tool_result_message(tool_call_id: &str, text: &str) -> AgentMessage {
    AgentMessage::ToolResult(ToolResultMessage {
        role: ToolResultRole::Tag,
        tool_call_id: tool_call_id.to_owned(),
        tool_name: "read".to_owned(),
        content: vec![TextOrImageContent::Text(TextContent::new(text))],
        details: None,
        usage: None,
        added_tool_names: None,
        is_error: false,
        timestamp: 1,
    })
}

struct EntryFactory {
    next_id: i64,
}

impl EntryFactory {
    fn new() -> Self {
        Self { next_id: 0 }
    }

    fn identity(&mut self) -> (String, i64) {
        let id = format!("entry-{}", self.next_id);
        self.next_id += 1;
        (id, self.next_id)
    }

    fn message(&mut self, message: AgentMessage) -> Entry {
        let (id, seq) = self.identity();
        Entry {
            body: EntryBody::Message {
                id,
                message,
                terminate: None,
            },
            parent_id: None,
            seq,
            timestamp: seq,
        }
    }

    fn branch_summary(&mut self, summary: &str, details: Option<Value>) -> Entry {
        let (id, seq) = self.identity();
        Entry {
            body: EntryBody::BranchSummary {
                id,
                from_id: "branch".to_owned(),
                summary: summary.to_owned(),
                details: details.map_or(OptionalValue::Missing, OptionalValue::Present),
                usage: None,
            },
            parent_id: None,
            seq,
            timestamp: seq,
        }
    }

    fn compaction(&mut self, summary: &str, duration_ms: Option<f64>) -> Entry {
        let (id, seq) = self.identity();
        Entry {
            body: EntryBody::Compaction {
                id,
                summary: summary.to_owned(),
                tokens_before: 123.0,
                retained_tail: Vec::new(),
                duration_ms,
                details: OptionalValue::Missing,
                usage: None,
            },
            parent_id: None,
            seq,
            timestamp: seq,
        }
    }
}

struct CountingIdGenerator {
    next: Mutex<i64>,
}

impl CountingIdGenerator {
    fn new() -> Self {
        Self { next: Mutex::new(0) }
    }
}

impl IdGenerator for CountingIdGenerator {
    fn next(&self) -> String {
        let mut next = self.next.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        *next += 1;
        format!("entry-{next}")
    }
}

fn session() -> Session<SessionMetadata> {
    Session::with_id_generator(
        Arc::new(InMemorySessionStorage::new(SessionMetadata {
            id: "session".to_owned(),
            created_at: 1,
            parent_session_id: None,
        })),
        Arc::new(CountingIdGenerator::new()),
    )
}

// ---------------------------------------------------------------------------
// Branch collection
// ---------------------------------------------------------------------------

#[tokio::test]
async fn collects_the_abandoned_side_of_a_branch_in_chronological_order() {
    let session = session();
    let root_id = session.append_message(message("root")).await.expect("append root");
    let common_id = session.append_message(message("common")).await.expect("append common");
    let abandoned_ids = [
        session.append_message(message("abandoned 1")).await.expect("append 1"),
        session.append_message(message("abandoned 2")).await.expect("append 2"),
    ];
    session
        .create_lane("target", Some(&common_id))
        .await
        .expect("create lane");
    let target_id = session
        .view("target")
        .append_message(message("target"))
        .await
        .expect("append target");

    let result = collect_entries_for_branch_summary(&session, Some(&abandoned_ids[1]), &target_id)
        .await
        .expect("collection must succeed");

    assert_eq!(result.common_ancestor_id, Some(common_id));
    assert_eq!(
        result.entries.iter().map(|entry| entry.id()).collect::<Vec<_>>(),
        abandoned_ids.iter().map(String::as_str).collect::<Vec<_>>()
    );
    assert!(!result.entries.iter().any(|entry| entry.id() == root_id));
}

#[tokio::test]
async fn returns_no_entries_when_there_was_no_previous_leaf() {
    let session = session();
    let target_id = session.append_message(message("target")).await.expect("append target");

    assert_eq!(
        collect_entries_for_branch_summary(&session, None, &target_id)
            .await
            .expect("collection must succeed"),
        CollectEntriesResult {
            entries: Vec::new(),
            common_ancestor_id: None,
        }
    );
}

// ---------------------------------------------------------------------------
// Branch preparation
// ---------------------------------------------------------------------------

#[test]
fn prepares_all_branch_messages_without_a_budget() {
    let mut factory = EntryFactory::new();
    let user = factory.message(message("explore"));
    let assistant = factory.message(tool_call_message("read", "call-1", json!({"path": "src/a.ts"})));
    let tool_result = factory.message(tool_result_message("call-1", "contents"));
    let entries = vec![user, assistant, tool_result];

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
    let branch = factory.branch_summary(
        "earlier branch",
        Some(json!({"readFiles": ["old-read.ts"], "modifiedFiles": ["old-edit.ts"]})),
    );
    let compaction = factory.compaction("compacted", Some(12.5));
    let entries = vec![branch, compaction];

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
    assert_eq!(summary.get("summary").and_then(Value::as_str), Some("earlier branch"));
    let AgentMessage::Custom(compaction_message) = &preparation.messages[1] else {
        panic!("compaction summary message must be custom");
    };
    assert_eq!(compaction_message.get("durationMs").and_then(Value::as_f64), Some(12.5));
}

#[test]
fn respects_the_token_budget_when_selecting_branch_messages() {
    let mut factory = EntryFactory::new();
    let entries = vec![
        factory.message(message("old user")),
        factory.message(assistant_text("old assistant")),
        factory.message(message(&"x".repeat(40_000))),
        factory.message(assistant_text(&"x".repeat(40_000))),
        factory.message(message("latest user")),
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
        factory.message(message("a")),
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

static FAUX_COUNT: AtomicUsize = AtomicUsize::new(0);

fn create_faux_model(reasoning: bool, context_window: f64, max_tokens: f64) -> (FauxProviderHandle, Model) {
    let count = FAUX_COUNT.fetch_add(1, Ordering::SeqCst) + 1;
    let faux = faux_provider(RegisterFauxProviderOptions {
        provider: Some(format!("faux-branch-{count}")),
        models: Some(vec![FauxModelDefinition {
            id: "branch-model".to_owned(),
            reasoning: Some(reasoning),
            context_window: Some(context_window),
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
    prompt: String,
    max_tokens: Option<f64>,
    cache_retention: Option<CacheRetention>,
}

type CallLog = Arc<Mutex<Vec<CapturedCall>>>;

fn scripted_responses(log: CallLog, texts: Vec<&'static str>) -> Vec<FauxResponseStep> {
    texts
        .into_iter()
        .map(|text| {
            let log = Arc::clone(&log);
            FauxResponseStep::Factory(Arc::new(
                move |context: Context,
                      options: Option<pi_ai::request::SimpleStreamOptions>,
                      _snapshot: FauxProviderStateSnapshot,
                      _model: Model| {
                    log.lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .push(CapturedCall {
                            prompt: first_user_text(&context),
                            max_tokens: options.as_ref().and_then(|options| options.stream.max_tokens),
                            cache_retention: options.as_ref().and_then(|options| options.stream.cache_retention),
                        });
                    Box::pin(async move { Ok(faux_assistant_message(text, FauxAssistantMessageOptions::default())) })
                },
            ))
        })
        .collect()
}

fn first_user_text(context: &Context) -> String {
    match context.messages.first() {
        Some(Message::User(user)) => match &user.content {
            UserContent::Text(text) => text.clone(),
            UserContent::Blocks(blocks) => blocks
                .iter()
                .filter_map(|block| match block {
                    TextOrImageContent::Text(text) => Some(text.text.as_str()),
                    TextOrImageContent::Image(_) => None,
                })
                .collect(),
        },
        _ => String::new(),
    }
}

fn scripted_message(text: &str) -> FauxResponseStep {
    FauxResponseStep::message(faux_assistant_message(text, FauxAssistantMessageOptions::default()))
}

fn scripted_failure(stop_reason: StopReason, error_message: &str) -> FauxResponseStep {
    FauxResponseStep::message(faux_assistant_message(
        "",
        FauxAssistantMessageOptions {
            stop_reason: Some(stop_reason),
            error_message: Some(error_message.to_owned()),
            ..Default::default()
        },
    ))
}

fn options<'a>(models: &'a dyn pi_ai::models::Models, model: &'a Model) -> GenerateBranchSummaryOptions<'a> {
    GenerateBranchSummaryOptions {
        models,
        model,
        signal: None,
        custom_instructions: None,
        replace_instructions: false,
        reserve_tokens: None,
        retry: None,
    }
}

fn branch_entries() -> Vec<Entry> {
    let mut factory = EntryFactory::new();
    vec![
        factory.message(message("explore the branch")),
        factory.message(tool_call_message("read", "call-1", json!({"path": "src/a.ts"}))),
        factory.message(tool_result_message("call-1", "contents")),
        factory.message(assistant_text("found it")),
    ]
}

#[tokio::test]
async fn returns_no_content_when_the_branch_has_no_summarizable_entries() {
    let mut factory = EntryFactory::new();
    let entries = vec![factory.message(tool_result_message("call-1", "contents"))];
    let (faux, model) = create_faux_model(false, 200000.0, 8192.0);
    let models = registered_models(&faux);
    faux.set_responses(vec![scripted_message("unused")]);

    let result = generate_branch_summary(&entries, &options(&models, &model))
        .await
        .expect("summary must succeed");
    assert_eq!(result.summary, "No content to summarize");
    assert_eq!(result.usage, None);
    assert!(result.read_files.is_empty());
    assert!(result.modified_files.is_empty());
    assert_eq!(faux.get_pending_response_count(), 1);
}

#[tokio::test]
async fn generates_a_branch_summary_with_the_exact_prompt_and_file_sections() {
    let log: CallLog = Arc::default();
    let (faux, model) = create_faux_model(false, 200000.0, 8192.0);
    let models = registered_models(&faux);
    faux.set_responses(scripted_responses(Arc::clone(&log), vec!["## Goal\nExplore"]));

    let result = generate_branch_summary(&branch_entries(), &options(&models, &model))
        .await
        .expect("summary must succeed");

    let calls = log.lock().unwrap().clone();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].max_tokens, Some(2048.0));
    assert_eq!(calls[0].cache_retention, Some(CacheRetention::None));
    assert!(
        calls[0]
            .prompt
            .starts_with("<conversation>\n[User]: explore the branch\n\n")
    );
    assert!(
        calls[0]
            .prompt
            .contains("[Assistant tool calls]: read(path=\"src/a.ts\")")
    );
    assert!(calls[0].prompt.contains("[Assistant]: found it\n</conversation>\n\n"));
    assert!(
        calls[0]
            .prompt
            .contains("Create a structured summary of this conversation branch")
    );

    assert!(result.summary.starts_with(
        "The user explored a different conversation branch before returning here.\nSummary of that exploration:\n\n## Goal\nExplore"
    ));
    assert!(result.summary.contains("<read-files>\nsrc/a.ts\n</read-files>"));
    assert_eq!(result.read_files, ["src/a.ts"]);
    assert!(result.modified_files.is_empty());
    assert!(result.usage.is_some_and(|usage| usage.total_tokens > 0.0));
}

#[tokio::test]
async fn appends_or_replaces_custom_instructions() {
    let log: CallLog = Arc::default();
    let (faux, model) = create_faux_model(false, 200000.0, 8192.0);
    let models = registered_models(&faux);
    faux.set_responses(scripted_responses(Arc::clone(&log), vec!["first", "second"]));

    let mut appended = options(&models, &model);
    appended.custom_instructions = Some("focus on files");
    generate_branch_summary(&branch_entries(), &appended)
        .await
        .expect("summary must succeed");

    let mut replaced = options(&models, &model);
    replaced.custom_instructions = Some("only this instruction");
    replaced.replace_instructions = true;
    generate_branch_summary(&branch_entries(), &replaced)
        .await
        .expect("summary must succeed");

    let calls = log.lock().unwrap().clone();
    assert!(calls[0].prompt.ends_with("Keep each section concise. Preserve exact file paths, function names, and error messages.\n\nAdditional focus: focus on files"));
    assert!(calls[1].prompt.ends_with("</conversation>\n\nonly this instruction"));
}

#[tokio::test]
async fn honors_the_reserve_tokens_option_when_building_the_budget() {
    let log: CallLog = Arc::default();
    let (faux, model) = create_faux_model(false, 100.0, 8192.0);
    let models = registered_models(&faux);
    faux.set_responses(scripted_responses(Arc::clone(&log), vec!["summary"]));

    let mut configured = options(&models, &model);
    configured.reserve_tokens = Some(200.0);
    let result = generate_branch_summary(&branch_entries(), &configured)
        .await
        .expect("summary must succeed");
    assert!(result.summary.contains("summary"));
    assert_eq!(log.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn returns_branch_summary_errors_for_failed_or_aborted_generations() {
    let (faux, model) = create_faux_model(false, 200000.0, 8192.0);
    let models = registered_models(&faux);
    faux.set_responses(vec![scripted_failure(StopReason::Error, "boom")]);
    let error = generate_branch_summary(&branch_entries(), &options(&models, &model))
        .await
        .expect_err("summary must fail");
    assert_eq!(error.code, BranchSummaryErrorCode::SummarizationFailed);
    assert_eq!(error.message, "Branch summary failed: boom");

    let (aborted_faux, aborted_model) = create_faux_model(false, 200000.0, 8192.0);
    let models = registered_models(&aborted_faux);
    aborted_faux.set_responses(vec![scripted_failure(StopReason::Aborted, "stopped")]);
    let error = generate_branch_summary(&branch_entries(), &options(&models, &aborted_model))
        .await
        .expect_err("summary must fail");
    assert_eq!(error.code, BranchSummaryErrorCode::Aborted);
    assert_eq!(error.message, "stopped");
}

#[tokio::test]
async fn uses_a_retry_policy_for_transient_branch_summary_failures() {
    let (faux, model) = create_faux_model(false, 200000.0, 8192.0);
    let models = registered_models(&faux);
    faux.set_responses(vec![
        scripted_failure(StopReason::Error, "overloaded"),
        scripted_message("recovered"),
    ]);
    let retry = RetryPolicy {
        enabled: true,
        max_retries: 1,
        base_delay_ms: 0.0,
    };
    let mut configured = options(&models, &model);
    configured.retry = Some(&retry);

    let result = generate_branch_summary(&branch_entries(), &configured)
        .await
        .expect("summary must succeed");
    assert!(result.summary.contains("recovered"));
    assert_eq!(faux.get_pending_response_count(), 0);
}

#[test]
fn branch_summary_details_fields_are_publicly_mutable() {
    let mut details = BranchSummaryDetails::default();
    details.read_files.push("a.ts".to_owned());
    details.modified_files.push("b.ts".to_owned());
    assert_eq!(details.read_files, ["a.ts"]);
    assert_eq!(details.modified_files, ["b.ts"]);
}
