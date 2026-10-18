#![deny(clippy::print_stdout)]

pub mod cli;
pub mod core;
pub mod modes;
pub mod supervisor;
pub mod utils;

pub use core::{
    ParsedSkillBlock, parse_skill_block,
    session_manager::CURRENT_SESSION_VERSION,
    session_tool_output::{MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES, TOOL_RESULT_TRUNCATION_MARKER},
    source_info::{SourceInfo, create_synthetic_source_info},
    tools::{DEFAULT_MAX_BYTES, DEFAULT_MAX_LINES, format_size},
};
