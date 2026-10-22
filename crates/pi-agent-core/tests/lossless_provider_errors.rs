use std::sync::Arc;

use pi_agent_core::agent::{AgentContext, AgentError, AgentLoopConfig, AgentMessage, StreamFn, agent_loop};
use pi_agent_core::harness::compaction::{GenerateSummaryOptions, generate_summary};
use pi_ai::models::{CreateModelsOptions, MutableModels, create_models};
use pi_ai::providers::faux::{
    FauxAssistantMessageOptions, FauxResponseStep, RegisterFauxProviderOptions, faux_assistant_message, faux_provider,
};
use pi_ai::types::{AssistantMessage, Model, StopReason};
use pi_ai::utils::event_stream::{StreamFailure, create_assistant_message_event_stream};
use pi_js_compat::js_json::{JsString, JsValue, parse_lossless};

fn raw_error() -> JsString {
    JsString::from("overloaded ") + JsString::from_utf16(vec![0xd800, 0x21, 0xdfff])
}

#[tokio::test]
async fn failed_compaction_keeps_provider_units_and_exact_prefix() {
    let faux = faux_provider(RegisterFauxProviderOptions::default());
    let model = faux.get_model(None).unwrap();
    let models = create_models(CreateModelsOptions::default());
    models.set_provider(Arc::clone(&faux.provider));
    for (reason, prefix) in [(StopReason::Error, "Summarization failed: "), (StopReason::Aborted, "")] {
        faux.set_responses(vec![FauxResponseStep::message(faux_assistant_message(
            "",
            FauxAssistantMessageOptions {
                stop_reason: Some(reason),
                error_message: Some(raw_error()),
                ..Default::default()
            },
        ))]);
        let error = generate_summary(&GenerateSummaryOptions {
            current_messages: &[],
            models: &models,
            model: &model,
            reserve_tokens: 100.0,
            signal: None,
            custom_instructions: None,
            previous_summary: None,
            thinking_level: None,
            retry: None,
        })
        .await
        .unwrap_err();
        assert_eq!(error.message, JsString::from(prefix) + raw_error());
    }
}

