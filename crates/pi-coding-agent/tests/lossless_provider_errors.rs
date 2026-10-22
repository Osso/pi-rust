use pi_ai::models::ModelsRefreshResult;
use pi_ai::request::CallbackError;
use pi_coding_agent::core::agent_session::AutoRetryEndEvent;
use pi_coding_agent::core::model_runtime::ModelRuntimeError;
use pi_coding_agent::package_manager_cli::model_catalog_refresh_outcome;
use pi_js_compat::js_json::{JsString, JsValue, parse_lossless};

fn raw_error() -> JsString {
    JsString::from_utf16(vec![0xd800, 0x21, 0xdfff])
}

#[test]
fn model_catalog_failure_keeps_provider_message_units_and_prefix() {
    let mut result = ModelsRefreshResult::default();
    result.errors.insert("probe".into(), CallbackError::new(raw_error()));
    assert_eq!(
        model_catalog_refresh_outcome(result).unwrap_err(),
        JsString::from("Could not refresh model catalogs: probe: ") + raw_error()
    );
}

#[test]
fn model_runtime_message_keeps_raw_resolve_callback() {
    let error = ModelRuntimeError::from(pi_ai::auth::resolve::ResolveError::Callback(CallbackError::new(
        raw_error(),
    )));
    assert_eq!(error.message(), raw_error());
}

#[tokio::test]
async fn coding_compaction_keeps_failed_stream_message_with_source_prefix() {
    use pi_agent_core::agent::StreamFn;
    use pi_ai::providers::faux::{RegisterFauxProviderOptions, faux_provider};
    use pi_ai::utils::event_stream::{StreamFailure, create_assistant_message_event_stream};
    use pi_coding_agent::core::compaction::{GenerateSummaryWithUsageOptions, generate_summary};
    use std::sync::Arc;
    let model = faux_provider(RegisterFauxProviderOptions::default())
        .get_model(None)
        .unwrap();
    let stream_fn: StreamFn = Arc::new(|_, _, _| {
        Box::pin(async {
            let (sender, stream) = create_assistant_message_event_stream();
            sender.fail(StreamFailure::new(raw_error()));
            stream
        })
    });
    let error = generate_summary(&GenerateSummaryWithUsageOptions {
        current_messages: &[],
        model: &model,
        reserve_tokens: 100.0,
        api_key: None,
        headers: None,
        signal: None,
        custom_instructions: None,
        previous_summary: None,
        thinking_level: None,
        stream_fn: Some(&stream_fn),
        env: None,
        retry: None,
    })
    .await
    .unwrap_err();
    assert_eq!(
        error.message(),
        &(JsString::from("Summarization failed: ") + raw_error())
    );
}

#[test]
fn retry_finished_event_keeps_native_units_in_wire() {
    let event = AutoRetryEndEvent {
        event_type: "auto_retry_end",
        success: false,
        attempt: 1,
        final_error: Some(raw_error()),
    };
    let wire = serde_json::to_string(&event).unwrap();
    let parsed = parse_lossless(&wire).unwrap();
    assert_eq!(
        parsed.get("finalError").and_then(JsValue::as_string),
        Some(&raw_error())
    );
}
