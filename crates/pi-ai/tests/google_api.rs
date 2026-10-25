//! Behavioral tests for the Google adapters that the differential oracle cannot cover.

#[path = "support/google_fetch.rs"]
mod google_fetch;

use std::sync::Arc;

use google_fetch::{RecordingFetch, ScriptedResponse};
use pi_ai::api::google_generative_ai::{google_generative_ai_api, stream_simple as gemini_stream_simple};
use pi_ai::api::google_vertex::{google_vertex_api, stream_simple as vertex_stream_simple};
use pi_ai::request::{ProviderRequestOptions, ProviderStreams, SimpleStreamOptions, StreamOptions};
use pi_ai::types::{Context, Model};
use pi_ai::utils::event_stream::AssistantMessageEventStream;
use rsa::pkcs1v15::{Signature, VerifyingKey};
use rsa::pkcs8::DecodePrivateKey;
use rsa::signature::Verifier;
use serde_json::{Value, json};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

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

fn vertex_model(id: &str) -> Model {
    serde_json::from_value(json!({
        "id": id, "name": id, "api": "google-vertex", "provider": "google-vertex",
        "baseUrl": "https://{location}-aiplatform.googleapis.com",
        "reasoning": true,
        "input": ["text", "image"],
        "cost": {"input": 0.3, "output": 2.5, "cacheRead": 0.03, "cacheWrite": 0.0},
        "contextWindow": 1048576, "maxTokens": 65536
    }))
    .expect("vertex model")
}

fn context() -> Context {
    Context {
        system_prompt: None,
        messages: vec![pi_ai::types::Message::User(pi_ai::types::UserMessage {
            role: Default::default(),
            content: pi_ai::types::UserContent::Text(("hello".to_owned()).into()),
            input_source: None,
            provider_native: None,
            timestamp: 1.0,
        })],
        tools: None,
    }
}

async fn collect(mut stream: AssistantMessageEventStream) -> pi_ai::types::AssistantMessage {
    while stream.next().await.is_some() {}
    stream.result().await.expect("stream result")
}

fn base64url_decode(text: &str) -> Vec<u8> {
    let mut bits = Vec::new();
    for byte in text.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'-' | b'+' => 62,
            b'_' | b'/' => 63,
            _ => continue,
        };
        bits.push(value);
    }
    let mut bytes = Vec::new();
    for chunk in bits.chunks(4) {
        let mut buffer = 0_u32;
        for (index, value) in chunk.iter().enumerate() {
            buffer |= u32::from(*value) << (18 - index * 6);
        }
        bytes.push((buffer >> 16) as u8);
        if chunk.len() > 2 {
            bytes.push((buffer >> 8) as u8);
        }
        if chunk.len() > 3 {
            bytes.push(buffer as u8);
        }
    }
    bytes
}

fn token_response() -> ScriptedResponse {
    ScriptedResponse::json(
        200,
        r#"{"access_token":"test-token","expires_in":3600,"token_type":"Bearer"}"#,
    )
}

fn text_stream() -> ScriptedResponse {
    ScriptedResponse::sse(&[
        "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"ok\"}]},\"finishReason\":\"STOP\"}]}\n\n",
    ])
}

fn vertex_options(env: Value, max_retries: Option<f64>, fetch: Arc<dyn pi_ai::request::Fetch>) -> StreamOptions {
    StreamOptions {
        request: ProviderRequestOptions {
            fetch: Some(fetch),
            max_retries,
            env: Some(
                env.as_object()
                    .expect("env object")
                    .iter()
                    .map(|(key, value)| {
                        (
                            key.clone(),
                            pi_js_compat::js_json::JsString::from(value.as_str().unwrap_or_default()),
                        )
                    })
                    .collect(),
            ),
            ..Default::default()
        },
        extra: [
            ("project".to_owned(), json!("test-project")),
            ("location".to_owned(), json!("us-central1")),
        ]
        .into_iter()
        .collect(),
        ..Default::default()
    }
}

#[tokio::test]
async fn gemini_stream_simple_without_an_api_key_rejects_acquisition() {
    let Err(error) = gemini_stream_simple(
        gemini_model("gemini-2.5-flash"),
        context(),
        SimpleStreamOptions::default(),
    ) else {
        panic!("direct streamSimple must reject before returning a stream");
    };
    assert_eq!(error.message, "No API key for provider: google");
}

