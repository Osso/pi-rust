pub mod edit;
pub mod edit_diff;
pub mod file_mutation_queue;
pub mod node_fs;
pub mod output_accumulator;
pub mod path_utils;
pub mod read;
pub mod render_utils;
pub mod tool_definition_wrapper;
pub mod truncate;
pub mod write;

pub use truncate::{
    DEFAULT_MAX_BYTES, DEFAULT_MAX_LINES, TruncationOptions, TruncationResult, format_size, truncate_head,
    truncate_line, truncate_tail,
};

/// Tools enabled for a default session (`DEFAULT_ACTIVE_TOOL_NAMES` in `tools/index.ts`).
pub const DEFAULT_ACTIVE_TOOL_NAMES: [&str; 19] = [
    "read",
    "bash",
    "edit",
    "write",
    "grep",
    "find",
    "ls",
    "outline",
    "symbol",
    "references",
    "end_turn",
    "change_working_directory",
    "resume_session",
    "search_current_session_history",
    "list_sessions",
    "broadcast",
    "channel_post",
    "ask_questions",
    "ask_supervisor",
];
