//! Behavioral tests for the Google Generative AI adapter that the differential oracle cannot cover.

use pi_ai::api::google_generative_ai::stream_simple as gemini_stream_simple;
use pi_ai::request::SimpleStreamOptions;
use pi_ai::types::{Context, Message, Model, StopReason, UserContent, UserMessage};
use pi_ai::utils::event_stream::AssistantMessageEventStream;
use serde_json::json;

fn gemini_model(id: &str) -> Model {
    serde_json::from_value(json!({
        "id": id, "name": id, "api": "google-generative-ai", "provider": "google",
        "baseUrl": "https://generativelanguage.googleapis.com/v1beta",
        "reasoning": true,
        "input": ["text", "image"],
        "cost": {"input": 0.3, "output": 2.5, "cacheRead": 0.03, "cacheWrite": 0.0},
        "contextWindow": 1048576, "maxTokens": 65536
    }))
    .expect("gemini model")
}

fn context() -> Context {
    Context {
        system_prompt: None,
        messages: vec![Message::User(UserMessage {
            role: Default::default(),
            content: UserContent::Text("hello".to_owned()),
            input_source: None,
            provider_native: None,
            timestamp: 1,
        })],
        tools: None,
    }
}

async fn collect(mut stream: AssistantMessageEventStream) -> pi_ai::types::AssistantMessage {
    while stream.next().await.is_some() {}
    stream.result().await.expect("stream result")
}

#[tokio::test]
async fn gemini_stream_simple_without_an_api_key_returns_an_error_stream() {
    let message = collect(gemini_stream_simple(
        gemini_model("gemini-2.5-flash"),
        context(),
        SimpleStreamOptions::default(),
    ))
    .await;
    assert_eq!(message.stop_reason, StopReason::Error);
    assert_eq!(
        message.error_message.as_deref(),
        Some("No API key for provider: google")
    );
}
