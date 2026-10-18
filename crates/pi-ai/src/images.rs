//! Port of `packages/ai/src/images.ts`.

use crate::images_api_registry::get_images_api_provider;
use crate::providers::images::register_builtins::ensure_built_in_images_api_providers;
use crate::request::{BoxFuture, CallbackError, ImagesOptions};
use crate::types::{AssistantImages, ImagesContext, ImagesModel};

/// `generateImages(model, context, options?)`.
///
/// Registers the bundled image APIs on first use (the TypeScript module does
/// this through an import side effect) and throws-equivalent `Err` when no API
/// provider is registered for `model.api`.
pub fn generate_images(
    model: ImagesModel,
    context: ImagesContext,
    options: ImagesOptions,
) -> BoxFuture<'static, Result<AssistantImages, CallbackError>> {
    ensure_built_in_images_api_providers();
    let provider = get_images_api_provider(&model.api);
    let api = model.api.clone();
    Box::pin(async move {
        let Some(provider) = provider else {
            return Err(CallbackError::new(format!("No API provider registered for api: {api}")));
        };
        provider.generate_images(model, context, options).await
    })
}
