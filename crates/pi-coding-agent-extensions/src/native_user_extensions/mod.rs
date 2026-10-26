//! Opt-in user extensions, keyed by their package names.
use crate::core::extensions::ExtensionRegistry;
use std::sync::Arc;
pub mod pi_atlas;

pub fn register(registry: &mut ExtensionRegistry) {
    registry.register_hidden(
        "<native:@mohndoe/pi-atlas>",
        Arc::new(|api| {
            let result = pi_atlas::command::extension(api);
            Box::pin(async move { result })
        }),
    );
}
