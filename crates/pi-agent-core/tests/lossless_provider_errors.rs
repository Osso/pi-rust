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
