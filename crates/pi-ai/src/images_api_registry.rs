//! Port of `packages/ai/src/images-api-registry.ts`.

use std::collections::BTreeMap;
use std::sync::{Arc, LazyLock, Mutex};

use crate::request::{BoxFuture, CallbackError, ImagesOptions, ProviderImages};
use crate::types::{AssistantImages, ImagesApi, ImagesContext, ImagesModel};

/// An image-generation API implementation registered under `api`.
pub struct ImagesApiProvider {
    pub api: ImagesApi,
    pub generate_images: Arc<dyn ProviderImages>,
}

static REGISTRY: LazyLock<Mutex<BTreeMap<String, Arc<dyn ProviderImages>>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));

struct RegisteredImagesApiProvider {
    api: ImagesApi,
    generate_images: Arc<dyn ProviderImages>,
}

impl ProviderImages for RegisteredImagesApiProvider {
    fn generate_images(
        &self,
        model: ImagesModel,
        context: ImagesContext,
        options: ImagesOptions,
    ) -> BoxFuture<'_, Result<AssistantImages, CallbackError>> {
        if model.api != self.api {
            let message = format!("Mismatched api: {} expected {}", model.api, self.api);
            return Box::pin(async move { Err(CallbackError::new(message)) });
        }
        self.generate_images.generate_images(model, context, options)
    }
}

/// `registerImagesApiProvider(provider)`.
///
/// The TypeScript `sourceId` argument is not observable through
/// `getImagesApiProvider`, so it is not part of the typed Rust API.
pub fn register_images_api_provider(provider: ImagesApiProvider) {
    let registered: Arc<dyn ProviderImages> = Arc::new(RegisteredImagesApiProvider {
        api: provider.api.clone(),
        generate_images: provider.generate_images,
    });
    lock().insert(provider.api, registered);
}

/// `getImagesApiProvider(api)`.
pub fn get_images_api_provider(api: &str) -> Option<Arc<dyn ProviderImages>> {
    lock().get(api).map(Arc::clone)
}

fn lock() -> std::sync::MutexGuard<'static, BTreeMap<String, Arc<dyn ProviderImages>>> {
    REGISTRY.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}
