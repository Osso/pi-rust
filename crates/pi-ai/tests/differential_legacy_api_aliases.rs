//! Differential test for `packages/ai/src/legacy-api-aliases.ts`.

use std::sync::Arc;

use pi_ai::legacy_api_aliases::{
    stream_anthropic, stream_azure_openai_responses, stream_google, stream_google_vertex, stream_mistral,
    stream_openai_codex_responses, stream_openai_completions, stream_openai_responses, stream_simple_anthropic,
    stream_simple_azure_openai_responses, stream_simple_google, stream_simple_google_vertex, stream_simple_mistral,
    stream_simple_openai_codex_responses, stream_simple_openai_completions, stream_simple_openai_responses,
};
use pi_ai::request::{ProviderRequestOptions, SimpleStreamOptions, StreamOptions};
use pi_ai::types::{
    Context, Message, Modality, Model, ModelCost, ModelCostRates, Transport, UserContent, UserMessage, UserRole,
};
use serde_json::{Value, json};

#[path = "support/oracle.rs"]
mod oracle;

#[allow(dead_code)]
#[path = "support/recording_fetch.rs"]
mod recording_fetch;

use recording_fetch::{RecordingFetch, ScriptedResponse};

const SOURCE_MODULE: &str = "packages/ai/src/legacy-api-aliases.ts";

const CODEX_TOKEN: &str =
    "eyJhbGciOiJub25lIn0.eyJodHRwczovL2FwaS5vcGVuYWkuY29tL2F1dGgiOnsiY2hhdGdwdF9hY2NvdW50X2lkIjoiYWNjdF90ZXN0In19.sig";

struct Case {
    name: &'static str,
    api: &'static str,
    provider: &'static str,
    base_url: &'static str,
    token: Option<&'static str>,
    transport: Option<Transport>,
}

const CASES: [Case; 8] = [
    Case {
        name: "Anthropic",
        api: "anthropic-messages",
        provider: "anthropic",
        base_url: "https://api.anthropic.com",
        token: None,
        transport: None,
    },
    Case {
        name: "AzureOpenAIResponses",
        api: "azure-openai-responses",
        provider: "azure-openai-responses",
        base_url: "https://example.test/openai",
        token: None,
        transport: None,
    },
    Case {
        name: "Google",
        api: "google-generative-ai",
        provider: "google",
        base_url: "https://example.test",
        token: None,
        transport: None,
    },
    Case {
        name: "GoogleVertex",
        api: "google-vertex",
        provider: "google-vertex",
        base_url: "https://example.test",
        token: None,
        transport: None,
    },
    Case {
        name: "Mistral",
        api: "mistral-conversations",
        provider: "mistral",
        base_url: "https://example.test",
        token: None,
        transport: None,
    },
    Case {
        name: "OpenAICodexResponses",
        api: "openai-codex-responses",
        provider: "openai-codex",
        base_url: "https://chatgpt.com/backend-api",
        token: Some(CODEX_TOKEN),
        transport: Some(Transport::Sse),
    },
    Case {
        name: "OpenAICompletions",
        api: "openai-completions",
        provider: "openai",
        base_url: "https://example.test/v1",
        token: None,
        transport: None,
    },
    Case {
        name: "OpenAIResponses",
        api: "openai-responses",
        provider: "openai",
        base_url: "https://example.test/v1",
        token: None,
        transport: None,
    },
];

