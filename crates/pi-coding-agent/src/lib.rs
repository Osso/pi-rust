#![deny(clippy::print_stdout)]

pub mod bun;
pub mod cli;
pub mod cli_entry;
pub mod config;
pub mod core;
pub mod internal_runner;
pub mod migrations;
pub mod modes;
pub mod package_manager_cli;
pub mod rpc_entry;
pub mod supervisor;
pub mod utils;

pub use core::{
    ParsedSkillBlock, parse_skill_block,
    session_manager::CURRENT_SESSION_VERSION,
    session_tool_output::{MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES, TOOL_RESULT_TRUNCATION_MARKER},
    source_info::{SourceInfo, create_synthetic_source_info},
    tools::edit_diff::{EditDiffResult, generate_diff_string, generate_unified_patch},
    tools::{
        DEFAULT_MAX_BYTES, DEFAULT_MAX_LINES, TruncationOptions, TruncationResult, format_size, truncate_head,
        truncate_line, truncate_tail,
    },
};
