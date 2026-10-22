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
