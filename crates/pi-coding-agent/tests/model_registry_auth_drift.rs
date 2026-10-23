//! Frozen registered-OAuth callback rejection, replacing the old CredentialStore facade test.
#[path = "support/oracle.rs"]
mod oracle;
use pi_ai::auth::AuthError;
use pi_ai::request::CallbackError;
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::model_registry::{
    ExtensionOAuthConfig, ModelRegistry, ProviderConfigInput, ResolvedRequestAuth,
};
use pi_js_compat::{
    abort::AbortReason,
    js_json,
    js_json::{JsString, JsValue},
    node_error::{NativeErrorClass, NativeNodeError},
};
use std::sync::Arc;

fn at<'a>(value: &'a JsValue, path: &[&str]) -> &'a JsValue {
    path.iter()
        .fold(value, |value, key| value.get(key).expect("oracle field"))
}
#[tokio::test]
async fn provider_key_lookup_preserves_frozen_error_brand_and_arbitrary_thrown_value() {
    let mut oracle = oracle::TsOracle::spawn(
        "model-registry-oracle.ts",
        "packages/coding-agent/src/core/model-registry.ts",
    );
    let raw = JsString::from_utf16(vec![0x71, 0xd800, 0x31, 0xdc00]);
    let object = js_json!({"toString":0});
    for (kind, value) in [
        ("error", JsValue::String(raw.clone())),
        ("value", JsValue::String(raw.clone())),
        ("value", object),
    ] {
        let expected = oracle
            .request(&js_json!({"operation":"oauth_throw","throwKind":kind,"value":value,"detail":{"native":true}}));
        assert_eq!(at(&expected, &["ok"]), &JsValue::Bool(true));
        let reason = if kind == "error" {
            AbortReason::Error(
                NativeNodeError::from_constructor(NativeErrorClass::TypeError, raw.clone())
                    .with_property("code", JsValue::String("E_Q15".into()))
                    .with_property("detail", js_json!({"native":true})),
            )
        } else {
            AbortReason::Value(value)
        };
        let registry = ModelRegistry::in_memory(AuthStorage::in_memory([(
            "q15-oauth-throw",
            js_json!({"type":"oauth","access":"access","refresh":"refresh","expires":9999999999999_f64}),
        )]));
        let rejected = reason.clone();
        registry
            .register_provider(
                "q15-oauth-throw",
                ProviderConfigInput {
                    oauth: Some(ExtensionOAuthConfig {
                        name: "Q15 throw fixture".into(),
                        uses_callback_server: None,
                        login: Arc::new(|_| Box::pin(async { Err(AuthError::failed("unexpected login")) })),
                        refresh_token: Arc::new(|_, _| {
                            Box::pin(async { Err(AuthError::failed("unexpected refresh")) })
                        }),
                        get_api_key: Arc::new(move |_| Err(CallbackError::from_rejection(rejected.clone()))),
                        modify_models: None,
                    }),
                    ..Default::default()
                },
            )
            .unwrap();
        let error = registry.get_api_key_for_provider("q15-oauth-throw").await.unwrap_err();
        assert_eq!(error.thrown_value(), reason);
        let snapshot = js_json!({"kind":kind,"value":error.thrown_value().value()});
        assert_eq!(&snapshot, at(&expected, &["output", "key", "rejection"]));
        let mut model = registry.find("anthropic", "claude-opus-4-8").unwrap();
        model.provider = "q15-oauth-throw".into();
        let auth = match registry.get_api_key_and_headers(&model).await {
            Ok(ResolvedRequestAuth::Err { error }) => js_json!({"ok":true,"value":{"ok":false,"error":error}}),
            Err(error) => js_json!({"ok":false,"rejection":{"kind":"error","value":error.thrown_value().value()}}),
            Ok(other) => panic!("unexpected callback success: {other:?}"),
        };
        assert_eq!(&auth, at(&expected, &["output", "auth"]));
    }
}