#[tokio::test]
async fn native_stream_failure_reaches_agent_result_without_display_projection() {
    let model: Model = serde_json::from_value(serde_json::json!({"id":"probe","name":"probe","api":"openai-responses","provider":"openai","baseUrl":"https://provider.invalid","reasoning":false,"input":["text"],"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},"contextWindow":10000,"maxTokens":1000})).unwrap();
    let stream_fn: StreamFn = Arc::new(|_, _, _| {
        Box::pin(async {
            let (sender, stream) = create_assistant_message_event_stream();
            sender.fail(StreamFailure::new(raw_error()));
            stream
        })
    });
    let config = AgentLoopConfig::new(model, Arc::new(|_| Box::pin(async { Ok(vec![]) })));
    let mut stream = agent_loop(
        vec![],
        AgentContext {
            system_prompt: "".into(),
            messages: vec![],
            tools: None,
        },
        config,
        None,
        Some(stream_fn),
    );
    while stream.next().await.is_some() {}
    assert_eq!(stream.result().await.unwrap_err().message(), &raw_error());
}

#[tokio::test]
async fn agent_failure_keeps_state_listener_and_persisted_message_native() {
    use pi_agent_core::agent::{Agent, AgentEvent, AgentOptions};
    use std::sync::Mutex;
    let agent = Agent::new(AgentOptions {
        stream_fn: Some(Arc::new(|_, _, _| {
            Box::pin(async {
                let (sender, stream) = create_assistant_message_event_stream();
                sender.fail(StreamFailure::new(raw_error()));
                stream
            })
        })),
        ..Default::default()
    })
    .unwrap();
    let observed = Arc::new(Mutex::new(Vec::new()));
    let listener_messages = observed.clone();
    agent.subscribe(Arc::new(move |event, _| {
        if let AgentEvent::MessageEnd {
            message: AgentMessage::Assistant(message),
        } = event
        {
            listener_messages.lock().unwrap().push(message.error_message.clone());
        }
        Box::pin(async { Ok(()) })
    }));
    agent.prompt("start").await.unwrap();
    assert_eq!(agent.error_message(), Some(raw_error()));
    assert_eq!(*observed.lock().unwrap(), vec![Some(raw_error())]);
    let wire = serde_json::to_string(agent.messages().last().unwrap()).unwrap();
    let restored: AgentMessage = serde_json::from_str(&wire).unwrap();
    assert_eq!(restored.as_assistant().unwrap().error_message, Some(raw_error()));
}

#[tokio::test]
async fn compaction_retry_callbacks_keep_raw_provider_messages() {
    use pi_agent_core::harness::compaction::generate_summary_with_usage_with_callbacks;
    use pi_ai::utils::retry::{RetryCallbacks, RetryPolicy};
    #[derive(Default)]
    struct Callbacks {
        scheduled: Vec<JsString>,
        finished: Vec<Option<JsString>>,
    }
    impl RetryCallbacks for Callbacks {
        async fn on_retry_scheduled<'a>(&'a mut self, _: u32, _: u32, _: f64, message: &'a JsString) {
            self.scheduled.push(message.clone());
        }
        async fn on_retry_finished(&mut self, _: bool, _: u32, message: Option<&JsString>) {
            self.finished.push(message.cloned());
        }
    }
    let faux = faux_provider(RegisterFauxProviderOptions::default());
    let model = faux.get_model(None).unwrap();
    let models = create_models(CreateModelsOptions::default());
    models.set_provider(Arc::clone(&faux.provider));
    let failure = faux_assistant_message(
        "",
        FauxAssistantMessageOptions {
            stop_reason: Some(StopReason::Error),
            error_message: Some(raw_error()),
            ..Default::default()
        },
    );
    faux.set_responses(vec![
        FauxResponseStep::message(failure.clone()),
        FauxResponseStep::message(failure),
    ]);
    let policy = RetryPolicy {
        enabled: true,
        max_retries: 1,
        base_delay_ms: 0.0,
    };
    let mut callbacks = Callbacks::default();
    let error = generate_summary_with_usage_with_callbacks(
        &GenerateSummaryOptions {
            current_messages: &[],
            models: &models,
            model: &model,
            reserve_tokens: 100.0,
            signal: None,
            custom_instructions: None,
            previous_summary: None,
            thinking_level: None,
            retry: Some(&policy),
        },
        Some(&mut callbacks),
    )
    .await
    .unwrap_err();
    assert_eq!(callbacks.scheduled, vec![raw_error()]);
    assert_eq!(callbacks.finished, vec![Some(raw_error())]);
    assert_eq!(error.message, JsString::from("Summarization failed: ") + raw_error());
}

#[tokio::test]
async fn branch_failures_preserve_raw_error_and_source_defaults() {
    use pi_agent_core::harness::compaction::branch_summarization::{
        GenerateBranchSummaryOptions, generate_branch_summary,
    };
    use pi_agent_core::harness::session::{Entry, EntryBody};
    let faux = faux_provider(RegisterFauxProviderOptions::default());
    let model = faux.get_model(None).unwrap();
    let models = create_models(CreateModelsOptions::default());
    models.set_provider(Arc::clone(&faux.provider));
    let user: AgentMessage = serde_json::from_str(r#"{"role":"user","content":"branch","timestamp":0}"#).unwrap();
    let entries = [Entry {
        body: EntryBody::Message {
            id: "entry".into(),
            message: user,
            terminate: None,
        },
        parent_id: None,
        seq: 1,
        timestamp: 0,
    }];
    for (reason, detail, expected) in [
        (
            StopReason::Error,
            Some(raw_error()),
            JsString::from("Branch summary failed: ") + raw_error(),
        ),
        (StopReason::Aborted, Some(raw_error()), raw_error()),
        (StopReason::Error, None, "Branch summary failed: Unknown error".into()),
        (StopReason::Aborted, None, "Branch summary aborted".into()),
        (
            StopReason::Error,
            Some(JsString::default()),
            "Branch summary failed: ".into(),
        ),
    ] {
        let mut response = faux_assistant_message("", FauxAssistantMessageOptions::default());
        response.stop_reason = reason;
        response.error_message = detail;
        faux.set_responses(vec![FauxResponseStep::message(response)]);
        let error = generate_branch_summary(
            &entries,
            &GenerateBranchSummaryOptions {
                models: &models,
                model: &model,
                signal: None,
                custom_instructions: None,
                replace_instructions: false,
                reserve_tokens: None,
                retry: None,
            },
        )
        .await
        .unwrap_err();
        assert_eq!(error.message, expected);
    }
}

#[tokio::test]
async fn tool_hook_failures_keep_native_units_in_result_messages() {
    use pi_agent_core::agent::{
        Agent, AgentInitialState, AgentOptions, AgentTool, AgentToolResult, AgentToolUpdateCallback, ToolError,
    };
    use pi_ai::request::BoxFuture;
    use pi_ai::types::{AssistantContent, AssistantMessageEvent, DoneReason, ToolCall};
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Tool {
        schema: serde_json::Value,
    }
    impl AgentTool for Tool {
        fn name(&self) -> &str {
            "probe"
        }
        fn description(&self) -> &str {
            "probe"
        }
        fn label(&self) -> &str {
            "probe"
        }
        fn parameters(&self) -> &serde_json::Value {
            &self.schema
        }
        fn execute(
            &self,
            _: String,
            _: JsValue,
            _: Option<tokio_util::sync::CancellationToken>,
            _: AgentToolUpdateCallback,
            _: Option<pi_agent_core::harness::AgentToolExecutionContext>,
        ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
            Box::pin(async {
                Ok(AgentToolResult {
                    content: vec![],
                    details: None,
                    is_error: None,
                    terminate: None,
                })
            })
        }
    }
    for before in [true, false] {
        let turn = Arc::new(AtomicUsize::new(0));
        let stream_fn: StreamFn = Arc::new(move |model, _, _| {
            let first = turn.fetch_add(1, Ordering::SeqCst) == 0;
            Box::pin(async move {
                let mut message = AssistantMessage::empty(model.api, model.provider, model.id, 0);
                let reason = if first {
                    message.content.push(AssistantContent::ToolCall(ToolCall::new(
                        "call",
                        "probe",
                        pi_js_compat::js_json::JsObject::new(),
                    )));
                    message.stop_reason = StopReason::ToolUse;
                    DoneReason::ToolUse
                } else {
                    DoneReason::Stop
                };
                let (sender, stream) = create_assistant_message_event_stream();
                sender.push(AssistantMessageEvent::Done { reason, message });
                stream
            })
        });
        let mut options = AgentOptions {
            stream_fn: Some(stream_fn),
            initial_state: Some(AgentInitialState {
                tools: Some(vec![Arc::new(Tool {
                    schema: serde_json::json!({"type":"object","properties":{}}),
                })]),
                ..Default::default()
            }),
            ..Default::default()
        };
        if before {
            options.before_tool_call = Some(Arc::new(|_, _| Box::pin(async { Err(AgentError::new(raw_error())) })));
        } else {
            options.after_tool_call = Some(Arc::new(|_, _| Box::pin(async { Err(AgentError::new(raw_error())) })));
        }
        let agent = Agent::new(options).unwrap();
        agent.prompt("probe").await.unwrap();
        let messages = agent.messages();
        let result = messages
            .iter()
            .find_map(|message| {
                if let AgentMessage::ToolResult(result) = message {
                    Some(result)
                } else {
                    None
                }
            })
            .unwrap();
        let wire = parse_lossless(&serde_json::to_string(result).unwrap()).unwrap();
        assert_eq!(
            wire.get("content").unwrap().as_array().unwrap()[0]
                .get("text")
                .and_then(JsValue::as_string),
            Some(&raw_error())
        );
        assert!(result.is_error);
    }
}

#[tokio::test]
async fn provider_failure_survives_jsonl_repo_close_and_reopen() {
    use pi_agent_core::agent::{Agent, AgentOptions};
    use pi_agent_core::harness::session::jsonl::types::NativeFileSystem;
    use pi_agent_core::harness::session::{
        EntryBody, EntryQuery, JsonlSessionCreateOptions, JsonlSessionRepo, JsonlSessionRepoOptions,
    };
    let root = tempfile::tempdir().unwrap();
    let repo = JsonlSessionRepo::with_clock(
        JsonlSessionRepoOptions {
            fs: Arc::new(NativeFileSystem::new(root.path().to_str().unwrap())),
            sessions_root: root.path().to_str().expect("utf-8 test path").to_owned().into(),
        },
        Arc::new(|| 1_700_000_000_000),
    );
    let agent = Agent::new(AgentOptions {
        stream_fn: Some(Arc::new(|_, _, _| {
            Box::pin(async {
                let (sender, stream) = create_assistant_message_event_stream();
                sender.fail(StreamFailure::new(raw_error()));
                stream
            })
        })),
        ..Default::default()
    })
    .unwrap();
    agent.prompt("start").await.unwrap();
    let session = repo
        .create(JsonlSessionCreateOptions {
            id: Some("native-error".into()),
            parent_session_id: None,
            cwd: root.path().to_str().expect("utf-8 test path").to_owned().into(),
            metadata: None,
        })
        .await
        .unwrap();
    session
        .append_entry(
            EntryBody::Message {
                id: "failure".into(),
                message: agent.messages().last().unwrap().clone(),
                terminate: None,
            },
            "main",
        )
        .await
        .unwrap();
    let raw_payload: AgentMessage =
        serde_json::from_str(r#"{"role":"raw-custom","content":"\ud800","\udfff":{"error":"\ud800!\udfff"}}"#).unwrap();
    session
        .append_entry(
            EntryBody::Message {
                id: "raw-payload".into(),
                message: raw_payload.clone(),
                terminate: None,
            },
            "main",
        )
        .await
        .unwrap();
    use pi_agent_core::harness::session::{OperationError, OperationOutcome, RecordBody, RecordQuery};
    session
        .append_record(RecordBody::OperationFinished {
            id: "failed-op".into(),
            lane: "main".into(),
            run_id: "run".into(),
            outcome: OperationOutcome::Failed,
            error: Some(OperationError {
                code: "provider_error".into(),
                message: raw_error(),
            }),
        })
        .await
        .unwrap();
    let metadata = session.get_metadata().await.unwrap();
    drop(session);
    let restored = repo.open(&metadata).await.unwrap();
    let entries = restored.find_entries(EntryQuery::default()).await.unwrap();
    assert_eq!(entries.len(), 2);
    let failure = entries.iter().find(|entry| entry.id() == "failure").unwrap();
    let EntryBody::Message { message, .. } = &failure.body else {
        panic!("persisted failure message")
    };
    assert_eq!(message.as_assistant().unwrap().error_message, Some(raw_error()));
    let payload = entries.iter().find(|entry| entry.id() == "raw-payload").unwrap();
    let EntryBody::Message { message, .. } = &payload.body else {
        panic!("persisted raw custom payload")
    };
    assert_eq!(message, &raw_payload);
    let records = restored.find_records(RecordQuery::default()).await.unwrap();
    let RecordBody::OperationFinished { error: Some(error), .. } = &records[0].body else {
        panic!("persisted provider operation failure")
    };
    assert_eq!(error.message, raw_error());
}

#[test]
fn invalid_provider_native_metadata_is_not_silently_discarded() {
    use pi_agent_core::harness::messages::convert_to_llm;
    let message: AgentMessage = serde_json::from_str(r#"{"role":"compactionSummary","summary":"summary","providerNative":{"provider":"\ud800","api":"openai-responses","format":"openai.responses.input","value":{"error":"\udfff"}},"timestamp":0}"#).unwrap();
    let error = convert_to_llm(&[message]).expect_err("invalid opaque metadata must be rejected explicitly");
    assert!(error.message().starts_with("Invalid provider-native context: "));
}

#[test]
fn provider_native_summary_sink_keeps_original_payload_and_valid_raw_value() {
    use pi_agent_core::harness::messages::{
        CompactionSummaryOptions, convert_to_llm, create_compaction_summary_message,
    };
    use pi_ai::types::Message;
    let native = parse_lossless(r#"{"provider":"openai","api":"openai-responses","format":"openai.responses.input","value":{"\ud800":"\udfff"}}"#).unwrap();
    let message = create_compaction_summary_message(
        "summary",
        0.0,
        0,
        &CompactionSummaryOptions {
            provider_native: Some(native.clone()),
            ..Default::default()
        },
    );
    let wire = parse_lossless(&serde_json::to_string(&message).unwrap()).unwrap();
    assert_eq!(wire.get("providerNative"), Some(&native));
    let output = convert_to_llm(&[message]).unwrap();
    let Message::User(user) = &output[0] else {
        panic!("summary user message")
    };
    assert_eq!(user.provider_native.as_ref().unwrap().value, native["value"]);
}

#[test]
fn persisted_provider_failure_keeps_native_units() {
    let mut message = AssistantMessage::empty("openai-responses", "openai", "probe", 0);
    message.stop_reason = StopReason::Error;
    message.error_message = Some(raw_error());
    let wire = serde_json::to_string(&AgentMessage::Assistant(Box::new(message))).unwrap();
    let parsed = parse_lossless(&wire).unwrap();
    assert_eq!(
        parsed.get("errorMessage").and_then(JsValue::as_string),
        Some(&raw_error())
    );
    assert_eq!(AgentError::new(raw_error()).message(), &raw_error());
}

#[test]
fn custom_type_and_branch_origin_keep_native_units() {
    use pi_agent_core::harness::messages::{
        CustomMessageContent, create_branch_summary_message, create_custom_message,
    };
    let custom = create_custom_message(&raw_error(), CustomMessageContent::Text("body".into()), true, None, 0);
    let branch = create_branch_summary_message("summary", &raw_error(), 0);
    for (message, field) in [(custom, "customType"), (branch, "fromId")] {
        let wire = serde_json::to_string(&message).unwrap();
        assert_eq!(
            parse_lossless(&wire).unwrap().get(field).and_then(JsValue::as_string),
            Some(&raw_error())
        );
    }
}
