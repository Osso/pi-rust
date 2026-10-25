//! Frozen coding compaction.ts:697–708 and branch-summarization.ts:349–353.
use pi_agent_core::{AgentError, AgentMessage, StreamFn};
use pi_ai::providers::faux::{RegisterFauxProviderOptions, faux_provider};
use pi_coding_agent::core::compaction::branch_summarization::{GenerateBranchSummaryOptions, generate_branch_summary};
use pi_coding_agent::core::compaction::{
    GenerateSummaryOptions, MaterializeCompactionSummaryOptions, generate_summary, materialize_compaction_summary,
};
use pi_coding_agent::core::session_manager::SessionEntry;
use pi_js_compat::js_json::JsString;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

fn raw_error() -> JsString {
    JsString::from_utf16(vec![0xd800, 0x21, 0xdfff])
}
fn failing_stream(attempts: Arc<AtomicUsize>) -> StreamFn {
    Arc::new(move |_, _, options| {
        attempts.fetch_add(1, Ordering::SeqCst);
        assert_eq!(options.stream.cache_retention, Some(pi_ai::CacheRetention::None));
        assert_eq!(options.stream.session_id, None);
        Box::pin(async { Err(AgentError::new(raw_error())) })
    })
}
fn entry(text: &str) -> SessionEntry {
    serde_json::from_str(text).unwrap()
}

#[tokio::test]
async fn summary_and_materialization_propagate_acquisition_failure_without_retry_or_prefix() {
    let model = faux_provider(RegisterFauxProviderOptions::default())
        .get_model(None)
        .unwrap();
    let attempts = Arc::new(AtomicUsize::new(0));
    let stream = failing_stream(attempts.clone());
    let user: AgentMessage = serde_json::from_str(r#"{"role":"user","content":"summarize","timestamp":1}"#).unwrap();
    let failure = generate_summary(&GenerateSummaryOptions {
        current_messages: &[user],
        model: &model,
        reserve_tokens: 2000.0,
        api_key: None,
        headers: None,
        signal: None,
        custom_instructions: None,
        previous_summary: None,
        thinking_level: None,
        stream_fn: Some(&stream),
        env: None,
    })
    .await
    .unwrap_err();
    assert_eq!(failure.message(), &raw_error());
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
    let checkpoint = entry(
        r#"{"type":"compaction","id":"checkpoint","parentId":null,"timestamp":"2026-01-01T00:00:00.000Z","summary":"","firstKeptEntryId":"root","tokensBefore":4,"providerNative":{"provider":"openai","api":"openai-responses","format":"openai.responses.input","value":{"id":"checkpoint"}}}"#,
    );
    let failure = materialize_compaction_summary(
        &checkpoint.as_compaction().unwrap(),
        &model,
        &MaterializeCompactionSummaryOptions {
            reserve_tokens: 2000.0,
            api_key: None,
            headers: None,
            env: None,
            signal: None,
            stream_fn: Some(&stream),
        },
    )
    .await
    .unwrap_err();
    assert_eq!(failure.message(), &raw_error());
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn branch_summary_propagates_acquisition_failure_instead_of_a_provider_error_record() {
    let model = faux_provider(RegisterFauxProviderOptions::default())
        .get_model(None)
        .unwrap();
    let attempts = Arc::new(AtomicUsize::new(0));
    let stream = failing_stream(attempts.clone());
    let user = entry(
        r#"{"type":"message","id":"root","parentId":null,"timestamp":"2026-01-01T00:00:00.000Z","message":{"role":"user","content":"summarize","timestamp":1}}"#,
    );
    let failure = generate_branch_summary(
        &[user],
        &GenerateBranchSummaryOptions {
            model: &model,
            api_key: None,
            headers: None,
            env: None,
            signal: None,
            custom_instructions: None,
            replace_instructions: false,
            reserve_tokens: None,
            stream_fn: Some(&stream),
        },
    )
    .await
    .unwrap_err();
    assert_eq!(failure.message(), &raw_error());
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
}
