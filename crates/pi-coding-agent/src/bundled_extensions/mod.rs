//! Bundled first-party extensions, ported from
//! `packages/coding-agent/extensions/*` as Rust-native modules.
//!
//! Each extension is registered under the same `<first-party:<name>>` id that
//! the TypeScript `main.ts` registers.

use std::sync::Arc;

use crate::core::extensions::{ExtensionFactory, ExtensionRegistry};

pub mod agent_viewer;
pub mod agents_core;
pub mod agents_mailbox;
pub mod bwrap;
pub mod goal;
pub mod pyrun;

use agents_core::runtime::{MultiAgentExtensionOptions, create_multi_agent_runtime_handles, resolve_multi_agent_store};

/// First-party agents extension factories in source registration order.
pub fn agents_extension_factories(options: &MultiAgentExtensionOptions) -> Vec<(&'static str, ExtensionFactory)> {
    let store = resolve_multi_agent_store(options);
    let mut shared = options.clone();
    shared.store = Some(Arc::clone(&store));
    if shared.runtime_handles.is_none() {
        shared.runtime_handles = Some(create_multi_agent_runtime_handles());
    }

    let core_options = shared.clone();
    let core: ExtensionFactory = Arc::new(move |pi| {
        let options = core_options.clone();
        let result = agents_core::registration::register_agents_core_tools(pi, &options);
        Box::pin(async move { result })
    });

    let viewer_options = shared.clone();
    let viewer: ExtensionFactory = Arc::new(move |pi| {
        let options = viewer_options.clone();
        let result = agent_viewer::agent_viewer_extension(pi, &options);
        Box::pin(async move { result })
    });

    let mailbox: ExtensionFactory = Arc::new(move |pi| {
        let options = shared.clone();
        let result = agents_mailbox::agents_mailbox_extension(pi, &options);
        Box::pin(async move { result })
    });

    vec![
        ("agents-core", core),
        ("agent-viewer", viewer),
        ("agents-mailbox", mailbox),
    ]
}

/// Registers the first-party agents extensions under their source ids.
///
/// `main.ts` shares one store and runtime handle set across agents-core,
/// agent-viewer, agents-mailbox, and the Pyrun request handler; callers that
/// construct their own options can pass the same handles to preserve that.
pub fn register_agents_extensions(registry: &mut ExtensionRegistry, options: &MultiAgentExtensionOptions) {
    for (name, factory) in agents_extension_factories(options) {
        registry.register(format!("<first-party:{name}>"), factory);
    }
}

/// Registers every bundled first-party extension.
pub fn register_bundled_extensions(registry: &mut ExtensionRegistry) {
    bwrap::register(registry);
    goal::register(registry);
    pyrun::register(registry);
}
