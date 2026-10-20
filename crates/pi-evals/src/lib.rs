//! Rust port of `packages/evals`.
//!
//! The crate provides the Pi coding-agent eval harness, the comparative
//! harness table, the `runs.jsonl`/comparison reporter, and the
//! extension-authoring and smoke eval definitions. The eval cases themselves
//! are Rust integration tests; Vitest is not part of the Rust runtime.

pub mod error;
pub mod evals;
pub mod harness;
pub mod pi_harness;
pub mod run_evals;
pub mod style;
pub mod vitest_evals;

pub use error::{PiEvalError, PiEvalErrorKind};
pub use harness::{
    Harness, HarnessContext, HarnessRun, NormalizedError, NormalizedSession, TimingSummary, ToolCall, ToolCallStatus,
    TranscriptEvent, TranscriptRole, UsageSummary, tool_calls,
};
pub use pi_harness::{
    ModelSelection, PiCodingAgentHarness, PiCodingAgentHarnessOptions, PiCodingAgentInput, PiCodingAgentStep,
    PiHarnessOutputArgs, PiHarnessOutputFn, PiModelEnvironment, PiSystemPromptTransform,
    create_pi_coding_agent_harness, resolve_model_selection,
};
pub use style::{StyleText, TextStyle};
