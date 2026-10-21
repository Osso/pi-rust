//! Adapter for the frozen core's `pi-ai/compat` default stream function.

use std::sync::Arc;

use crate::agent::types::StreamFn;

pub(crate) fn default_stream_fn() -> StreamFn {
    Arc::new(|model, context, options| Box::pin(async move { pi_ai::compat::stream_simple(model, context, options) }))
}
