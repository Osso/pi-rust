/// Formats a non-negative byte count with the shared truncation utility.
pub fn format_size(bytes: usize) -> String {
    pi_agent_core::harness::utils::format_size(bytes)
}