#[tokio::test]
async fn vertex_stream_simple_without_project_or_env_reports_the_typescript_message() {
    let message = collect(vertex_stream_simple(
        vertex_model("gemini-3-flash-preview"),
        context(),
        SimpleStreamOptions::default(),
    ))
    .await;
    assert_eq!(message.stop_reason, pi_ai::types::StopReason::Error);
    assert_eq!(
        message.error_message.as_ref(),
        Some(&pi_js_compat::js_json::JsString::from(
            "Vertex AI requires a project ID. Set GOOGLE_CLOUD_PROJECT/GCLOUD_PROJECT or pass project in options."
        ))
    );
}

#[tokio::test]
async fn lazy_api_wrappers_expose_the_provider_streams_contract() {
    let gemini = google_generative_ai_api();
    let message = collect(
        gemini
            .stream(gemini_model("gemini-2.5-flash"), context(), StreamOptions::default())
            .expect("acquisition"),
    )
    .await;
    assert_eq!(
        message.error_message.as_ref(),
        Some(&pi_js_compat::js_json::JsString::from(
            "No API key for provider: google"
        ))
    );

    let vertex = google_vertex_api();
    let message = collect(
        vertex
            .stream(
                vertex_model("gemini-3-flash-preview"),
                context(),
                StreamOptions::default(),
            )
            .expect("acquisition"),
    )
    .await;
    assert_eq!(
        message.error_message.as_ref(),
        Some(&pi_js_compat::js_json::JsString::from(
            "Vertex AI requires a project ID. Set GOOGLE_CLOUD_PROJECT/GCLOUD_PROJECT or pass project in options."
        ))
    );
}

#[tokio::test]
async fn service_account_tokens_are_signed_rs256_with_the_google_claims() {
    let credentials = format!("{FIXTURES}/service-account.json");
    let fetch = RecordingFetch::new(vec![token_response(), text_stream()]);
    let options = vertex_options(
        json!({"GOOGLE_APPLICATION_CREDENTIALS": credentials}),
        None,
        fetch.clone(),
    );
    let message = collect(pi_ai::api::google_vertex::stream(
        vertex_model("gemini-3-flash-preview"),
        context(),
        options,
    ))
    .await;
    assert_eq!(message.stop_reason, pi_ai::types::StopReason::Stop, "{message:?}");

    let requests = fetch.requests();
    assert_eq!(requests.len(), 2);
    let body = String::from_utf8(requests[0].body.clone().expect("token body")).expect("utf8");
    let assertion = body
        .split('&')
        .find_map(|pair| pair.strip_prefix("assertion="))
        .expect("assertion");
    let mut parts = assertion.split('.');
    let header: Value = serde_json::from_slice(&base64url_decode(parts.next().unwrap())).expect("header");
    let claims: Value = serde_json::from_slice(&base64url_decode(parts.next().unwrap())).expect("claims");
    let signature = base64url_decode(parts.next().unwrap());
    assert_eq!(header, json!({"alg": "RS256"}));
    assert_eq!(claims["iss"], "test@test-project.iam.gserviceaccount.com");
    assert_eq!(claims["scope"], "https://www.googleapis.com/auth/cloud-platform");
    assert_eq!(claims["aud"], "https://oauth2.googleapis.com/token");
    assert_eq!(claims["exp"].as_i64().unwrap() - claims["iat"].as_i64().unwrap(), 3600);

    let private_key = std::fs::read_to_string(format!("{FIXTURES}/service-account-key.pem")).expect("key");
    let key = rsa::RsaPrivateKey::from_pkcs8_pem(&private_key).expect("private key");
    let verifying_key = VerifyingKey::<rsa::sha2::Sha256>::new(key.to_public_key());
    let signing_input = assertion.rsplit_once('.').expect("signature").0;
    verifying_key
        .verify(
            signing_input.as_bytes(),
            &Signature::try_from(signature.as_slice()).expect("signature"),
        )
        .expect("RS256 signature verifies");

    assert_eq!(requests[0].url, "https://oauth2.googleapis.com/token");
    assert_eq!(
        requests[1].headers.iter().find(|(name, _)| name == "authorization"),
        Some(&("authorization".to_owned(), "Bearer test-token".to_owned()))
    );
}

