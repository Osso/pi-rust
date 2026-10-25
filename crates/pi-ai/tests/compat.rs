//! Behavioral tests for `packages/ai/src/compat.ts`.

#[test]
fn compatibility_entrypoint_forwards_the_public_root() {
    let models = pi_ai::compat::create_models(Default::default());
    assert!(pi_ai::compat::Models::get_providers(&models).is_empty());
    let message = pi_ai::compat::AssistantMessage::empty("api", "provider", "model", 0.0);
    assert_eq!(message.model, "model");
    let _: pi_ai::compat::StreamOptions = Default::default();
    let _: pi_ai::compat::OAuthPrompt = pi_ai::compat::OAuthPrompt {
        message: "Code".into(),
        placeholder: None,
        allow_empty: None,
    };
}

use std::sync::{Arc, Mutex};

use pi_ai::compat::{
    ApiProvider, complete, get_api_provider, get_api_providers, register_api_provider, register_faux_provider,
    reset_api_providers, stream, unregister_api_providers,
};
use pi_ai::providers::faux::{FauxAssistantMessageOptions, faux_assistant_message};
use pi_ai::request::{ProviderRequestOptions, ProviderStreams, SimpleStreamOptions, StreamOptions};
use pi_ai::types::{
    AssistantContent, AssistantMessage, AssistantMessageEvent, Context, DoneReason, Modality, Model, ModelCost,
    ModelCostRates, ProviderEnv, StopReason, TextContent,
};
use pi_ai::utils::event_stream::{AssistantMessageEventStream, create_assistant_message_event_stream};

const BUILTIN_APIS: [&str; 9] = [
    "anthropic-messages",
    "openai-completions",
    "openai-responses",
    "openai-codex-responses",
    "azure-openai-responses",
    "google-generative-ai",
    "google-vertex",
    "mistral-conversations",
    "bedrock-converse-stream",
];

/// The api registry is process-global; serialize tests that mutate it.
static REGISTRY_LOCK: Mutex<()> = Mutex::new(());

fn serial() -> std::sync::MutexGuard<'static, ()> {
    REGISTRY_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn context() -> Context {
    Context {
        system_prompt: None,
        messages: Vec::new(),
        tools: None,
    }
}

fn model(api: &str, provider: &str, id: &str) -> Model {
    Model {
        properties: Default::default(),
        id: id.to_owned(),
        name: "Test Model".into(),
        api: api.to_owned(),
        provider: provider.to_owned(),
        base_url: ("https://example.test/v1".to_owned()).into(),
        reasoning: false,
        thinking_level_map: None,
        input: vec![Modality::Text],
        cost: ModelCost {
            properties: Default::default(),
            rates: ModelCostRates::default(),
            tiers: None,
        },
        context_window: 128_000.0,
        auto_compaction_threshold: None,
        max_tokens: 4096.0,
        headers: None,
        compat: None,
    }
}

fn ok_message(model: &Model) -> AssistantMessage {
    let mut message = AssistantMessage::empty(&model.api, &model.provider, &model.id, 0.0);
    message.content = vec![AssistantContent::Text(TextContent::new("ok"))];
    message.stop_reason = StopReason::Stop;
    message
}

fn fake_stream(model: &Model) -> AssistantMessageEventStream {
    let message = ok_message(model);
    let (sender, stream) = create_assistant_message_event_stream();
    sender.push(AssistantMessageEvent::Start {
        partial: message.clone(),
    });
    sender.push(AssistantMessageEvent::Done {
        reason: DoneReason::Stop,
        message,
    });
    sender.end(None);
    stream
}

fn fake_provider(api: &str, captured: Arc<Mutex<Vec<Option<pi_js_compat::js_json::JsString>>>>) -> ApiProvider {
    let stream_captured = Arc::clone(&captured);
    let simple_captured = Arc::clone(&captured);
    ApiProvider::new(
        api,
        move |model, _context, options| {
            stream_captured.lock().unwrap().push(options.request.api_key.clone());
            Ok(fake_stream(&model))
        },
        move |model, _context, options| {
            simple_captured
                .lock()
                .unwrap()
                .push(options.stream.request.api_key.clone());
            Ok(fake_stream(&model))
        },
    )
}

fn stream_options() -> StreamOptions {
    StreamOptions::default()
}

fn scoped_env(name: &str, value: &str) -> Option<ProviderEnv> {
    Some(indexmap::IndexMap::from([(name.into(), value.into())]))
}

