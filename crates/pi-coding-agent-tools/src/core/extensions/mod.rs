//! Extension system for lifecycle events and custom tools.
//!
//! Port of `core/extensions/index.ts`. Extensions are Rust-native; the
//! TypeScript module loader is an approved deviation (see traceability).

#[cfg(test)]
mod native_identity_tests;

pub mod extension_runtime;
pub mod loader;
pub mod runner;
pub mod types;
pub mod wrapper;

pub use extension_runtime::{ExtensionRuntime, create_extension_runtime};
pub use loader::{
    ExtensionFactory, ExtensionRegistry, clear_extension_cache, discover_and_load_extensions, discover_extension_paths,
    inline_extension_name, is_extension_factory_disabled, is_extension_path_disabled, load_extension_from_factory,
    load_extensions, load_extensions_cached,
};
pub use runner::{
    BeforeAgentStartCombinedResult, DiscoveredPath, EmitResult, ExtensionErrorListener, ExtensionRunner,
    ResourcesDiscoverPaths, emit_project_trust_event, emit_session_shutdown_event,
};
pub use types::*;
pub use wrapper::{wrap_registered_tool, wrap_registered_tools};
