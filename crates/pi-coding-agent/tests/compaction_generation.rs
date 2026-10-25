//! Behavioral tests for compaction summary generation, materialization and `compact`.

use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use std::sync::{Arc, Mutex};

use pi_agent_core::agent::{AgentMessage, StreamFn};
use pi_ai::request::SimpleStreamOptions;
use pi_ai::types::{
    AssistantContent, AssistantMessage, Context, Message, Model, ProviderNativeContext, ProviderNativeFormat,
    StopReason, TextContent, Usage, UsageCost, UserContent, UserMessage, UserRole,
};
use pi_coding_agent::core::compaction::{
    CompactionPreparation, CompactionSettings, CompactionSourceType, CompactionSummaryOptions, GenerateSummaryOptions,
    MaterializeCompactionSummaryOptions, MaterializeCompactionSummaryResult, compact, generate_summary,
    materialize_compaction_summary,
};
use pi_coding_agent::core::session_manager::CompactionEntry;
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

fn model() -> Model {
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
        context_window: 200_000.0,
        auto_compaction_threshold: None,
        max_tokens: 8192.0,
        headers: None,
        compat: None,
    }
}

fn response(stop_reason: StopReason, text: &str, error_message: Option<&str>) -> AssistantMessage {
    let mut message = AssistantMessage::empty("anthropic-messages", "anthropic", "claude-sonnet-4-5", 0.0);
    message.content = vec![AssistantContent::Text(TextContent::new(text))];
    message.usage = Usage {
        input: 10.0,
        output: 10.0,
        total_tokens: 20.0,
        cost: UsageCost::default(),
        ..Usage::default()
    };
    message.stop_reason = stop_reason;
    message.error_message = error_message.map(str::to_owned).map(JsString::from);
    message
}

#[derive(Debug, Clone, PartialEq)]
struct CapturedCall {
    system_prompt: Option<JsString>,
    prompt: JsString,
    max_tokens: Option<f64>,
    cache_retention: Option<pi_ai::types::CacheRetention>,
    session_id_is_set: bool,
    api_key: Option<JsString>,
}

fn first_user_text(context: &Context) -> JsString {
    let texts = context
        .messages
        .iter()
        .filter_map(|message| match message {
            Message::User(user) => Some(match &user.content {
                UserContent::Text(text) => text.clone(),
                UserContent::Blocks(blocks) => JsString::join(
                    blocks.iter().filter_map(|block| match block {
                        pi_ai::types::TextOrImageContent::Text(text) => Some(&text.text),
                        pi_ai::types::TextOrImageContent::Image(_) => None,
                    }),
                    "",
                ),
            }),
            _ => None,
        })
        .collect::<Vec<_>>();
    JsString::join(texts, "\n")
}

