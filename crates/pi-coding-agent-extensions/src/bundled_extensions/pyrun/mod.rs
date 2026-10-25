//! Port of `packages/coding-agent/extensions/pyrun/src/*.ts`.

pub mod detached_bridge;
pub mod detached_evaluation;
pub mod detached_output;
pub mod detached_runner;
pub mod eval_tool;
pub mod index;
pub mod runner;

pub use index::{PyrunExtensionOptions, install_pyrun_extension, pyrun_extension_factory};

use crate::core::extensions::ExtensionRegistry;

/// Registers `<first-party:pyrun>` with default options.
pub fn register(registry: &mut ExtensionRegistry) {
    registry.register(
        "<first-party:pyrun>",
        pyrun_extension_factory(PyrunExtensionOptions::default()),
    );
}
