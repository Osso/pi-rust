pub mod truncate;

pub use truncate::{DEFAULT_MAX_BYTES, DEFAULT_MAX_LINES, format_size};

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
