mod truncate;

pub use truncate::{
    DEFAULT_MAX_BYTES, DEFAULT_MAX_LINES, GREP_MAX_LINE_LENGTH, TruncatedBy, TruncatedLine, TruncationOptions,
    TruncationResult, format_size, truncate_head, truncate_line, truncate_tail,
};
