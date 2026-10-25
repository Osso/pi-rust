use crate::shared_oracle as oracle;
use pi_ai::api::{
    anthropic_messages::AnthropicMessagesApi, azure_openai_responses::AzureOpenAIResponsesApi,
    google_generative_ai::GoogleGenerativeAiApi, mistral_conversations::MistralConversationsApi,
    openai_completions::OpenAICompletionsApi, openai_responses::OpenAIResponsesApi,
};
use pi_ai::request::{ProviderStreams, SimpleStreamOptions};
use serde_json::{Value, json};

#[tokio::test]
async fn direct_simple_auth_rejections_match_the_frozen_acquisition_boundary() {
    let apis: Vec<(&str, Box<dyn ProviderStreams>)> = vec![
        ("anthropic-messages", Box::new(AnthropicMessagesApi)),
        ("azure-openai-responses", Box::new(AzureOpenAIResponsesApi)),
        ("google-generative-ai", Box::new(GoogleGenerativeAiApi)),
        ("mistral-conversations", Box::new(MistralConversationsApi)),
        ("openai-completions", Box::new(OpenAICompletionsApi)),
        ("openai-responses", Box::new(OpenAIResponsesApi)),
    ];
    let mut mismatches = Vec::new();
    for (api, implementation) in apis {
        let mut source = oracle::TsOracle::spawn(
            "api-simple-acquisition-oracle.ts",
            &format!("packages/ai/src/api/{api}.ts"),
        );
        let model = json!({"id":"test","name":"test","api":api,"provider":"fixture-provider",
            "baseUrl":"https://unused.invalid","reasoning":false,"input":["text"],
            "cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},"contextWindow":8192,"maxTokens":2048});
        for api_key in [None, Some("")] {
            let mut options = SimpleStreamOptions::default();
            options.stream.request.api_key = api_key.map(Into::into);
            let mut values = json!({});
            if let Some(api_key) = api_key {
                values["apiKey"] = json!(api_key);
            }
            let expected = source.request(&json!({"model":model,"options":values}));
            assert_eq!(
                expected["acquired"],
                Value::Bool(false),
                "source unexpectedly acquired {api}"
            );
            let actual = match implementation.stream_simple(
                serde_json::from_value(model.clone()).unwrap(),
                serde_json::from_value(json!({"messages":[]})).unwrap(),
                options,
            ) {
                Ok(_) => json!({"acquired":true}),
                Err(error) => json!({"acquired":false,"name":error.name,"message":error.message}),
            };
            if actual != expected {
                mismatches.push(format!("{api}/{api_key:?}: {actual} != {expected}"));
            }
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}
