//! Compiles and exercises the crate-root surface mirroring `index.ts` and
//! `base.ts` (see `docs/porting/traceability/ai-index.md`).

use std::sync::Arc;

use serde_json::json;

fn assert_type_paths() {
    fn type_of<T>(_: Option<T>) {}
    type_of::<pi_ai::AnthropicEffort>(None);
    type_of::<pi_ai::AnthropicOptions>(None);
    type_of::<pi_ai::AnthropicThinkingDisplay>(None);
    type_of::<pi_ai::AzureOpenAIResponsesOptions>(None);
    type_of::<pi_ai::BedrockOptions>(None);
    type_of::<pi_ai::BedrockThinkingDisplay>(None);
    type_of::<pi_ai::GoogleOptions>(None);
    type_of::<pi_ai::GoogleThinkingLevel>(None);
    type_of::<pi_ai::GoogleVertexOptions>(None);
    type_of::<pi_ai::LazyApi>(None);
    type_of::<pi_ai::MistralOptions>(None);
    type_of::<pi_ai::OpenAICodexResponsesOptions>(None);
    type_of::<pi_ai::OpenAICodexWebSocketDebugStats>(None);
    type_of::<pi_ai::OpenAICompletionsOptions>(None);
    type_of::<pi_ai::OpenAIResponsesOptions>(None);
    type_of::<Arc<dyn pi_ai::AuthContext>>(None);
    type_of::<Arc<dyn pi_ai::CredentialStore>>(None);
    type_of::<Arc<dyn pi_ai::OAuthAuth>>(None);
    type_of::<pi_ai::OAuthAuthInfo>(None);
    type_of::<pi_ai::OAuthDeviceCodeInfo>(None);
    type_of::<pi_ai::OAuthPrompt>(None);
    type_of::<pi_ai::OAuthSelectOption>(None);
    type_of::<pi_ai::OAuthSelectPrompt>(None);
    type_of::<Arc<dyn pi_ai::ImagesProvider>>(None);
    type_of::<Arc<dyn pi_ai::ImagesModels>>(None);
    type_of::<Arc<dyn pi_ai::Models>>(None);
    type_of::<pi_ai::FauxCore>(None);
    type_of::<pi_ai::SessionResourceCleanup>(None);
    type_of::<pi_ai::Model>(None);
    type_of::<pi_ai::Context>(None);
    type_of::<pi_ai::AssistantMessageEvent>(None);
    type_of::<pi_ai::DiagnosticCode>(None);
    type_of::<pi_ai::StreamFailure>(None);
    type_of::<pi_ai::JsonParseError>(None);
    type_of::<pi_ai::RetryPolicy>(None);
    type_of::<pi_ai::ValidationError>(None);
    type_of::<pi_ai::StringEnumOptions>(None);
}

#[test]
fn crate_root_reexports_are_usable() {
    assert_type_paths();

    assert_eq!(
        pi_ai::string_enum(&["a", "b"], pi_ai::StringEnumOptions::default()).into_value(),
        json!({"type": "string", "enum": ["a", "b"]})
    );
    assert_eq!(pi_ai::repair_json("{\"a\":\"x\\q\"}"), "{\"a\":\"x\\\\q\"}");
    assert_eq!(
        pi_ai::parse_json_with_repair("{\"a\":1}").expect("json"),
        pi_js_compat::js_json::JsValue::from(json!({"a": 1}))
    );
    assert_eq!(
        pi_ai::parse_streaming_json(Some(&pi_js_compat::js_json::JsString::from("{\"a\":"))),
        pi_js_compat::js_json::JsValue::from(json!({}))
    );
    assert!(pi_ai::get_overflow_patterns().len() > 1);
    assert!(!pi_ai::is_context_overflow(
        &pi_ai::AssistantMessage::empty("api", "provider", "model", 0.0),
        Some(1.0)
    ));
    assert!(!pi_ai::is_codex_cybersecurity_restriction(
        &pi_ai::AssistantMessage::empty("api", "provider", "model", 0.0)
    ));
    assert!(!pi_ai::is_retryable_assistant_error(&pi_ai::AssistantMessage::empty(
        "api", "provider", "model", 0.0
    )));
    assert_eq!(
        pi_ai::format_thrown_value(&pi_ai::ThrownValue::Text(("x".to_owned()).into())),
        "x"
    );
    assert_eq!(
        pi_ai::schema_error_lines(
            &pi_js_compat::js_json::JsValue::from(json!({"type": "string"})),
            &pi_js_compat::js_json::JsValue::Number(1.0)
        ),
        vec!["  - root: must be string".to_owned()]
    );

    let _ = pi_ai::create_models(pi_ai::CreateModelsOptions::default());
    let _ = pi_ai::create_images_models(pi_ai::CreateModelsOptions::default());
    let _ = pi_ai::create_faux_core(pi_ai::RegisterFauxProviderOptions::default());
    let _ = pi_ai::default_provider_auth_context();
    let _ = pi_ai::InMemoryCredentialStore::default();
    let _ = pi_ai::lazy_api(Arc::new(|| {
        Box::pin(async { Err(pi_ai::CallbackError::new("unused")) })
    }));
    let _ = pi_ai::env_api_key_auth("key", &["KEY"]);
    let _ = pi_ai::lazy_oauth(pi_ai::LazyOAuthInput {
        name: "oauth".into(),
        load: Arc::new(|| Box::pin(async { Err(pi_ai::AuthError::failed("unused")) })),
    });
    let _ = pi_ai::create_provider(pi_ai::CreateProviderOptions {
        id: "custom".to_owned(),
        name: None,
        base_url: None,
        headers: None,
        auth: pi_ai::ProviderAuth {
            api_key: None,
            oauth: None,
        },
        models: Vec::new(),
        refresh_models: None,
        api: pi_ai::ProviderApi::Single(Arc::new(pi_ai::compat::ApiProvider::new(
            "custom-api",
            |model, _context, _options| Ok(pi_ai::error_stream(&model, "unused")),
            |model, _context, _options| Ok(pi_ai::error_stream(&model, "unused")),
        ))),
    });
    let _ = pi_ai::register_session_resource_cleanup(Box::new(|_| Ok(())));
    let (_sender, stream) = pi_ai::create_assistant_message_event_stream();
    drop(stream);
}

#[test]
fn base_reexports_the_compat_surface() {
    pi_ai::base::reset_api_providers();
    let apis: Vec<String> = pi_ai::base::get_api_providers()
        .iter()
        .map(|provider| provider.api().to_owned())
        .collect();
    assert_eq!(apis.len(), 9);
    assert!(pi_ai::base::get_model("openai", "gpt-5").is_some());
    assert!(pi_ai::base::get_providers().contains(&"openai-codex".to_owned()));
    let _ = pi_ai::base::faux_text("x");
    let _ = pi_ai::base::anthropic_messages_api();
    let _ = pi_ai::base::get_env_api_key("openai", None).unwrap();
    let _ = pi_ai::base::stream_anthropic;
}
