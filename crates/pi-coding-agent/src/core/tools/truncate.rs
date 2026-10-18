//! Coding-agent tool-output truncation.
//!
//! The pinned `coding-agent/src/core/tools/truncate.ts` differs from the agent
//! harness copy only in how it counts lone UTF-16 surrogates, which Rust strings
//! cannot contain. The shared agent-core implementation is therefore reused.

pub use pi_agent_core::harness::utils::{
    DEFAULT_MAX_BYTES, DEFAULT_MAX_LINES, GREP_MAX_LINE_LENGTH, TruncatedBy, TruncatedLine, TruncationOptions,
    TruncationResult, format_size, truncate_head, truncate_line, truncate_tail,
};
