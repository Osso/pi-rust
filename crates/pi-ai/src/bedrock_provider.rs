//! Port of `packages/ai/src/bedrock-provider.ts` and
//! `packages/ai/src/api/bedrock-converse-stream.lazy.ts`.

use std::sync::{Arc, Mutex, OnceLock};

use crate::api::bedrock_converse_stream::BedrockProviderModule;
use crate::api::lazy::{LazyApi, lazy_api};
use crate::request::{BoxFuture, CallbackError, ProviderStreams};

/// `bedrockProviderModule`.
pub fn bedrock_provider_module() -> BedrockProviderModule {
    BedrockProviderModule
}

static OVERRIDE: OnceLock<Mutex<Option<Arc<dyn ProviderStreams>>>> = OnceLock::new();

fn override_slot() -> &'static Mutex<Option<Arc<dyn ProviderStreams>>> {
    OVERRIDE.get_or_init(|| Mutex::new(None))
}

/// `setBedrockProviderModule`: replaces the dynamically loaded implementation.
pub fn set_bedrock_provider_module(module: Arc<dyn ProviderStreams>) {
    *override_slot().lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(module);
}

/// `bedrockConverseStreamApi`.
pub fn bedrock_converse_stream_api() -> LazyApi {
    lazy_api(Arc::new(|| {
        Box::pin(async move {
            let module = override_slot()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone()
                .unwrap_or_else(|| Arc::new(BedrockProviderModule) as Arc<dyn ProviderStreams>);
            Ok(module)
        }) as BoxFuture<'static, Result<Arc<dyn ProviderStreams>, CallbackError>>
    }))
}