fn scripted_stream(log: Arc<Mutex<Vec<CapturedCall>>>, responses: Vec<AssistantMessage>) -> StreamFn {
    let responses = Arc::new(Mutex::new(responses));
    Arc::new(move |_model, context, options: SimpleStreamOptions| {
        log.lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(CapturedCall {
                system_prompt: context.system_prompt.clone(),
                prompt: first_user_text(&context),
                max_tokens: options.stream.max_tokens,
                cache_retention: options.stream.cache_retention,
                session_id_is_set: options.stream.session_id.is_some(),
                api_key: options.stream.request.api_key.clone(),
            });
        let message = responses
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(0);
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

fn compaction_entry(provider_native: Option<ProviderNativeContext>) -> CompactionEntry {
    CompactionEntry {
        id: JsString::from("compaction-1"),
        parent_id: Some(JsString::from("message-1")),
        timestamp: JsString::from("2026-08-08T00:00:00.000Z"),
        summary: JsString::from("OpenAI native compaction stored in session entry details."),
        first_kept_entry_id: JsString::from("message-2"),
        tokens_before: 42_000.0,
        extra: Default::default(),
        duration_ms: None,
        details: Some(Box::new(JsValue::from(json!({"type": "openai-remote-compaction"})))),
        provider_native: provider_native
            .map(|value| Box::new(pi_js_compat::js_json::to_value(&value).expect("provider native context"))),
        from_hook: None,
        first_kept_entry_index: None,
    }
}

fn native_context() -> ProviderNativeContext {
    ProviderNativeContext {
        provider: "openai-codex".to_owned(),
        api: "openai-codex-responses".to_owned(),
        format: ProviderNativeFormat::OpenAIResponsesInput,
        value: JsValue::from(json!([{ "type": "compaction_summary", "encrypted_content": "encrypted-checkpoint" }])),
    }
}

fn preparation() -> CompactionPreparation {
    let mut file_ops = pi_coding_agent::core::compaction::create_file_ops();
    file_ops.read.insert(JsString::from("read.ts"));
    file_ops.edited.insert(JsString::from("edited.ts"));
    CompactionPreparation {
        first_kept_entry_id: JsString::from("kept-1"),
        messages_to_summarize: vec![user("Summarize this.")],
        turn_prefix_messages: Vec::new(),
        is_split_turn: false,
        tokens_before: 42_000.0,
        previous_summary: None,
        file_ops,
        settings: CompactionSettings {
            threshold_percent: None,
            enabled: true,
            reserve_tokens: 2000.0,
            keep_recent_tokens: 20_000.0,
        },
    }
}

#[tokio::test]
async fn materializes_native_context_and_returns_the_summary() {
    let model = model();
    let entry = compaction_entry(Some(native_context()));
    let log = Arc::new(Mutex::new(Vec::new()));
    let stream_fn = scripted_stream(
        Arc::clone(&log),
        vec![response(
            StopReason::Stop,
            "## Goal\n\nContinue from the encrypted checkpoint.",
            None,
        )],
    );

    let result = materialize_compaction_summary(
        &entry,
        &model,
        &MaterializeCompactionSummaryOptions {
            reserve_tokens: 16_384.0,
            api_key: Some(JsString::from("test-key")),
            headers: None,
            env: None,
            signal: None,
            stream_fn: Some(&stream_fn),
        },
    )
    .await
    .expect("materialization succeeds");

    assert_eq!(
        result,
        MaterializeCompactionSummaryResult::Summary(JsString::from(
            "## Goal\n\nContinue from the encrypted checkpoint."
        ))
    );
    let calls = log.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].api_key.as_ref(), Some(&JsString::from("test-key")));
    assert_eq!(calls[0].max_tokens, Some(8192.0));
    assert_eq!(calls[0].cache_retention, Some(pi_ai::types::CacheRetention::None));
    assert!(!calls[0].session_id_is_set);
    assert!(calls[0].prompt.contains("complete plaintext continuation summary"));
    assert!(calls[0].prompt.contains("Return only the summary text"));
}

#[tokio::test]
async fn materialization_reports_aborted_responses() {
    let model = model();
    let entry = compaction_entry(Some(native_context()));
    let log = Arc::new(Mutex::new(Vec::new()));
    let stream_fn = scripted_stream(Arc::clone(&log), vec![response(StopReason::Aborted, "", None)]);

    let result = materialize_compaction_summary(
        &entry,
        &model,
        &MaterializeCompactionSummaryOptions {
            reserve_tokens: 16_384.0,
            api_key: Some(JsString::from("test-key")),
            headers: None,
            env: None,
            signal: None,
            stream_fn: Some(&stream_fn),
        },
    )
    .await
    .expect("aborted materialization is not an error");
    assert_eq!(result, MaterializeCompactionSummaryResult::Aborted);
}

#[tokio::test]
async fn materialization_requires_a_native_checkpoint() {
    let error = materialize_compaction_summary(
        &compaction_entry(None),
        &model(),
        &MaterializeCompactionSummaryOptions {
            reserve_tokens: 16_384.0,
            api_key: None,
            headers: None,
            env: None,
            signal: None,
            stream_fn: None,
        },
    )
    .await
    .expect_err("missing checkpoint must fail");
    assert_eq!(
        error.message(),
        "Compaction entry compaction-1 has no provider-native checkpoint"
    );
}

#[tokio::test]
async fn materialization_fails_without_summary_text() {
    let entry = compaction_entry(Some(native_context()));
    let log = Arc::new(Mutex::new(Vec::new()));
    let stream_fn = scripted_stream(Arc::clone(&log), vec![response(StopReason::Stop, "   ", None)]);

    let error = materialize_compaction_summary(
        &entry,
        &model(),
        &MaterializeCompactionSummaryOptions {
            reserve_tokens: 16_384.0,
            api_key: None,
            headers: None,
            env: None,
            signal: None,
            stream_fn: Some(&stream_fn),
        },
    )
    .await
    .expect_err("empty summary must fail");
    assert_eq!(error.message(), "Compaction summary materialization returned no text");
}

#[tokio::test]
async fn generates_a_summary_with_the_exact_request_shape() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let mut answer = response(StopReason::Stop, "Summary", None);
    answer
        .content
        .insert(0, AssistantContent::Text(TextContent::new(" \t\n")));
    answer
        .content
        .push(AssistantContent::Text(TextContent::new("\u{feff}")));
    let stream_fn = scripted_stream(Arc::clone(&log), vec![answer]);
    let model = model();
    let messages = vec![user("Summarize this.")];

    let result = generate_summary(&GenerateSummaryOptions {
        current_messages: &messages,
        model: &model,
        reserve_tokens: 2000.0,
        api_key: Some(JsString::from("test-key")),
        headers: None,
        signal: None,
        custom_instructions: None,
        previous_summary: None,
        thinking_level: None,
        stream_fn: Some(&stream_fn),
        env: None,
    })
    .await
    .expect("summary succeeds");

    assert_eq!(result, "Summary");
    let calls = log.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    assert_eq!(calls[0].max_tokens, Some(1600.0));
    assert_eq!(
        calls[0].system_prompt.as_ref(),
        Some(&JsString::from(
            pi_coding_agent::core::compaction::SUMMARIZATION_SYSTEM_PROMPT
        ))
    );
    assert_eq!(
        calls[0].prompt,
        "<conversation>\n[User]: Summarize this.\n</conversation>\n\nThe messages above are a conversation to summarize. Create a structured context checkpoint summary that another LLM will use to continue the work.\n\nUse this EXACT format:\n\n## Goal\n[What is the user trying to accomplish? Can be multiple items if the session covers different tasks.]\n\n## Constraints & Preferences\n- [Any constraints, preferences, or requirements mentioned by user]\n- [Or \"(none)\" if none were mentioned]\n\n## Progress\n### Done\n- [x] [Completed tasks/changes]\n\n### In Progress\n- [ ] [Current work]\n\n### Blocked\n- [Issues preventing progress, if any]\n\n## Key Decisions\n- **[Decision]**: [Brief rationale]\n\n## Next Steps\n1. [Ordered list of what should happen next]\n\n## Critical Context\n- [Any data, examples, or references needed to continue]\n- [Or \"(none)\" if not applicable]\n\nKeep each section concise. Preserve exact file paths, function names, and error messages.\n\nContinuation rules:\n- Deduplicate repeated or semantically equivalent content. Represent each fact, message, and tool result once.\n- Derive ongoing work from the actual conversation, including research and discussion; do not invent a fixed session purpose.\n- Preserve pending user questions and requests, user decisions and corrections, relevant context, and unfinished work.\n- Distinguish completed historical work from ongoing work; do not revive it as a current task.\n- Do not infer the current task or authority from the working directory or messages addressed to another session.\n- Represent ambiguity or missing intent explicitly rather than inventing a goal, assignment, or authorization.\n- Preserve user-stated goals and constraints, reasons for decisions, current status, unresolved blockers, next steps, and exact artifacts needed to continue.\n- Prefer the latest confirmed state over superseded state. Record unresolved contradictions explicitly.\n- Do not invent completion, evidence, file changes, commands, test results, or deployment state.\n- Prioritize the current task, then parked tasks, then only earlier history needed to continue."
    );
}