#[test]
fn built_in_apis_are_registered_in_compat_order() {
    let _guard = serial();
    reset_api_providers();
    let apis: Vec<String> = get_api_providers()
        .iter()
        .map(|provider| provider.api().to_owned())
        .collect();
    assert_eq!(apis, BUILTIN_APIS);
    for api in BUILTIN_APIS {
        assert!(get_api_provider(api).is_some(), "missing builtin api {api}");
    }
}

#[test]
fn unknown_api_throws_the_registration_error_before_returning_a_stream() {
    let _guard = serial();
    reset_api_providers();
    let model = model("missing-api", "custom", "m");
    let error = match stream(model, context(), stream_options()) {
        Ok(_) => panic!("missing provider must throw before returning a stream"),
        Err(error) => error,
    };
    assert_eq!(error.message, "No API provider registered for api: missing-api");
    assert_eq!(error.name, "Error");
}

#[test]
fn registered_provider_receives_the_explicit_api_key() {
    let _guard = serial();
    reset_api_providers();
    let captured = Arc::new(Mutex::new(Vec::new()));
    register_api_provider(fake_provider("openai-responses", Arc::clone(&captured)), None);

    let model = model("openai-responses", "custom-openai", "test-model");
    let options = StreamOptions {
        request: ProviderRequestOptions {
            api_key: Some(("request-key".to_owned()).into()),
            ..ProviderRequestOptions::default()
        },
        ..StreamOptions::default()
    };
    let message = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(complete(model, context(), options))
        .expect("complete");
    assert_eq!(message.content, vec![AssistantContent::Text(TextContent::new("ok"))]);
    assert_eq!(
        *captured.lock().unwrap(),
        vec![Some(pi_js_compat::js_json::JsString::from("request-key"))]
    );
}

#[test]
fn scoped_env_key_is_injected_through_a_builtin_provider_override() {
    let _guard = serial();
    reset_api_providers();
    let captured = Arc::new(Mutex::new(Vec::new()));
    register_api_provider(fake_provider("openai-responses", Arc::clone(&captured)), None);

    let model = model("openai-responses", "openai", "gpt-5");
    let options = StreamOptions {
        request: ProviderRequestOptions {
            env: scoped_env("OPENAI_API_KEY", "env-key"),
            ..ProviderRequestOptions::default()
        },
        ..StreamOptions::default()
    };
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(complete(model, context(), options))
        .expect("complete");
    assert_eq!(
        *captured.lock().unwrap(),
        vec![Some(pi_js_compat::js_json::JsString::from("env-key"))]
    );
}

#[test]
fn explicit_key_beats_the_scoped_env_key_and_blank_keys_fall_back() {
    let _guard = serial();
    reset_api_providers();
    let captured = Arc::new(Mutex::new(Vec::new()));
    register_api_provider(fake_provider("openai-responses", Arc::clone(&captured)), None);

    let runtime = tokio::runtime::Runtime::new().unwrap();
    for (explicit, expected) in [
        (Some("request-key"), "request-key"),
        (Some("   "), "env-key"),
        (None, "env-key"),
    ] {
        let model = model("openai-responses", "openai", "gpt-5");
        let options = StreamOptions {
            request: ProviderRequestOptions {
                api_key: (explicit.map(str::to_owned)).map(pi_js_compat::js_json::JsString::from),
                env: scoped_env("OPENAI_API_KEY", "env-key"),
                ..ProviderRequestOptions::default()
            },
            ..StreamOptions::default()
        };
        runtime.block_on(complete(model, context(), options)).expect("complete");
        assert_eq!(
            captured.lock().unwrap().pop(),
            Some(Some(pi_js_compat::js_json::JsString::from(expected))),
            "explicit key {explicit:?}"
        );
    }
}

