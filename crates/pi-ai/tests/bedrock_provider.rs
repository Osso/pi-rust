mod bedrock_support;

use std::sync::Arc;

use bedrock_support::{ScriptedFetch, ScriptedResponse, env, message_start, message_stop, model, text_delta};
use pi_ai::bedrock_provider::{bedrock_converse_stream_api, bedrock_provider_module, set_bedrock_provider_module};
use pi_ai::request::{ProviderRequestOptions, ProviderStreams, SimpleStreamOptions, StreamOptions, error_stream};
use pi_ai::types::{Context, Model, UserContent, UserMessage, UserRole};
use pi_ai::utils::event_stream::AssistantMessageEventStream;

struct MarkerProvider;

impl ProviderStreams for MarkerProvider {
    fn stream(&self, model: Model, _context: Context, _options: StreamOptions) -> AssistantMessageEventStream {
        error_stream(&model, "marker provider")
    }

    fn stream_simple(
        &self,
        model: Model,
        _context: Context,
        _options: SimpleStreamOptions,
    ) -> AssistantMessageEventStream {
        error_stream(&model, "marker provider")
    }
}

fn context() -> Context {
    Context {
        system_prompt: None,
        messages: vec![pi_ai::types::Message::User(UserMessage {
            role: UserRole::Tag,
            content: UserContent::Text(("hi".to_owned()).into()),
            input_source: None,
            provider_native: None,
            timestamp: 0,
        })],
        tools: None,
    }
}

fn options(fetch: Arc<ScriptedFetch>) -> StreamOptions {
    StreamOptions {
        request: ProviderRequestOptions {
            fetch: Some(fetch),
            env: Some(env()),
            ..Default::default()
        },
        ..Default::default()
    }
}

#[tokio::test]
async fn provider_module_and_lazy_override_match_the_typescript_exports() {
    let body = [message_start(), text_delta(0, "Hello"), message_stop("end_turn")].concat();
    let fetch = ScriptedFetch::new(vec![ScriptedResponse::ok(body, &[])]);
    let mut stream = bedrock_provider_module().stream(model(), context(), options(fetch));
    let mut text = pi_js_compat::js_json::JsString::default();
    while let Some(event) = stream.next().await {
        if let pi_ai::types::AssistantMessageEvent::TextDelta { delta, .. } = event.expect("event") {
            text.push_str(&delta);
        }
    }
    let message = stream.result().await.expect("message");
    assert_eq!(text, "Hello");
    assert_eq!(message.stop_reason, pi_ai::types::StopReason::Stop);

    set_bedrock_provider_module(Arc::new(MarkerProvider));
    let api = bedrock_converse_stream_api();
    let fetch = ScriptedFetch::new(vec![]);
    let message = api
        .stream(model(), context(), options(fetch))
        .result()
        .await
        .expect("marker result");
    assert_eq!(
        message.error_message.as_ref(),
        Some(&pi_js_compat::js_json::JsString::from("marker provider"))
    );
}
