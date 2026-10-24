use indexmap::IndexMap;
use std::sync::Arc;

use pi_ai::auth::AuthContext;
use pi_ai::image_models::get_image_models;
use pi_ai::images_models::{ImagesModels, MutableImagesModels, create_images_models};
use pi_ai::providers::openrouter_images::openrouter_images_provider;
use pi_ai::request::BoxFuture;

struct FakeAuthContext {
    env: IndexMap<String, String>,
}

impl AuthContext for FakeAuthContext {
    fn env(
        &self,
        name: &str,
    ) -> BoxFuture<'_, Result<Option<pi_js_compat::js_json::JsString>, pi_ai::auth::AuthError>> {
        let value = self.env.get(name).cloned();
        Box::pin(async move { Ok(value.map(pi_js_compat::js_json::JsString::from)) })
    }

    fn file_exists(
        &self,
        _path: &pi_js_compat::js_json::JsString,
    ) -> BoxFuture<'_, Result<bool, pi_ai::auth::AuthError>> {
        Box::pin(async { Ok(false) })
    }
}

fn auth_context(pairs: &[(&str, &str)]) -> Arc<dyn AuthContext> {
    Arc::new(FakeAuthContext {
        env: pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect(),
    })
}

#[tokio::test]
async fn provider_metadata_and_catalog_match_the_generated_catalog() {
    let provider = openrouter_images_provider();
    assert_eq!(provider.id(), "openrouter");
    assert_eq!(provider.name(), "OpenRouter");
    let models = provider.get_models();
    assert_eq!(models, get_image_models("openrouter"));
    assert!(models.iter().all(|model| model.api == "openrouter-images"));
    assert!(provider.auth().api_key.is_some());
    assert!(provider.auth().oauth.is_none());
    assert!(!provider.supports_refresh_models());
}

#[tokio::test]
async fn api_key_auth_resolves_openrouter_api_key() {
    let models = create_images_models(pi_ai::models::CreateModelsOptions {
        credentials: None,
        auth_context: Some(auth_context(&[("OPENROUTER_API_KEY", "or-key")])),
    });
    models.set_provider(openrouter_images_provider());

    let resolved = models
        .get_auth(&models.get_models(Some("openrouter"))[0])
        .await
        .expect("auth resolves")
        .expect("configured");
    assert_eq!(
        resolved.auth.api_key.as_ref(),
        Some(&pi_js_compat::js_json::JsString::from("or-key"))
    );
    assert_eq!(
        resolved.source.as_ref(),
        Some(&pi_js_compat::js_json::JsString::from("OPENROUTER_API_KEY"))
    );
}

#[tokio::test]
async fn unconfigured_provider_reports_no_auth() {
    let models = create_images_models(pi_ai::models::CreateModelsOptions {
        credentials: None,
        auth_context: Some(auth_context(&[])),
    });
    models.set_provider(openrouter_images_provider());
    assert!(
        models
            .get_auth(&models.get_models(Some("openrouter"))[0])
            .await
            .expect("auth resolves")
            .is_none()
    );
}

#[tokio::test]
async fn builtin_style_collection_exposes_the_openrouter_catalog() {
    let models = create_images_models(pi_ai::models::CreateModelsOptions {
        credentials: None,
        auth_context: Some(auth_context(&[("OPENROUTER_API_KEY", "or-key")])),
    });
    models.set_provider(openrouter_images_provider());
    let providers: Vec<String> = models
        .get_providers()
        .iter()
        .map(|provider| provider.id().to_owned())
        .collect();
    assert_eq!(providers, ["openrouter"]);
    let catalog = models.get_models(Some("openrouter"));
    assert_eq!(catalog, get_image_models("openrouter"));
    assert!(catalog.iter().all(|model| model.api == "openrouter-images"));
    let first = catalog[0].clone();
    assert_eq!(
        models
            .get_auth(&first)
            .await
            .expect("auth resolves")
            .expect("configured")
            .auth
            .api_key
            .as_ref(),
        Some(&pi_js_compat::js_json::JsString::from("or-key"))
    );
}
