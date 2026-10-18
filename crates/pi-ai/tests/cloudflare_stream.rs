//! Port of `packages/ai/test/cloudflare-stream.test.ts`.

use std::sync::{Arc, Mutex};

use pi_ai::providers::cloudflare_stream::{cloudflare_streams, resolve_cloudflare_model};
use pi_ai::request::{ProviderStreams, SimpleStreamOptions, StreamOptions};
use pi_ai::types::{Context, Model, ProviderEnv};
use pi_ai::utils::event_stream::{AssistantMessageEventStream, create_assistant_message_event_stream};
use serde_json::json;

fn model() -> Model {
    serde_json::from_value(json!({
        "id": "model",
        "name": "model",
        "api": "openai-completions",
        "provider": "cloudflare-ai-gateway",
        "baseUrl": "https://gateway.ai.cloudflare.com/v1/{CLOUDFLARE_ACCOUNT_ID}/{CLOUDFLARE_GATEWAY_ID}/openai",
        "reasoning": false,
        "input": ["text"],
        "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0},
        "contextWindow": 1000,
        "maxTokens": 100
    }))
    .expect("model")
}

fn context() -> Context {
    Context {
        system_prompt: None,
        messages: Vec::new(),
        tools: None,
    }
}

struct CapturingStreams {
    captured: Arc<Mutex<Vec<String>>>,
}

impl ProviderStreams for CapturingStreams {
    fn stream(&self, model: Model, _context: Context, _options: StreamOptions) -> AssistantMessageEventStream {
        self.captured.lock().expect("captured").push(model.base_url);
        create_assistant_message_event_stream().1
    }

    fn stream_simple(
        &self,
        model: Model,
        _context: Context,
        _options: SimpleStreamOptions,
    ) -> AssistantMessageEventStream {
        self.captured.lock().expect("captured").push(model.base_url);
        create_assistant_message_event_stream().1
    }
}

fn env(pairs: &[(&str, &str)]) -> ProviderEnv {
    pairs
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect()
}

#[test]
fn materializes_the_model_endpoint_before_dispatch() {
    let captured = Arc::new(Mutex::new(Vec::new()));
    let streams = cloudflare_streams(Arc::new(CapturingStreams {
        captured: Arc::clone(&captured),
    }));
    let env = env(&[
        ("CLOUDFLARE_ACCOUNT_ID", "account"),
        ("CLOUDFLARE_GATEWAY_ID", "gateway"),
    ]);

    let mut options = StreamOptions::default();
    options.request.env = Some(env.clone());
    streams.stream(model(), context(), options);

    let mut simple = SimpleStreamOptions::default();
    simple.stream.request.env = Some(env);
    streams.stream_simple(model(), context(), simple);

    assert_eq!(
        *captured.lock().expect("captured"),
        vec![
            "https://gateway.ai.cloudflare.com/v1/account/gateway/openai".to_owned(),
            "https://gateway.ai.cloudflare.com/v1/account/gateway/openai".to_owned(),
        ]
    );
}

#[test]
fn keeps_placeholders_when_the_provider_env_does_not_resolve_them() {
    let captured = Arc::new(Mutex::new(Vec::new()));
    let streams = cloudflare_streams(Arc::new(CapturingStreams {
        captured: Arc::clone(&captured),
    }));
    streams.stream_simple(model(), context(), SimpleStreamOptions::default());

    assert_eq!(*captured.lock().expect("captured"), vec![model().base_url]);
}

#[test]
fn resolves_each_placeholder_independently() {
    let resolved = resolve_cloudflare_model(&model(), Some(&env(&[("CLOUDFLARE_ACCOUNT_ID", "account")])));
    assert_eq!(
        resolved.base_url,
        "https://gateway.ai.cloudflare.com/v1/account/{CLOUDFLARE_GATEWAY_ID}/openai"
    );

    let unchanged = resolve_cloudflare_model(&model(), None);
    assert_eq!(unchanged.base_url, model().base_url);
}
