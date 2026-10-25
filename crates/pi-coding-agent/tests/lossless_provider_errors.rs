use pi_coding_agent::core::agent_session::AutoRetryEndEvent;
use pi_js_compat::js_json::{JsString, JsValue, parse_lossless};

fn raw_error() -> JsString {
    JsString::from_utf16(vec![0xd800, 0x21, 0xdfff])
}

#[tokio::test]
async fn coding_compaction_propagates_failed_stream_message_without_adding_a_prefix() {
    use pi_agent_core::agent::StreamFn;
    use pi_ai::providers::faux::{RegisterFauxProviderOptions, faux_provider};
    use pi_ai::utils::event_stream::{StreamFailure, create_assistant_message_event_stream};
    use pi_coding_agent::core::compaction::{GenerateSummaryOptions, generate_summary};
    use std::sync::Arc;
    let model = faux_provider(RegisterFauxProviderOptions::default())
        .get_model(None)
        .unwrap();
    let stream_fn: StreamFn = Arc::new(|_, _, _| {
        Box::pin(async {
            let (sender, stream) = create_assistant_message_event_stream();
            sender.fail(StreamFailure::new(raw_error()));
            Ok(stream)
        })
    });
    let error = generate_summary(&GenerateSummaryOptions {
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
    })
    .await
    .unwrap_err();
    assert_eq!(error.message(), &raw_error());
}

#[test]
fn extension_custom_message_payload_keeps_raw_kind_through_wire() {
    use pi_coding_agent::core::extensions::types::{CustomMessageContent, CustomMessagePayload};
    let payload = CustomMessagePayload {
        custom_type: raw_error(),
        content: CustomMessageContent::Text("body".into()),
        display: true,
        details: None,
    };
    let wire = serde_json::to_string(&payload).unwrap();
    assert_eq!(
        parse_lossless(&wire)
            .unwrap()
            .get("customType")
            .and_then(JsValue::as_string),
        Some(&raw_error())
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
