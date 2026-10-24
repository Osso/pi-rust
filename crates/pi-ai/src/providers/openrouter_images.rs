//! Port of `packages/ai/src/providers/openrouter-images.ts`.

use crate::api::openrouter_images_lazy::openrouter_images_api;
use crate::auth::{ProviderAuth, env_api_key_auth};
use crate::image_models::get_image_models;
use crate::images_models::{CreateImagesProviderOptions, ImagesProvider, create_images_provider};
use std::sync::Arc;

/// `openrouterImagesProvider()`.
pub fn openrouter_images_provider() -> Arc<dyn ImagesProvider> {
    create_images_provider(CreateImagesProviderOptions {
        id: "openrouter".to_owned(),
        name: Some("OpenRouter".to_owned()),
        auth: ProviderAuth::api_key(Arc::new(env_api_key_auth(
            "OpenRouter API key",
            &["OPENROUTER_API_KEY"],
        ))),
        models: get_image_models("openrouter").to_vec(),
        refresh_models: None,
        api: openrouter_images_api(),
    })
}