#[test]
fn stream_simple_injects_the_env_key_and_keeps_reasoning_options() {
    let _guard = serial();
    reset_api_providers();
    let captured = Arc::new(Mutex::new(Vec::new()));
    let simple_seen = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&simple_seen);
    register_api_provider(
        ApiProvider::new(
            "openai-responses",
            {
                let captured = Arc::clone(&captured);
                move |model, _context, options| {
                    captured.lock().unwrap().push(options.request.api_key.clone());
                    Ok(fake_stream(&model))
                }
            },
            move |model, _context, options| {
                seen.lock()
                    .unwrap()
                    .push((options.stream.request.api_key.clone(), options.reasoning));
                Ok(fake_stream(&model))
            },
        ),
        None,
    );

    let model = model("openai-responses", "ant-ling", "test-model");
    let options = SimpleStreamOptions {
        stream: StreamOptions {
            request: ProviderRequestOptions {
                env: scoped_env("ANT_LING_API_KEY", "env-key"),
                ..ProviderRequestOptions::default()
            },
            ..StreamOptions::default()
        },
        reasoning: Some(pi_ai::types::ThinkingLevel::High),
        ..SimpleStreamOptions::default()
    };
    let stream = pi_ai::compat::stream_simple(model, context(), options).unwrap();
    let _ = tokio::runtime::Runtime::new().unwrap().block_on(stream.result());
    assert_eq!(
        *simple_seen.lock().unwrap(),
        vec![(
            Some(pi_js_compat::js_json::JsString::from("env-key")),
            Some(pi_ai::types::ThinkingLevel::High)
        )]
    );
}

#[test]
fn unregister_removes_only_the_named_source() {
    let _guard = serial();
    reset_api_providers();
    let captured = Arc::new(Mutex::new(Vec::new()));
    register_api_provider(
        fake_provider("openai-responses", Arc::clone(&captured)),
        Some("source-a".to_owned()),
    );
    register_api_provider(
        fake_provider("openai-completions", Arc::clone(&captured)),
        Some("source-b".to_owned()),
    );

    unregister_api_providers("source-a");
    assert!(get_api_provider("openai-responses").is_none());
    assert_eq!(
        get_api_provider("openai-completions").unwrap().api(),
        "openai-completions"
    );
    reset_api_providers();
}

#[test]
fn override_replaces_the_builtin_instance_until_reset() {
    let _guard = serial();
    reset_api_providers();
    let builtin = get_api_provider("openai-responses").expect("builtin");
    let captured = Arc::new(Mutex::new(Vec::new()));
    register_api_provider(fake_provider("openai-responses", captured), None);
    let override_instance = get_api_provider("openai-responses").expect("override");
    assert!(!Arc::ptr_eq(&builtin, &override_instance));

    reset_api_providers();
    let restored = get_api_provider("openai-responses").expect("restored builtin");
    assert!(!Arc::ptr_eq(&builtin, &restored));
    assert!(!Arc::ptr_eq(&override_instance, &restored));
}

#[test]
fn api_mismatch_is_reported_as_the_typescript_error_message() {
    let _guard = serial();
    reset_api_providers();
    let captured = Arc::new(Mutex::new(Vec::new()));
    register_api_provider(fake_provider("openai-responses", captured), None);
    let provider = get_api_provider("openai-responses").expect("registered");
    let model = model("other-api", "custom", "m");
    let error = provider
        .stream(model, context(), stream_options())
        .err()
        .expect("synchronous mismatch rejection");
    assert_eq!(error.name, "Error");
    assert_eq!(error.message, "Mismatched api: other-api expected openai-responses");
}

#[test]
fn faux_registration_completes_and_estimates_usage() {
    let _guard = serial();
    reset_api_providers();
    let registration = register_faux_provider(Default::default());
    registration.set_responses(vec![
        faux_assistant_message("hello world", FauxAssistantMessageOptions::default()).into(),
    ]);
    let model = registration.get_model(None).expect("default faux model");

    let mut context = context();
    context.messages = vec![pi_ai::types::Message::User(pi_ai::types::UserMessage {
        role: pi_ai::types::UserRole::Tag,
        content: pi_ai::types::UserContent::Text(("hi there".to_owned()).into()),
        input_source: None,
        provider_native: None,
        timestamp: 0.0,
    })];
    let message = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(complete(model, context, stream_options()))
        .expect("complete");
    assert_eq!(
        message.content,
        vec![AssistantContent::Text(TextContent::new("hello world"))]
    );
    assert!(message.usage.input > 0.0);
    assert!(message.usage.output > 0.0);
    assert_eq!(message.usage.total_tokens, message.usage.input + message.usage.output);
    assert_eq!(registration.state().lock().unwrap().call_count, 1);
    assert_eq!(registration.get_pending_response_count(), 0);

    registration.unregister();
    assert!(get_api_provider(registration.api()).is_none());
    reset_api_providers();
}
