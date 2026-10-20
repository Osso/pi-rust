//! Port of `packages/agent/src/harness/session/index.ts`.

pub mod context;
pub mod facade;
pub mod jsonl;
pub mod memory;
pub mod state;
pub mod testing;
pub mod types;

pub use context::{
    ContextEntryTransform, CustomEntryContextMessageProjector, SessionContext, SessionContextBuildOptions,
    SessionModel, build_context_entries, build_session_context, default_context_entry_transform,
    session_entry_to_context_messages,
};
pub use facade::{CompactionAppendOptions, Session, SessionView};
pub use jsonl::{
    JsonlSessionCreateOptions, JsonlSessionListOptions, JsonlSessionMetadata, JsonlSessionRepo,
    JsonlSessionRepoFileSystem, JsonlSessionRepoOptions, JsonlV4Header, list_jsonl_session_metadata,
    load_jsonl_session_storage,
};
pub use memory::{InMemorySessionRepo, InMemorySessionStorage};
pub use types::*;
