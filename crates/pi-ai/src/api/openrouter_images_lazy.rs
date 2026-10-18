//! Port of `packages/ai/src/api/openrouter-images.lazy.ts`.
//!
//! Rust links the module directly; the function keeps the lazy module's
//! `ProviderImages` factory shape.

use std::sync::Arc;

use crate::request::ProviderImages;

use super::openrouter_images::OpenRouterImagesApi;

/// `openrouterImagesApi()`.
pub fn openrouter_images_api() -> Arc<dyn ProviderImages> {
    Arc::new(OpenRouterImagesApi)
}