#[tokio::test]
async fn uses_the_update_prompt_with_a_previous_summary_and_custom_focus() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let stream_fn = scripted_stream(Arc::clone(&log), vec![response(StopReason::Stop, "Summary", None)]);
    let model = model();
    let messages = vec![user("Summarize this.")];

    let _ = generate_summary(&GenerateSummaryOptions {
        current_messages: &messages,
        model: &model,
        reserve_tokens: 2000.0,
        api_key: None,
        headers: None,
        signal: None,
        custom_instructions: Some(&JsString::from("focus")),
        previous_summary: Some(&JsString::from("previous")),
        thinking_level: None,
        stream_fn: Some(&stream_fn),
        env: None,
    })
    .await
    .expect("summary succeeds");

    let calls = log.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    assert!(
        calls[0]
            .prompt
            .contains("<previous-summary>\nprevious\n</previous-summary>\n\n")
    );
    assert!(
        calls[0]
            .prompt
            .ends_with("\n\nAdditional compaction focus requested by the user:\nfocus")
    );
}

#[tokio::test]
async fn returns_a_transient_summary_failure_without_retrying() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let stream_fn = scripted_stream(
        Arc::clone(&log),
        vec![
            response(StopReason::Error, "", Some("terminated")),
            response(StopReason::Stop, "Recovered summary", None),
        ],
    );
    let model = model();
    let messages = vec![user("Summarize this.")];

    let result = generate_summary(&GenerateSummaryOptions {
        current_messages: &messages,
        model: &model,
        reserve_tokens: 2000.0,
        api_key: Some(JsString::from("test-key")),
        headers: None,
        signal: None,
        custom_instructions: None,
        previous_summary: None,
        thinking_level: None,
        stream_fn: Some(&stream_fn),
        env: None,
    })
    .await
    .expect_err("frozen summary must not retry");

    assert_eq!(result.message(), "Summarization failed: terminated");
    let calls = log.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    assert_eq!(calls.len(), 1);
}

