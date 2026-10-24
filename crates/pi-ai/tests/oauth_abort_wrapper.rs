#[path = "support/oracle.rs"]
mod oracle;
use pi_ai::auth::{AuthError, OAuthCredential};
use pi_ai::oauth::{
    OAuthCredentials, OAuthLoginCallbacks, OAuthProviderInterface, get_oauth_api_key, register_oauth_provider,
    unregister_oauth_provider,
};
use pi_ai::request::{BoxFuture, CallbackError};
use pi_js_compat::abort::{AbortController, AbortReason, AbortSignal};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{JsString, JsValue};
use pi_js_compat::node_error::{NativeErrorClass, NativeNodeError};
use std::collections::HashMap;
use std::sync::Arc;
struct Cancelled(AbortSignal);
impl OAuthProviderInterface for Cancelled {
    fn id(&self) -> &str {
        "abort-fixture"
    }
    fn name(&self) -> &str {
        "Fixture"
    }
    fn login(&self, _: Arc<dyn OAuthLoginCallbacks>) -> BoxFuture<'static, Result<OAuthCredentials, AuthError>> {
        Box::pin(async { Err(AuthError::failed("must not login")) })
    }
    fn refresh_token(&self, _: OAuthCredentials) -> BoxFuture<'static, Result<OAuthCredentials, AuthError>> {
        let signal = self.0.clone();
        Box::pin(async move { Err(AuthError::from_abort_reason(signal.wait().await)) })
    }
    fn get_api_key(&self, _: &OAuthCredentials) -> Result<JsString, CallbackError> {
        Ok("unused".into())
    }
}
#[tokio::test]
async fn legacy_refresh_wrapper_preserves_actual_abort_causes_and_source_string_conversion() {
    let mut source = oracle::TsOracle::spawn("oauth-abort-wrapper-oracle.ts", "packages/ai/src/utils/oauth/index.ts");
    let message = JsString::from_utf16(vec![78, 0xd800]);
    let native = NativeNodeError::from_constructor(NativeErrorClass::TypeError, message.clone())
        .with_cause(AbortReason::Value(JsValue::Bool(false)))
        .with_property("code", JsValue::String("E_CANCEL".into()));
    let cases = vec![
        (None, json!({"default":true})),
        (Some(AbortReason::Value(JsValue::Bool(false))), json!({"reason":false})),
        (
            Some(AbortReason::Value(JsValue::String(message.clone()))),
            json!({"reason":message}),
        ),
        (
            Some(AbortReason::Value(json!({"toString":0}))),
            json!({"reason":{"toString":0}}),
        ),
        (Some(native.into()), json!({"nativeMessage":message})),
    ];
    for (reason, request) in cases {
        let controller = AbortController::new();
        controller.abort(reason);
        register_oauth_provider(Arc::new(Cancelled(controller.signal())));
        let error = get_oauth_api_key(
            "abort-fixture",
            &HashMap::from([("abort-fixture".into(), OAuthCredential::new("r", "old", 0.0))]),
        )
        .await
        .unwrap_err();
        unregister_oauth_provider("abort-fixture");
        let expected = source.request(&request);
        assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
        assert_eq!(
            json!({"kind":"error","value":error.into_callback().thrown_value().value()}),
            expected["output"]
        );
    }
}
