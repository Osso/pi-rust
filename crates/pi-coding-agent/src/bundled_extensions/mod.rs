//! Rust-native bundled extensions.

use std::sync::Arc;

use crate::core::extensions::loader::ExtensionRegistry;

pub mod agent_viewer;
pub mod agents_core;
pub mod agents_mailbox;
pub mod llama;

use agents_core::runtime::{MultiAgentExtensionOptions, create_multi_agent_runtime_handles, resolve_multi_agent_store};

/// Registers the first-party agents extensions under their source ids.
///
/// `main.ts` shares one store and runtime handle set across agents-core,
/// agent-viewer, agents-mailbox, and the Pyrun request handler; callers that
/// construct their own options can pass the same handles to preserve that.
pub fn register_agents_extensions(registry: &mut ExtensionRegistry, options: &MultiAgentExtensionOptions) {
    let store = resolve_multi_agent_store(options);
    let mut shared = options.clone();
    shared.store = Some(Arc::clone(&store));
    if shared.runtime_handles.is_none() {
        shared.runtime_handles = Some(create_multi_agent_runtime_handles());
    }

    let core_options = shared.clone();
    registry.register(
        "<first-party:agents-core>",
        Arc::new(move |pi| {
            let options = core_options.clone();
            let result = agents_core::runtime::multi_agent_extension(pi, &options);
            Box::pin(async move { result })
        }),
    );

    let viewer_options = shared.clone();
    registry.register(
        "<first-party:agent-viewer>",
        Arc::new(move |pi| {
            let options = viewer_options.clone();
            let result = agent_viewer::agent_viewer_extension(pi, &options);
            Box::pin(async move { result })
        }),
    );

    registry.register(
        "<first-party:agents-mailbox>",
        Arc::new(move |pi| {
            let options = shared.clone();
            let result = agents_mailbox::agents_mailbox_extension(pi, &options);
            Box::pin(async move { result })
        }),
    );
}
