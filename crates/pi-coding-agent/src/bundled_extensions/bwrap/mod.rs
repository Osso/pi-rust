//! Port of `packages/coding-agent/extensions/bwrap/src/*.ts` plus the
//! `fs-worker.cjs` internal mode.

pub mod backend;
pub mod index;
pub mod worker;

pub use index::{BwrapExtensionOptions, bwrap_extension_factory, install_bwrap_extension};

use crate::core::extensions::ExtensionRegistry;

/// Registers `<first-party:bwrap>` with default options.
pub fn register(registry: &mut ExtensionRegistry) {
    registry.register(
        "<first-party:bwrap>",
        bwrap_extension_factory(BwrapExtensionOptions::default()),
    );
}