#[tokio::test]
async fn authorized_user_tokens_use_the_refresh_token_grant() {
    let credentials = format!("{FIXTURES}/authorized-user.json");
    let fetch = RecordingFetch::new(vec![token_response(), text_stream()]);
    let options = vertex_options(
        json!({"GOOGLE_APPLICATION_CREDENTIALS": credentials}),
        None,
        fetch.clone(),
    );
    let message = collect(pi_ai::api::google_vertex::stream(
        vertex_model("gemini-3-flash-preview"),
        context(),
        options,
    ))
    .await;
    assert_eq!(message.stop_reason, pi_ai::types::StopReason::Stop, "{message:?}");

    let body = String::from_utf8(fetch.requests()[0].body.clone().expect("token body")).expect("utf8");
    assert_eq!(
        body,
        "refresh_token=refresh-token&client_id=client-id.apps.googleusercontent.com&client_secret=client-secret&grant_type=refresh_token"
    );
    assert_eq!(
        fetch.requests()[0].headers,
        vec![
            ("accept".to_owned(), "*/*".to_owned()),
            (
                "content-type".to_owned(),
                "application/x-www-form-urlencoded;charset=UTF-8".to_owned()
            )
        ]
    );
}

#[tokio::test]
async fn vertex_does_not_add_provider_retries_after_token_authentication() {
    let credentials = format!("{FIXTURES}/service-account.json");
    let fetch = RecordingFetch::new(vec![
        token_response(),
        ScriptedResponse::json(500, r#"{"error":{"message":"boom"}}"#).with_header("retry-after", "0.001"),
        text_stream(),
    ]);
    let options = vertex_options(
        json!({"GOOGLE_APPLICATION_CREDENTIALS": credentials}),
        Some(1.0),
        fetch.clone(),
    );
    let message = collect(pi_ai::api::google_vertex::stream(
        vertex_model("gemini-3-flash-preview"),
        context(),
        options,
    ))
    .await;
    assert_eq!(message.stop_reason, pi_ai::types::StopReason::Error, "{message:?}");
    assert_eq!(
        message.error_message.as_ref().unwrap(),
        r#"{"error":{"message":"boom"}}"#
    );
    let requests = fetch.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].url, "https://oauth2.googleapis.com/token");
    assert!(requests[1].url.contains(":streamGenerateContent"));
}

#[tokio::test]
async fn vertex_transport_failures_are_not_retried() {
    let fetch = RecordingFetch::new(vec![ScriptedResponse::throw("TypeError", "fetch failed")]);
    let options = StreamOptions {
        request: ProviderRequestOptions {
            api_key: Some(("AIzaSyRealKey".to_owned()).into()),
            fetch: Some(fetch.clone()),
            max_retries: Some(2.0),
            ..Default::default()
        },
        ..Default::default()
    };
    let message = collect(pi_ai::api::google_vertex::stream(
        vertex_model("gemini-3-flash-preview"),
        context(),
        options,
    ))
    .await;
    assert_eq!(message.stop_reason, pi_ai::types::StopReason::Error);
    assert_eq!(
        message.error_message.as_ref(),
        Some(&pi_js_compat::js_json::JsString::from("fetch failed"))
    );
    assert_eq!(fetch.requests().len(), 1);
}

#[tokio::test]
async fn aborted_vertex_streams_report_an_aborted_message() {
    let signal_controller = pi_js_compat::abort::AbortController::new();
    let signal = signal_controller.signal();
    let fetch = RecordingFetch::with_abort_after_chunks(
        vec![ScriptedResponse::sse(&[
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"partial\"}]}}]}\n\n",
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\" more\"}]},\"finishReason\":\"STOP\"}]}\n\n",
        ])],
        1,
        signal_controller.clone(),
    );
    let options = StreamOptions {
        request: ProviderRequestOptions {
            api_key: Some(("AIzaSyRealKey".to_owned()).into()),
            fetch: Some(fetch),
            signal: Some(signal),
            ..Default::default()
        },
        ..Default::default()
    };
    let message = collect(pi_ai::api::google_vertex::stream(
        vertex_model("gemini-3-flash-preview"),
        context(),
        options,
    ))
    .await;
    assert_eq!(message.stop_reason, pi_ai::types::StopReason::Aborted);
    assert_eq!(
        message.error_message.as_ref(),
        Some(&pi_js_compat::js_json::JsString::from("This operation was aborted"))
    );
}
