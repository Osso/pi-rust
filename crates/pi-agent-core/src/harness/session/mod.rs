//! Frozen agent-core v3 session tree; old v4 lane records are retired.

pub mod context;
pub mod facade;
pub mod jsonl;
pub mod memory;
pub mod repo_utils;
mod state;
pub mod types;
pub mod uuid;

pub use context::{SessionContext, SessionModel, build_session_context};
pub use facade::{CompactionAppendOptions, MoveSummary, Session};
pub use jsonl::*;
pub use memory::{InMemorySessionRepo, InMemorySessionStorage, InMemorySessionStorageOptions};
pub use repo_utils::{
    create_session_id, create_timestamp, get_entries_to_fork, get_file_system_result_or_throw, to_session,
};
pub use types::*;
