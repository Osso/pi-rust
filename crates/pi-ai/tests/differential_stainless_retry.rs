#[path = "support/oracle.rs"]
mod oracle;

use pi_ai::request::CallbackError;
use pi_ai::utils::provider_retry::{
    ProviderError, ProviderErrorHeaders, ProviderRequestError, ProviderRetryOptions, retry_provider_request,
};
use serde_json::json;
use std::time::{Duration, Instant};

#[tokio::test]
async fn zero_millisecond_hint_uses_seconds_header_like_both_actual_sdks() {
    let mut source = oracle::TsOracle::spawn("stainless-retry-oracle.ts", "packages/ai/src/api/anthropic-messages.ts");
    for sdk in ["openai", "anthropic"] {
        let observed = source.request(&json!({"sdk":sdk,"headers":{"retry-after-ms":"0","retry-after":"0.2"}}));
        assert_eq!(observed, json!({"calls":2,"delays":[200]}), "{sdk}");
    }
    let mut headers = ProviderErrorHeaders::new();
    headers.insert("retry-after-ms", "0");
    headers.insert("retry-after", "0.2");
    let error = ProviderError {
        message: "retry".into(),
        status: Some(503.0),
        headers,
        body: None,
        inner_error: None,
    };
    let mut calls = 0;
    let start = Instant::now();
    let result = retry_provider_request(
        || {
            calls += 1;
            let response = if calls == 1 {
                Err(ProviderRequestError::<CallbackError>::Provider(error.clone()))
            } else {
                Ok(calls)
            };
            async move { response }
        },
        ProviderRetryOptions {
            max_retries: 1,
            max_retry_delay_ms: Some(0.0),
            signal: None,
        },
    )
    .await;
    assert_eq!(result.unwrap(), 2);
    assert!(
        start.elapsed() >= Duration::from_millis(180),
        "seconds hint was ignored: {:?}",
        start.elapsed()
    );
}
