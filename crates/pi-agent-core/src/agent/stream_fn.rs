//! Port of `packages/agent/src/stream-fn.ts`.

use std::sync::{OnceLock, RwLock};

use crate::agent::types::{AgentError, StreamFn};

fn default_stream_fn_slot() -> &'static RwLock<Option<StreamFn>> {
    static SLOT: OnceLock<RwLock<Option<StreamFn>>> = OnceLock::new();
    SLOT.get_or_init(|| RwLock::new(None))
}

/// Installs or clears the process-global fallback stream function.
pub fn set_default_stream_fn(stream_fn: Option<StreamFn>) {
    let mut slot = default_stream_fn_slot()
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *slot = stream_fn;
}

/// Returns the process-global fallback stream function.
pub fn get_default_stream_fn() -> Result<StreamFn, AgentError> {
    default_stream_fn_slot()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
        .ok_or_else(AgentError::default_stream_fn_missing)
}
