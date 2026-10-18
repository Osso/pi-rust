//! Bundled first-party extensions, ported from
//! `packages/coding-agent/extensions/*` as Rust-native modules.
//!
//! Each extension is registered under the same `<first-party:<name>>` id that
//! the TypeScript `main.ts` registers.

pub mod goal;

use crate::core::extensions::ExtensionRegistry;

/// Registers every bundled first-party extension.
pub fn register_bundled_extensions(registry: &mut ExtensionRegistry) {
    goal::register(registry);
}
