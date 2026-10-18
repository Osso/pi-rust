pub mod edit_diff;
pub mod file_mutation_queue;
mod jsdiff;
pub mod output_accumulator;
pub mod path_utils;
pub mod truncate;

pub use truncate::{
    DEFAULT_MAX_BYTES, DEFAULT_MAX_LINES, TruncationOptions, TruncationResult, format_size, truncate_head,
    truncate_line, truncate_tail,
};
