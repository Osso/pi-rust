#[path = "support/oracle.rs"]
mod oracle;
use pi_ai::auth::{ApiKeyAuth, ApiKeyResolveInput, AuthError, AuthResult, ModelAuth, ProviderAuth};
use pi_ai::models::{
    CreateModelsOptions, CreateProviderOptions, Models, MutableModels, ProviderApi, create_models, create_provider,
};
use pi_ai::request::{BoxFuture, ProviderStreams};
use pi_ai::types::{Context, Model};
use pi_ai::utils::event_stream::AssistantMessageEventStream;
use serde_json::json;
use std::sync::Arc;

struct Ambient;
impl ApiKeyAuth for Ambient {
    fn name(&self) -> &str {
        "Ambient"
    }
    fn resolve(&self, _: ApiKeyResolveInput) -> BoxFuture<'static, Result<Option<AuthResult>, AuthError>> {
        Box::pin(async {
            Ok(Some(AuthResult {
                auth: ModelAuth::default(),
                env: None,
                source: None,
            }))
        })
    }
}
struct UnusedStreams;
impl ProviderStreams for UnusedStreams {
    fn stream(&self, _: Model, _: Context, _: pi_ai::request::StreamOptions) -> AssistantMessageEventStream {
        unreachable!("must not stream")
    }
    fn stream_simple(
        &self,
        _: Model,
        _: Context,
        _: pi_ai::request::SimpleStreamOptions,
    ) -> AssistantMessageEventStream {
        unreachable!("must not stream")
    }
}
fn model(id: &str) -> Model {
    serde_json::from_value(json!({"id":id,"name":id,"api":"fixture","provider":"dynamic","baseUrl":"http://fixture.invalid","reasoning":false,"input":["text"],"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},"contextWindow":4096,"maxTokens":512})).unwrap()
}
#[tokio::test]
async fn refresh_replaces_the_provider_model_list() {
    let models = create_models(CreateModelsOptions::default());
    models.set_provider(create_provider(CreateProviderOptions {
        id: "dynamic".into(),
        name: None,
        base_url: None,
        headers: None,
        auth: ProviderAuth::api_key(Arc::new(Ambient)),
        models: vec![model("old")],
        refresh_models: Some(Arc::new(|| Box::pin(async { Ok(vec![model("new")]) }))),
        api: ProviderApi::Single(Arc::new(UnusedStreams)),
    }));
    let before: Vec<_> = models
        .get_models(Some("dynamic"))
        .iter()
        .map(|model| model.id.clone())
        .collect();
    models.refresh(Some("dynamic")).await.unwrap();
    let actual = json!({"before":before,"after":models.get_models(Some("dynamic")).iter().map(|model| model.id.clone()).collect::<Vec<_>>()});
    let mut source = oracle::TsOracle::spawn("models-contract-oracle.ts", "packages/ai/src/models.ts");
    let expected = source.request(&json!({"id":1,"initial":[model("old")],"refreshed":[model("new")]}));
    assert_eq!(expected["ok"], true, "{expected}");
    assert_eq!(actual, expected["output"]);
}
