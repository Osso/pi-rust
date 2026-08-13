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
