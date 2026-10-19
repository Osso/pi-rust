//! Rust port of `packages/agent`.
//!
//! The crate root mirrors `packages/agent/src/index.ts`. See
//! `docs/porting/traceability/agent-index.md` for the export map.

pub mod agent;
pub mod harness;
pub mod jsdiff;
pub mod node;
pub mod search;
pub mod unicode;

pub use pi_ai::utils::uuid::uuidv7;
pub use pi_telemetry::SpanAttributes as TelemetrySpanAttributes;
pub use pi_telemetry::{
    AttributeValue, InMemoryTelemetryContext, NOOP_TELEMETRY_CONTEXT, RecordedTelemetryEvent, RecordedTelemetrySpan,
    SpanAttributes, SpanOptions, SpanStatus, TelemetryAttributeDefinition, TelemetryAttributeMetadata,
    TelemetryAttributeType, TelemetryContext, TelemetryEventAttributeDefinition, TelemetryEventDefinition,
    TelemetryParentDefinition, TelemetrySchemaDefinition, TelemetrySpan, TelemetrySpanDefinition,
    TelemetryStartAttributeDefinition, TypedSpanStarter, create_typed_span_starter, define_telemetry_schema,
};

pub use agent::types::*;
pub use agent::{
    Agent, AgentEventSink, AgentEventStream, AgentInitialState, AgentListener, AgentOptions, ContinueOptions,
    PrepareNextTurnSignalHook, PromptInput, ProxyAssistantMessageEvent, ProxyStreamOptions, ScriptedAssistantStream,
    agent_loop, agent_loop_continue, run_agent_loop, run_agent_loop_continue, set_default_stream_fn, stream_proxy,
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
pub use harness::telemetry::{
    AiSpan, AiSpanAttributes, AiSpanEndAttributes, AiSpanName, AiSpanStartAttributes, AiTelemetrySpan, HarnessSpan,
    HarnessSpanAttributes, HarnessSpanEndAttributes, HarnessSpanName, HarnessSpanStartAttributes, HarnessTelemetrySpan,
    agent_telemetry_schemas, ai_telemetry_schema, harness_telemetry_schema, start_ai_span, start_harness_span,
};
pub use harness::tools::{
    BashExecution, BashPrepare, BashTool, BashToolOptions, EditTool, ExecutionToolContext, ImageProcessorOptions,
    ReadImageProcessor, ReadImageProcessorResult, ReadTool, ReadToolOptions, WriteTool,
};
pub use harness::types::{
    AgentHarnessResources, AgentHarnessStreamOptions, AgentHarnessStreamOptionsPatch, AgentHarnessTool,
    AgentHarnessToolContextSource, AgentToolExecutionContext, BeforeToolCallResult, BranchSummaryError,
    BranchSummaryErrorCode, CompactionError, CompactionErrorCode, ExecutionEnv, ExecutionError, ExecutionErrorCode,
    FileError, FileErrorCode, FileInfo, FileKind, FileSystem, PromptTemplate, QueueMode, Shell, ShellExecOptions,
    Skill, ThinkingLevel, ToolExecutionMode,
};
pub use harness::utils::*;
pub use search::*;
