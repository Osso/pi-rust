// Scripted stream fixture, registered through the frozen ProviderConfigInput API.
// Provider auth methods are deliberately not imported into the coding-agent registry.
use pi_ai::models::Provider;
use pi_coding_agent::core::model_registry::{ModelRegistry, ProviderConfigInput};
use std::sync::Arc;

pub fn register_scripted_provider(registry: &ModelRegistry, provider: Arc<dyn Provider>) {
    let models = provider.get_models().unwrap();
    let first = models.first().expect("scripted registry fixture requires a model");
    let id = provider.id().to_owned();
    let config = ProviderConfigInput {
        name: Some(provider.name().into()),
        api: Some(first.api.clone()),
        base_url: Some(first.base_url.clone()),
        api_key: Some("fixture".into()),
        models: Some(models),
        stream_simple: Some(Arc::new(move |model, context, options| {
            provider.stream_simple(model, context, options.unwrap_or_default())
        })),
        ..Default::default()
    };
    registry
        .register_provider(&id, config)
        .expect("scripted provider registration");
}
