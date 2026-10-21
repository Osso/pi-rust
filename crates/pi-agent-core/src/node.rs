//! Port of `packages/agent-core/src/node.ts`.
//!
//! The Node entrypoint re-exports the crate root plus [`NodeExecutionEnv`].

pub use crate::harness::env::NodeExecutionEnv;
pub use crate::*;
