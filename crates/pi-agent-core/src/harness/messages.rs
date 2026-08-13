/// Role tag for shell-command execution messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BashExecutionRole {
    BashExecution,
}

impl BashExecutionRole {
    /// Returns the exact source role literal.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BashExecution => "bashExecution",
        }
    }
}

/// Value-only record for shell-command execution results.
#[derive(Debug)]
pub struct BashExecutionMessage {
    pub role: BashExecutionRole,
    pub command: String,
    pub output: String,
    pub exit_code: Option<f64>,
    pub cancelled: bool,
    pub truncated: bool,
    pub full_output_path: Option<String>,
    pub timestamp: f64,
    pub exclude_from_context: Option<bool>,
}

/// Prefix framing a compacted conversation summary.
pub const COMPACTION_SUMMARY_PREFIX: &str =
    "The conversation history before this point was compacted into the following summary:\n\n<summary>\n";

/// Suffix framing a compacted conversation summary.
pub const COMPACTION_SUMMARY_SUFFIX: &str = "\n</summary>";

/// Prefix framing a summary returned from a conversation branch.
pub const BRANCH_SUMMARY_PREFIX: &str =
    "The following is a summary of a branch that this conversation came back from:\n\n<summary>\n";

/// Suffix framing a summary returned from a conversation branch.
pub const BRANCH_SUMMARY_SUFFIX: &str = "</summary>";