fn model(case: &Case) -> Model {
    Model {
        properties: Default::default(),
        id: "test-model".to_owned(),
        name: "Test Model".into(),
        api: case.api.to_owned(),
        provider: case.provider.to_owned(),
        base_url: (case.base_url.to_owned()).into(),
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

fn context() -> Context {
    Context {
        system_prompt: None,
        messages: vec![Message::User(UserMessage {
            role: UserRole::Tag,
            content: UserContent::Text(("hi".to_owned()).into()),
            input_source: None,
            provider_native: None,
            timestamp: 0.0,
        })],
        tools: None,
    }
}

fn stream_options(case: &Case, fetch: Arc<RecordingFetch>) -> StreamOptions {
    StreamOptions {
        request: ProviderRequestOptions {
            api_key: Some((case.token.unwrap_or("test-key").to_owned()).into()),
            fetch: Some(fetch as Arc<dyn pi_ai::request::Fetch>),
            ..ProviderRequestOptions::default()
        },
        transport: case.transport,
        ..StreamOptions::default()
    }
}

fn simple_options(case: &Case, fetch: Arc<RecordingFetch>) -> SimpleStreamOptions {
    SimpleStreamOptions {
        stream: stream_options(case, fetch),
        ..SimpleStreamOptions::default()
    }
}

fn fetch() -> Arc<RecordingFetch> {
    RecordingFetch::new(vec![ScriptedResponse::json(
        400,
        &json!({"error": {"message": "recorded"}}).to_string(),
    )])
}

#[tokio::test(flavor = "current_thread")]
async fn aliases_match_the_pinned_typescript_module() {
    let mut oracle = oracle::TsOracle::spawn("legacy-api-aliases-oracle.ts", SOURCE_MODULE);
    let expected = oracle.request(&json!({"id": 1, "op": "aliases"}));
    assert_eq!(expected["ok"], true, "{expected}");
    let expected = expected["output"]["results"].as_array().expect("results").clone();

    let mut results: Vec<Value> = Vec::new();
    for case in &CASES {
        let plain: fn(
            Model,
            Context,
            StreamOptions,
        ) -> Result<
            pi_ai::utils::event_stream::AssistantMessageEventStream,
            pi_ai::request::CallbackError,
        > = match case.name {
            "Anthropic" => stream_anthropic,
            "AzureOpenAIResponses" => stream_azure_openai_responses,
            "Google" => stream_google,
            "GoogleVertex" => stream_google_vertex,
            "Mistral" => stream_mistral,
            "OpenAICodexResponses" => stream_openai_codex_responses,
            "OpenAICompletions" => stream_openai_completions,
            "OpenAIResponses" => stream_openai_responses,
            other => panic!("unknown case {other}"),
        };
        let simple: fn(
            Model,
            Context,
            SimpleStreamOptions,
        ) -> Result<
            pi_ai::utils::event_stream::AssistantMessageEventStream,
            pi_ai::request::CallbackError,
        > = match case.name {
            "Anthropic" => stream_simple_anthropic,
            "AzureOpenAIResponses" => stream_simple_azure_openai_responses,
            "Google" => stream_simple_google,
            "GoogleVertex" => stream_simple_google_vertex,
            "Mistral" => stream_simple_mistral,
            "OpenAICodexResponses" => stream_simple_openai_codex_responses,
            "OpenAICompletions" => stream_simple_openai_completions,
            "OpenAIResponses" => stream_simple_openai_responses,
            other => panic!("unknown case {other}"),
        };

        for is_simple in [false, true] {
            let fetch = fetch();
            let stream = if is_simple {
                simple(model(case), context(), simple_options(case, Arc::clone(&fetch)))
            } else {
                plain(model(case), context(), stream_options(case, Arc::clone(&fetch)))
            };
            let message = stream.expect("acquisition").result().await.expect("stream result");
            let requests: Vec<Value> = fetch
                .requests()
                .into_iter()
                .map(|request| json!({"url": request.url, "method": request.method}))
                .collect();
            results.push(json!({
                "name": if is_simple { format!("streamSimple{}", case.name) } else { format!("stream{}", case.name) },
                "requests": requests,
                "stopReason": serde_json::to_value(message.stop_reason).expect("stop reason"),
                "errorMessage": message.error_message,
                "error": null,
            }));
        }
    }

    assert_eq!(Value::Array(results), Value::Array(expected));
}
