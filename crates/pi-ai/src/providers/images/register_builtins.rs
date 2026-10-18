//! Port of `packages/ai/src/providers/images/register-builtins.ts`.
//!
//! Rust links the OpenRouter module directly, so the TypeScript lazy-import
//! failure branch cannot occur.

use std::sync::{Arc, OnceLock};

use crate::api::openrouter_images::generate_images as generate_images_openrouter_api;
use crate::images_api_registry::{ImagesApiProvider, register_images_api_provider};
use crate::request::{BoxFuture, CallbackError, ImagesOptions};
use crate::types::{AssistantImages, ImagesContext, ImagesModel};

/// `generateImagesOpenRouter`.
pub fn generate_images_openrouter(
    model: ImagesModel,
    context: ImagesContext,
    options: ImagesOptions,
) -> BoxFuture<'static, Result<AssistantImages, CallbackError>> {
    generate_images_openrouter_api(model, context, options)
}

/// `registerBuiltInImagesApiProviders()`.
pub fn register_built_in_images_api_providers() {
    register_images_api_provider(ImagesApiProvider {
        api: "openrouter-images".to_owned(),
        generate_images: Arc::new(crate::api::openrouter_images::OpenRouterImagesApi),
    });
}

/// Applies the module's import-time registration once.
pub fn ensure_built_in_images_api_providers() {
    static REGISTERED: OnceLock<()> = OnceLock::new();
    REGISTERED.get_or_init(register_built_in_images_api_providers);
}
