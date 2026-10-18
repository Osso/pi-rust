//! Port of `packages/ai/src/image-models.ts`.

use crate::image_models_generated::image_model_groups;
use crate::types::ImagesModel;

/// `getImageModel(provider, modelId)`.
///
/// The TypeScript signature claims a model but returns `undefined` for unknown
/// provider/model pairs; Rust makes that case explicit with `Option`.
pub fn get_image_model(provider: &str, model_id: &str) -> Option<&'static ImagesModel> {
    get_image_models(provider).iter().find(|model| model.id == model_id)
}

/// `getImageProviders()`.
pub fn get_image_providers() -> Vec<&'static str> {
    image_model_groups()
        .iter()
        .map(|(provider, _)| provider.as_str())
        .collect()
}

/// `getImageModels(provider)`; unknown providers yield an empty slice.
pub fn get_image_models(provider: &str) -> &'static [ImagesModel] {
    image_model_groups()
        .iter()
        .find(|(name, _)| name == provider)
        .map_or(&[], |(_, models)| models.as_slice())
}
