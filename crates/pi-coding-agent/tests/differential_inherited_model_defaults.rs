//! Frozen plain-object prototype leaks are reachable through real models.json provider IDs.
#[path = "support/oracle.rs"]
mod oracle;
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::model_registry::{ExtensionOAuthConfig, ModelRegistry, ProviderConfigInput};
use pi_coding_agent::modes::interactive::components::oauth_selector::AuthSelectorProvider;
use pi_coding_agent::modes::interactive::login_options::{get_login_provider_options, get_logout_provider_options};
use pi_js_compat::js_json::JsString;
use serde_json::{Value, json};
use std::sync::Arc;

#[test]
fn inherited_provider_names_through_models_json_match_frozen_login_options() {
    let mut source = oracle::TsOracle::spawn("interactive-login-oracle.mts", "packages/coding-agent/src");
    let properties = source.request(&json!({"operation":"prototype"}));
    assert_eq!(properties["ok"], true, "{properties}");
    eprintln!("NODE_OBJECT_PROTOTYPE:{}", properties["output"]);
    for property in properties["output"]["properties"].as_array().unwrap() {
        let id = property["name"].as_str().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("models.json");
        let config = json!({"providers":{(id):{"api":"openai-completions","baseUrl":"https://fixture.invalid","apiKey":"fixture","models":[{"id":"prototype-probe","name":"Prototype probe","reasoning":false,"input":["text"],"contextWindow":4096,"maxTokens":512,"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0}}]}}});
        std::fs::write(&path, serde_json::to_vec(&config).unwrap()).unwrap();
        let expected=source.request(&json!({"operation":"options","modelsPath":path,"credentials":{},"oauthProviders":[{"id":id,"name":"Prototype fixture"}]}));
        assert_eq!(expected["ok"], true, "id={id}: {expected}");
        assert_eq!(expected["output"]["error"], Value::Null, "id={id}: {expected}");
        let registry = ModelRegistry::create(
            AuthStorage::in_memory(Vec::<(String, Value)>::new()),
            Some(JsString::from(path.to_str().unwrap())),
        )
        .unwrap();
        assert_eq!(registry.get_error(), None);
        assert!(registry.find(id, "prototype-probe").is_some());
        registry
            .register_provider(
                id,
                ProviderConfigInput {
                    oauth: Some(ExtensionOAuthConfig {
                        name: "Prototype fixture".into(),
                        uses_callback_server: None,
                        login: Arc::new(|_| Box::pin(async { Err(pi_ai::auth::AuthError::failed("unused login")) })),
                        refresh_token: Arc::new(|_, _| {
                            Box::pin(async { Err(pi_ai::auth::AuthError::failed("unused refresh")) })
                        }),
                        get_api_key: Arc::new(|credential| Ok(credential.access.clone())),
                        modify_models: None,
                    }),
                    ..Default::default()
                },
            )
            .unwrap();
        let encode = |options: Vec<AuthSelectorProvider>| {
            options
                .into_iter()
                .map(|option| json!({"id":option.id,"name":option.name.units(),"authType":option.auth_type.as_str()}))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            json!({"login":encode(get_login_provider_options(&registry,None)),"logout":encode(get_logout_provider_options(&registry).unwrap()),"error":registry.get_error()}),
            expected["output"],
            "id={id}"
        );
        registry.unregister_provider(id);
    }
}
