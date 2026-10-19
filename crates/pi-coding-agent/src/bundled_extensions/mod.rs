//! Bundled first-party extensions, ported from
//! `packages/coding-agent/extensions/*` as Rust-native modules.
//!
//! Each extension is registered under the same `<first-party:<name>>` id that
//! the TypeScript `main.ts` registers.

use std::sync::Arc;

use agents_core::runtime::{MultiAgentExtensionOptions, create_multi_agent_runtime_handles, resolve_multi_agent_store};

use crate::core::extensions::{ExtensionFactory, ExtensionRegistry};

pub mod agent_viewer;
pub mod agents_core;
pub mod agents_mailbox;
pub mod approval_controls;
pub mod ask_secret;
pub mod browser_cli;
pub mod bwrap;
pub mod claude_memory_enrich;
pub mod claude_memory_session_end;
pub mod debug;
pub mod docs_tree_context;
pub mod goal;
pub mod llama;
pub mod openai_remote_compact;
pub mod pyrun;
pub mod run_plan;
pub mod safe;
pub mod self_restart;
pub mod session_archive;
pub mod session_id;
pub mod spec_validation;

use debug::DebugReplControllerProvider;

/// Registers the inline built-in extensions (`extensions/index.ts`).
pub fn register_builtin_extensions(registry: &mut ExtensionRegistry) {
    llama::index::register_builtin_extensions(registry);
}

/// Options for the small first-party extensions.
pub struct SmallExtensionsOptions {
    pub get_debug_repl: DebugReplControllerProvider,
}

/// Registers the small first-party extensions under their source ids.
pub fn register_small_extensions(registry: &mut ExtensionRegistry, options: &SmallExtensionsOptions) {
    registry.register(
        "<first-party:claude-memory-enrich>",
        Arc::new(|pi| {
            let result = claude_memory_enrich::claude_memory_enrich_extension(pi);
            Box::pin(async move { result })
        }),
    );
    registry.register(
        "<first-party:claude-memory-session-end>",
        Arc::new(|pi| {
            let result = claude_memory_session_end::claude_memory_session_end_extension(pi);
            Box::pin(async move { result })
        }),
    );
    registry.register(
        "<first-party:docs-tree-context>",
        Arc::new(|pi| {
            let result = docs_tree_context::docs_tree_context_extension(pi);
            Box::pin(async move { result })
        }),
    );
    registry.register(
        "<first-party:ask-secret>",
        Arc::new(|pi| {
            let result = ask_secret::ask_secret_extension(pi);
            Box::pin(async move { result })
        }),
    );
    registry.register(
        "<first-party:browser-cli>",
        Arc::new(|pi| {
            let result = browser_cli::browser_cli_extension(pi, browser_cli::BrowserCliExtensionOptions::default());
            Box::pin(async move { result })
        }),
    );
    registry.register(
        "<first-party:approval-controls>",
        Arc::new(|pi| {
            let result = approval_controls::approval_controls_extension(pi);
            Box::pin(async move { result })
        }),
    );
    registry.register("<first-party:debug>", {
        let get_debug_repl = Arc::clone(&options.get_debug_repl);
        Arc::new(move |pi| {
            let result = debug::debug_extension(pi, Arc::clone(&get_debug_repl));
            Box::pin(async move { result })
        })
    });
    registry.register(
        "<first-party:run-plan>",
        Arc::new(|pi| {
            let result = run_plan::run_plan_extension(pi);
            Box::pin(async move { result })
        }),
    );
    registry.register(
        "<first-party:safe>",
        Arc::new(|pi| {
            let result = safe::safe_extension(pi);
            Box::pin(async move { result })
        }),
    );
    registry.register(
        "<first-party:self-restart>",
        Arc::new(|pi| {
            let result = self_restart::self_restart_extension(pi);
            Box::pin(async move { result })
        }),
    );
    registry.register(
        "<first-party:session-archive>",
        Arc::new(|pi| {
            let result = session_archive::session_archive_extension(pi);
            Box::pin(async move { result })
        }),
    );
    registry.register(
        "<first-party:session-id>",
        Arc::new(|pi| {
            let result = session_id::session_id_extension(pi);
            Box::pin(async move { result })
        }),
    );
    registry.register(
        "<first-party:spec-validation>",
        Arc::new(|pi| {
            let result = spec_validation::spec_validation_extension(pi);
            Box::pin(async move { result })
        }),
    );
}

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
