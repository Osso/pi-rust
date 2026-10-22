//! Rust port of `packages/agent-core`.
//!
//! The crate root mirrors `packages/agent-core/src/index.ts`. See
//! `docs/porting/traceability/agent-index.md` for the export map.

pub mod agent;
pub mod harness;
pub mod node;

pub use pi_ai::utils::uuid::uuidv7;

pub use agent::types::*;
pub use agent::{
    Agent, AgentEventSink, AgentEventStream, AgentInitialState, AgentListener, AgentOptions, ContinueOptions,
    PrepareNextTurnSignalHook, PromptInput, ProxyAssistantMessageEvent, ProxyStreamOptions, agent_loop,
    agent_loop_continue, restored_pending_tool_message, run_agent_loop, run_agent_loop_continue, stream_proxy,
};
pub use harness::agent_harness::*;
pub use harness::compaction::{
    BranchPreparation, BranchSummaryDetails, BranchSummaryResult, CollectEntriesResult, CompactResult,
    CompactionPreparation, CompactionSettings, DEFAULT_COMPACTION_SETTINGS, FileOperations,
    GenerateBranchSummaryOptions, calculate_context_tokens, collect_entries_for_branch_summary, compact,
    estimate_context_tokens, estimate_tokens, find_cut_point, find_turn_start_index, generate_branch_summary,
    generate_summary, generate_summary_with_usage, get_last_assistant_usage, prepare_branch_entries,
    prepare_compaction, serialize_conversation, should_compact,
};
pub use harness::messages::*;
pub use harness::prompt_templates::*;
pub use harness::session::*;
pub use harness::skills::*;
pub use harness::system_prompt::*;
pub use harness::types::{
    AgentHarnessResources, AgentHarnessStreamOptions, AgentHarnessStreamOptionsPatch, AgentHarnessTool,
    AgentHarnessToolContextSource, AgentToolExecutionContext, BeforeToolCallResult, BranchSummaryError,
    BranchSummaryErrorCode, CompactionError, CompactionErrorCode, ExecutionEnv, ExecutionError, ExecutionErrorCode,
    FileError, FileErrorCode, FileInfo, FileKind, FileSystem, PromptTemplate, QueueMode, Shell, ShellExecOptions,
    Skill, ThinkingLevel, ToolExecutionMode,
};
pub use harness::utils::*;
