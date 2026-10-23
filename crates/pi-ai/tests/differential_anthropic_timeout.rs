#[path = "support/abort_reason.rs"]
mod abort_reason;
#[path = "support/oauth.rs"]
mod oauth_support;
#[path = "support/oracle.rs"]
mod oracle;

use pi_ai::auth::oauth::anthropic::AnthropicOAuth;
use pi_ai::auth::{OAuthAuth, OAuthCredential};
use pi_ai::request::{BoxFuture, CallbackError, Fetch, HttpRequest, HttpResponse};
use pi_js_compat::abort::{AbortController, AbortReason, AbortSignal};
use pi_js_compat::js_json::JsValue;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct TimeoutFetch {
    signals: Mutex<Vec<AbortSignal>>,
}
impl Fetch for TimeoutFetch {
    fn fetch(
        &self,
        _request: HttpRequest,
        signal: Option<AbortSignal>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        let signal = signal.expect("timeout signal");
        self.signals.lock().unwrap().push(signal.clone());
        Box::pin(async move { Err(CallbackError::from_rejection(signal.wait().await)) })
    }
}

struct RejectFetch(CallbackError);
impl Fetch for RejectFetch {
    fn fetch(
        &self,
        _request: HttpRequest,
        _signal: Option<AbortSignal>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        let error = self.0.clone();
        Box::pin(async move { Err(error) })
    }
}

#[tokio::test]
async fn anthropic_refresh_formats_branded_errors_and_raw_values_without_eager_projection() {
    let mut oracle = oracle::TsOracle::spawn(
        "anthropic-timeout-oracle.ts",
        "packages/ai/src/utils/oauth/anthropic.ts",
    );
    let cases = pi_js_compat::js_json::parse_lossless(r#"[
        {"kind":"value","value":null},
        {"kind":"value","value":false},
        {"kind":"value","value":"failure\ud800"},
        {"kind":"value","value":{"name":"NotError","message":"not used"}},
        {"kind":"value","value":{"toString":false}},
        {"kind":"error","name":"Fetch\ud800","message":"failed\udc00","properties":{"code":"E\ud800","errno":0,"cause":{"name":"NotError","message":"raw"}}},
        {"kind":"error","message":"empty metadata","properties":{"code":0,"errno":null,"cause":false}}
    ]"#).unwrap();
    for input in cases.as_array().unwrap() {
        let expected = oracle.request(&pi_js_compat::js_json!({"id":"failure","failure":input}));
        assert_eq!(expected["ok"], JsValue::Bool(true));
        let reason = if input["kind"] == JsValue::String("value".into()) {
            AbortReason::Value(input["value"].clone())
        } else {
            let mut error = pi_js_compat::node_error::NativeNodeError::new(
                input["name"].as_string().cloned().unwrap_or_else(|| "Error".into()),
                input["message"].as_string().unwrap().clone(),
            );
            for (name, value) in input["properties"].as_object().unwrap() {
                error = error.with_property(name.clone(), value.clone());
            }
            AbortReason::Error(error)
        };
        let oauth = AnthropicOAuth::with_runtime(oauth_support::runtime(
            Arc::new(RejectFetch(CallbackError::from_rejection(reason))),
            0.0,
        ));
        let failure = oauth
            .refresh(
                OAuthCredential::new("old-refresh", "old-access", 0.0),
                AbortSignal::default(),
            )
            .await
            .unwrap_err();
        let pi_ai::auth::AuthError::Failed(error) = failure else {
            panic!("unexpected aborted classification");
        };
        assert_eq!(
            pi_js_compat::js_json!({"name":error.name,"message":error.message}),
            expected["output"]["error"],
            "{input:?}"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn anthropic_refresh_uses_its_own_branded_deadline_and_formats_timeout_name() {
    let mut oracle = oracle::TsOracle::spawn(
        "anthropic-timeout-oracle.ts",
        "packages/ai/src/utils/oauth/anthropic.ts",
    );
    let expected = oracle.request(&pi_js_compat::js_json!({"id":"timeout"}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    assert_eq!(expected["output"]["delays"], pi_js_compat::js_json!([30000]));
    let fetch = Arc::new(TimeoutFetch::default());
    let oauth = AnthropicOAuth::with_runtime(oauth_support::runtime(fetch.clone(), 0.0));
    let caller = AbortController::new();
    let refresh = oauth.refresh(OAuthCredential::new("old-refresh", "old-access", 0.0), caller.signal());
    let failure = refresh.await.unwrap_err();
    let signal = fetch.signals.lock().unwrap()[0].clone();
    assert_ne!(
        signal,
        caller.signal(),
        "source deadline is independent of the caller signal"
    );
    assert_eq!(abort_reason::describe(signal.reason()), expected["output"]["signal"]);
    assert_eq!(failure.detail(), expected["output"]["error"]["message"].as_string());
}
