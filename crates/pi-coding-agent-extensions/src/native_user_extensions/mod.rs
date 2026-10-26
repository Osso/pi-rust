//! Native ports selected by the user's existing extension paths.

use std::sync::Arc;

use crate::core::extensions::ExtensionRegistry;

pub mod claude_bridge;
pub mod disable_broadcast;
pub mod regex_replace;

/// Register native user ports without loading them until a user path selects them.
pub fn register(registry: &mut ExtensionRegistry) {
    registry.register_hidden(
        "<native:regex-replace-pi-tool>",
        Arc::new(|pi| {
            let result = regex_replace::regex_replace_extension(pi);
            Box::pin(async move { result })
        }),
    );
    registry.register_hidden(
        "<native:disable-broadcast>",
        Arc::new(|pi| {
            let result = disable_broadcast::disable_broadcast_extension(pi);
            Box::pin(async move { result })
        }),
    );
}
