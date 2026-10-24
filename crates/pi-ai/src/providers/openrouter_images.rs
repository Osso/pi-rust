//! Port of `packages/ai/src/providers/openrouter-images.ts`.

use std::sync::Arc;

use crate::api::openrouter_images_lazy::openrouter_images_api;
use crate::auth::{AuthError, LazyOAuthInput, OAuthLoader, ProviderAuth, env_api_key_auth, lazy_oauth};
use crate::image_models::get_image_models;
use crate::images_models::{CreateImagesProviderOptions, ImagesProvider, create_images_provider};

/// `openrouterImagesProvider()`.
pub fn openrouter_images_provider() -> Arc<dyn ImagesProvider> {
    create_images_provider(CreateImagesProviderOptions {
        id: "openrouter".to_owned(),
        name: Some("OpenRouter".to_owned()),
        auth: ProviderAuth {
            api_key: Some(Arc::new(env_api_key_auth(
                "OpenRouter API key",
                &["OPENROUTER_API_KEY"],
            ))),
            oauth: Some(Arc::new(lazy_oauth(LazyOAuthInput {
                name: "OpenRouter OAuth".into(),
                load: openrouter_oauth_loader(),
            }))),
        },
        models: get_image_models("openrouter").to_vec(),
        refresh_models: None,
        api: openrouter_images_api(),
    })
}

/// Loader for `auth/oauth/openrouter.ts`, which is outside this slice.
fn openrouter_oauth_loader() -> OAuthLoader {
    Arc::new(|| {
        Box::pin(async {
            Err(AuthError::failed(
                "OpenRouter OAuth flow is not ported (auth/oauth/openrouter.ts)",
            ))
        })
    })
}
