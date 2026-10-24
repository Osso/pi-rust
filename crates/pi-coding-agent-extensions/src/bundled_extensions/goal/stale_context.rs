//! Port of `packages/coding-agent/extensions/goal/src/stale-context.ts`.

const STALE_CONTEXT_ERROR_PREFIX: &str = "This extension ctx is stale after session replacement or reload.";

/// `isStaleContextError`.
pub fn is_stale_context_error(message: &str) -> bool {
    message.starts_with(STALE_CONTEXT_ERROR_PREFIX)
}
