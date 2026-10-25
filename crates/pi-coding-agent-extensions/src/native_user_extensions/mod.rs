//! Native ports selected by the user's existing extension paths.

use std::sync::Arc;

use crate::core::extensions::ExtensionRegistry;

pub mod disable_broadcast;

/// Register native user ports without loading them until a user path selects them.
pub fn register(registry: &mut ExtensionRegistry) {
    registry.register_hidden(
        "<native:disable-broadcast>",
        Arc::new(|pi| {
            let result = disable_broadcast::disable_broadcast_extension(pi);
            Box::pin(async move { result })
        }),
    );
}