#[tokio::test]
async fn reports_summarization_failures_with_provider_detail() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let stream_fn = scripted_stream(Arc::clone(&log), vec![response(StopReason::Error, "", Some("boom"))]);
    let model = model();
    let messages = vec![user("Summarize this.")];

    let error = generate_summary(&GenerateSummaryOptions {
        current_messages: &messages,
        model: &model,
        reserve_tokens: 2000.0,
        api_key: None,
        headers: None,
        signal: None,
        custom_instructions: None,
        previous_summary: None,
        thinking_level: None,
        stream_fn: Some(&stream_fn),
        env: None,
    })
    .await
    .expect_err("error response must fail");
    assert_eq!(error.message(), "Summarization failed: boom");
}

#[tokio::test]
async fn compacts_with_file_operations_and_source_details() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let stream_fn = scripted_stream(Arc::clone(&log), vec![response(StopReason::Stop, "Summary", None)]);
    let model = model();

    let result = compact(
        &preparation(),
        &model,
        &CompactionSummaryOptions {
            model: &model,
            api_key: None,
            headers: None,
            custom_instructions: None,
            signal: None,
            thinking_level: None,
            stream_fn: Some(&stream_fn),
            env: None,
        },
    )
    .await
    .expect("compaction succeeds");

    assert_eq!(
        result.summary,
        "Summary\n\n<read-files>\nread.ts\n</read-files>\n\n<modified-files>\nedited.ts\n</modified-files>"
    );
    assert_eq!(result.first_kept_entry_id, "kept-1");
    assert_eq!(result.tokens_before, 42_000.0);
    assert!(result.duration_ms.is_some());
    let source = result.source.as_ref().expect("source");
    assert_eq!(source.r#type, CompactionSourceType::Local);
    assert_eq!(source.provider, "anthropic");
    assert_eq!(source.model, "claude-sonnet-4-5");
    assert!(serde_json::to_value(&result).unwrap().get("usage").is_none());
    assert_eq!(
        result.details,
        (Some(json!({"readFiles": ["read.ts"], "modifiedFiles": ["edited.ts"]}))).map(JsValue::from)
    );
}

#[tokio::test]
async fn compact_rejects_an_empty_first_kept_entry_id() {
    let mut preparation = preparation();
    preparation.first_kept_entry_id = JsString::default();
    let log = Arc::new(Mutex::new(Vec::new()));
    let stream_fn = scripted_stream(Arc::clone(&log), vec![response(StopReason::Stop, "Summary", None)]);
    let model = model();

    let error = compact(
        &preparation,
        &model,
        &CompactionSummaryOptions {
            model: &model,
            api_key: None,
            headers: None,
            custom_instructions: None,
            signal: None,
            thinking_level: None,
            stream_fn: Some(&stream_fn),
            env: None,
        },
    )
    .await
    .expect_err("missing id must fail");
    assert_eq!(
        error.message(),
        "First kept entry has no UUID - session may need migration"
    );
}

#[tokio::test]
async fn split_turn_compaction_combines_history_and_prefix_summaries() {
    let mut preparation = preparation();
    preparation.is_split_turn = true;
    preparation.turn_prefix_messages = vec![user("turn prefix")];
    let log = Arc::new(Mutex::new(Vec::new()));
    let stream_fn = scripted_stream(
        Arc::clone(&log),
        vec![
            response(StopReason::Stop, "History summary", None),
            response(StopReason::Stop, "Prefix summary", None),
        ],
    );
    let model = model();

    let result = compact(
        &preparation,
        &model,
        &CompactionSummaryOptions {
            model: &model,
            api_key: None,
            headers: None,
            custom_instructions: None,
            signal: None,
            thinking_level: None,
            stream_fn: Some(&stream_fn),
            env: None,
        },
    )
    .await
    .expect("split compaction succeeds");

    assert!(
        result
            .summary
            .starts_with("History summary\n\n---\n\n**Turn Context (split turn):**\n\nPrefix summary\n\n<read-files>")
    );
    let calls = log.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[1].max_tokens, Some(1000.0));
    assert!(
        calls[1]
            .prompt
            .contains("This is the PREFIX of a turn that was too large to keep.")
    );
}

#[test]
fn value_round_trips_through_serde() {
    let value: Value = serde_json::to_value(CompactionSourceType::OpenAiRemote).expect("serialize");
    assert_eq!(value, json!("openai_remote"));
}
