/// Default maximum number of output lines.
pub const DEFAULT_MAX_LINES: usize = pi_agent_core::harness::utils::DEFAULT_MAX_LINES;
/// Default maximum number of output bytes.
pub const DEFAULT_MAX_BYTES: usize = pi_agent_core::harness::utils::DEFAULT_MAX_BYTES;
/// Default maximum length of one grep match line.
pub const GREP_MAX_LINE_LENGTH: usize = pi_agent_core::harness::utils::GREP_MAX_LINE_LENGTH;

/// Formats a non-negative byte count with the shared truncation utility.
pub fn format_size(bytes: usize) -> String {
    pi_agent_core::harness::utils::format_size(bytes)
}
