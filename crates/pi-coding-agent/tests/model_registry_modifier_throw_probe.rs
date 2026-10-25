use crate::shared_oracle as oracle;
use pi_ai::request::CallbackError;
use pi_coding_agent::core::{
    auth_storage::AuthStorage,
    model_registry::{ExtensionOAuthConfig, ModelRegistry, ProviderConfigInput, ProviderModelInput},
};
use pi_js_compat::{
    abort::AbortReason,
    js_json,
    js_json::{JsString, JsValue},
    node_error::{NativeErrorClass, NativeNodeError},
};
use std::sync::Arc;

const PROVIDER: &str = "registry-modifier-throw";
fn config() -> JsValue {
    js_json!({"api":"openai-completions","baseUrl":"https://fixture.invalid","models":[{"id":"one","name":"One","reasoning":false,"input":["text"],"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},"contextWindow":128,"maxTokens":32}]})
}
fn throw_value(stage: &str, kind: &str, value: &JsString) -> CallbackError {
    if kind == "error" {
        CallbackError::from_rejection(
            NativeNodeError::from_constructor(NativeErrorClass::TypeError, value.clone())
                .with_property("code", JsValue::String("E_MODIFIER".into()))
                .with_property("detail", js_json!({"stage":stage}))
                .into(),
        )
    } else {
        CallbackError::from_rejection(AbortReason::Value(JsValue::String(value.clone())))
    }
}
fn rejection_snapshot(error: CallbackError) -> JsValue {
    match error.thrown_value() {
        AbortReason::Error(error) | AbortReason::DomException(error) => {
            js_json!({"kind":"error","value":error.value()})
        }
        value => js_json!({"kind":"value","value":value.value()}),
    }
}
fn native_snapshot(stage: &str, rejection: CallbackError) -> JsValue {
    pi_ai::oauth::reset_oauth_providers();
    let storage = AuthStorage::in_memory(serde_json::Map::new());
    let registry = ModelRegistry::in_memory(storage.clone()).unwrap();
    let credential = js_json!({"type":"oauth","access":"access","refresh":"refresh","expires":9999999999999_f64});
    if stage == "register" {
        storage.set(PROVIDER, credential.clone()).unwrap();
    }
    let result = registry.register_provider(
        PROVIDER,
        ProviderConfigInput {
            name: Some("Registered Modifier".into()),
            api: Some("openai-completions".into()),
            base_url: Some("https://fixture.invalid".into()),
            models: Some(vec![
                js_json::from_value::<ProviderModelInput>(&config()["models"][0]).unwrap(),
            ]),
            oauth: Some(ExtensionOAuthConfig {
                name: "Modifier Fixture".into(),
                uses_callback_server: None,
                login: Arc::new(|_| Box::pin(async { unreachable!("unused login") })),
                refresh_token: Arc::new(|credential, _| Box::pin(async move { Ok(credential) })),
                get_api_key: Arc::new(|credential| Ok(credential.access.clone())),
                modify_models: Some(Arc::new(move |_, _| Err(rejection.clone()))),
            }),
            ..Default::default()
        },
    );
    let result = if stage == "register" {
        result
    } else {
        result.and_then(|_| {
            storage.set(PROVIDER, credential).unwrap();
            if stage == "unregister" {
                registry.register_provider(
                    "modifier-side",
                    ProviderConfigInput {
                        name: Some("Side".into()),
                        ..Default::default()
                    },
                )?;
                registry.unregister_provider("modifier-side")
            } else if stage == "refresh" {
                registry.refresh()
            } else if stage == "create" {
                ModelRegistry::create(storage, Some("".into())).map(|_| ())
            } else {
                ModelRegistry::in_memory(storage).map(|_| ())
            }
        })
    };
    let rejection = rejection_snapshot(result.expect_err("modifier must reject"));
    let ids = registry
        .get_all()
        .into_iter()
        .filter(|model| model.provider == PROVIDER)
        .map(|model| model.id)
        .collect::<Vec<_>>();
    let actual = js_json!({"rejection":rejection,"ids":ids,"display":registry.get_provider_display_name(PROVIDER),"sideDisplay":registry.get_provider_display_name("modifier-side")});
    registry.unregister_provider(PROVIDER).unwrap();
    pi_ai::oauth::reset_oauth_providers();
    actual
}

#[tokio::test]
async fn modifier_rejections_match_frozen_at_all_registry_entrypoints() {
    let mut source = oracle::TsOracle::spawn(
        "model-registry-oracle.ts",
        "packages/coding-agent/src/core/model-registry.ts",
    );
    let raw = JsString::from("modifier-") + JsString::from_utf16(vec![0xd800]);
    for stage in ["register", "refresh", "constructor", "create", "unregister"] {
        for kind in ["error", "value"] {
            let result = source.request(
                &js_json!({"operation":"modifier_throw","stage":stage,"throwKind":kind,"value":raw,"config":config()}),
            );
            assert_eq!(result["ok"], JsValue::Bool(true), "{result:?}");
            let rejection = if kind == "error" {
                js_json!({"kind":"error","value":{"name":"TypeError","message":raw,"code":"E_MODIFIER","detail":{"stage":stage}}})
            } else {
                js_json!({"kind":"value","value":raw})
            };
            let display = if stage == "register" {
                "Modifier Fixture"
            } else {
                "Registered Modifier"
            };
            assert_eq!(
                result["output"],
                js_json!({"rejection":rejection,"ids":["one"],"display":display,"sideDisplay":"modifier-side"}),
                "{stage}/{kind}"
            );
            assert_eq!(
                native_snapshot(stage, throw_value(stage, kind, &raw)),
                result["output"],
                "{stage}/{kind}"
            );
        }
    }
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
