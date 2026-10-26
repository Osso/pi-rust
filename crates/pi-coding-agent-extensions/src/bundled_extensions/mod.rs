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
pub mod agents_status;
pub mod approval_controls;
pub mod ask_secret;
pub mod browser_cli;
pub mod bwrap;
pub mod claude_bash_hook;
pub mod claude_memory_enrich;
pub mod claude_memory_session_end;
pub mod codex_daybreak;
pub mod codex_fast;
pub mod codex_image_generation;
pub mod codex_usage;
pub mod codex_web_search;
pub mod debug;
pub mod default_footer;
pub mod docs_tree_context;
pub mod effort;
pub mod goal;
mod hosted_deadline;
#[path = "loop/mod.rs"]
pub mod r#loop;
pub mod openai_remote_compact;
pub mod pyrun;
pub mod run_plan;
pub mod safe;
pub mod self_restart;
pub mod session_archive;
pub mod session_autoname;
pub mod session_id;
pub mod spec_validation;

use debug::DebugReplControllerProvider;

/// Options for the small first-party extensions.
pub struct SmallExtensionsOptions {
    pub get_debug_repl: DebugReplControllerProvider,
    pub multi_agent_store: Option<Arc<crate::core::multi_agent_store::MultiAgentStore>>,
    pub fast_mode_authority: codex_fast::FastModeAuthority,
    pub fetch: Option<Arc<dyn pi_ai::request::Fetch>>,
}

/// Registers the small first-party extensions under their source ids, in the
/// order `main.ts` lists them in `createFirstPartyExtensionFactories`.
pub fn register_small_extensions(registry: &mut ExtensionRegistry, options: &SmallExtensionsOptions) {
    crate::native_user_extensions::register(registry);
    registry.register(
        "<first-party:approval-controls>",
        Arc::new(|pi| {
            let result = approval_controls::approval_controls_extension(pi);
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
        "<first-party:claude-bash-hook>",
        Arc::new(|pi| {
            let result =
                claude_bash_hook::claude_bash_hook_extension(pi, &claude_bash_hook::ClaudeBashHookOptions::default());
            Box::pin(async move { result })
        }),
    );
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
        "<first-party:codex-daybreak>",
        Arc::new(|pi| {
            let result = codex_daybreak::codex_daybreak_extension(pi);
            Box::pin(async move { result })
        }),
    );
    registry.register("<first-party:codex-fast>", {
        let authority = options.fast_mode_authority.clone();
        Arc::new(move |pi| {
            let options = codex_fast::CodexFastOptions {
                authority: authority.clone(),
            };
            let result = codex_fast::codex_fast_extension(pi, &options);
            Box::pin(async move { result })
        })
    });
    registry.register(
        "<first-party:codex-image-generation>",
        Arc::new(|pi| {
            let result = codex_image_generation::codex_image_generation_extension(
                pi,
                &codex_image_generation::ImageGenerationToolOptions::default(),
            );
            Box::pin(async move { result })
        }),
    );
    registry.register("<first-party:codex-usage>", {
        let fetch = options.fetch.clone();
        Arc::new(move |pi| {
            let options = codex_usage::CodexUsageOptions { fetch: fetch.clone() };
            let result = codex_usage::codex_usage_extension(pi, &options);
            Box::pin(async move { result })
        })
    });
    registry.register(
        "<first-party:codex-web-search>",
        Arc::new(|pi| {
            let result =
                codex_web_search::codex_web_search_extension(pi, &codex_web_search::WebSearchToolOptions::default());
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
        "<first-party:docs-tree-context>",
        Arc::new(|pi| {
            let result = docs_tree_context::docs_tree_context_extension(pi);
            Box::pin(async move { result })
        }),
    );
    registry.register("<first-party:default-footer>", {
        let store = options.multi_agent_store.clone();
        Arc::new(move |pi| {
            let options = default_footer::DefaultFooterExtensionOptions {
                multi_agent_store: store.clone(),
            };
            let result = default_footer::default_footer_extension(pi, &options);
            Box::pin(async move { result })
        })
    });
    registry.register(
        "<first-party:effort>",
        Arc::new(|pi| {
            let result = effort::effort_extension(pi);
            Box::pin(async move { result })
        }),
    );
    registry.register(
        "<first-party:loop>",
        Arc::new(|pi| {
            let result = r#loop::loop_extension(pi);
            Box::pin(async move { result })
        }),
    );
    registry.register(
        "<first-party:openai-remote-compact>",
        Arc::new(|pi| {
            let result = openai_remote_compact::openai_remote_compact_extension(pi);
            Box::pin(async move { result })
        }),
    );
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
        "<first-party:spec-validation>",
        Arc::new(|pi| {
            let result = spec_validation::spec_validation_extension(pi);
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
        "<first-party:session-autoname>",
        Arc::new(|pi| {
            let result = session_autoname::session_autoname_extension(pi);
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
        "<first-party:session-id>",
        Arc::new(|pi| {
            let result = session_id::session_id_extension(pi);
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

    let status_store = Arc::clone(&store);
    let status: ExtensionFactory = Arc::new(move |pi| {
        let result = agents_status::agents_status_extension(
            pi,
            &agents_status::AgentsStatusExtensionOptions {
                store: Some(Arc::clone(&status_store)),
            },
        );
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
        ("agents-status", status),
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
