/// Maximum UTF-8 byte length persisted for a tool result's content.
pub const MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES: usize = 1_048_576;

/// Marker appended when persisted tool-result content is truncated.
pub const TOOL_RESULT_TRUNCATION_MARKER: &str = "\n[tool output truncated at 1 MiB]";
