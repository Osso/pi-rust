//! Port of `packages/ai/src/api/anthropic-messages.lazy.ts`.

use std::sync::Arc;

use crate::api::anthropic_messages::AnthropicMessagesApi;
use crate::api::lazy::{LazyApi, lazy_api};
use crate::request::ProviderStreams;

/// `anthropicMessagesApi()`: loads the implementation on first use.
pub fn anthropic_messages_api() -> LazyApi {
    lazy_api(Arc::new(|| {
        Box::pin(async { Ok(Arc::new(AnthropicMessagesApi) as Arc<dyn ProviderStreams>) })
    }))
}
