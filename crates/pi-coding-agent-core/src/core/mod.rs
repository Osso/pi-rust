pub mod agent_session;
pub mod agent_session_runtime;
pub mod agent_session_services;
pub mod provider_restriction_recovery;
pub mod sdk;

pub use pi_coding_agent_tools::core::*;

// `core/index.ts` barrel: the public core surface re-exported from one place.
pub use agent_session::{
    AgentSession, AgentSessionConfig, AgentSessionEvent, AgentSessionEventListener, ModelCycleResult, PromptOptions,
    SessionStats,
};
pub use agent_session_runtime::{
    AgentSessionRuntime, CreateAgentSessionRuntimeFactory, CreateAgentSessionRuntimeResult,
    create_agent_session_runtime,
};
pub use agent_session_services::{
    AgentSessionRuntimeDiagnostic, AgentSessionServices, CreateAgentSessionFromServicesOptions,
    CreateAgentSessionServicesOptions, create_agent_session_from_services, create_agent_session_services,
};
pub use bash_executor::{BashExecutorOptions, BashResult, execute_bash_with_operations};
pub use compaction::CompactionResult;
pub use event_bus::{EventBus, EventBusController, create_event_bus};
pub use experimental::are_experimental_features_enabled;
pub use extensions::{
    AgentEndEvent, AgentStartEvent, AgentToolResult, AgentToolUpdateCallback, BeforeAgentStartEvent,
    BeforeAgentStartEventResult, ContextEvent, ExecOptions, ExecResult, Extension, ExtensionApi,
    ExtensionCommandContext, ExtensionContext, ExtensionError, ExtensionEvent, ExtensionFactory, ExtensionFlag,
    ExtensionRunner, ExtensionShortcut, ExtensionUiComponent, ExtensionUiComponentFactory, ExtensionUiContext,
    ExtensionUiCustomFactory, ExtensionUiCustomOptions, ExtensionUiDone, ExtensionUiFactoryContext,
    ExtensionUiFooterContext, ExtensionUiFooterFactory, ExtensionUiOverlayHandle, ExtensionUiOverlayOptions,
    ExtensionUiTui, ExtensionUiTuiBackend, Handler as ExtensionHandler, LoadExtensionsResult, MessageRenderer,
    ModelRequestEndEvent, ModelRequestStartEvent, RegisteredCommand, SessionBeforeCompactEvent, SessionBeforeForkEvent,
    SessionBeforeSwitchEvent, SessionBeforeTreeEvent, SessionCompactEvent, SessionShutdownEvent, SessionStartEvent,
    SessionTreeEvent, ToolCallEvent, ToolCallEventResult, ToolDefinition, ToolRenderResultOptions, ToolResultEvent,
    TurnEndEvent, TurnStartEvent, WorkingIndicatorOptions, define_tool, discover_and_load_extensions,
};
pub use multi_agent_store::{
    AgentActivity, AgentCurrentActivity, AgentCurrentActivityOwner, AgentFileReference, AgentLifecycleState,
    AgentMailboxMessage, AgentNode, AgentResult, AgentSnapshot, MailboxMessageKind, MailboxMessageStatus,
    MultiAgentStore, MultiAgentStoreOptions, SendSteeringInput, SpawnAgentInput, SteeringCheckpoint,
    TerminalAgentLifecycleState, is_active_lifecycle,
};
pub use resource_loader::{
    DefaultResourceLoader, DefaultResourceLoaderOptions, InlineExtension, ResourceLoader, load_project_context_files,
    load_rules_files_from_dir, load_rules_from_dir,
};
pub use source_info::{create_source_info, create_synthetic_source_info};
pub use system_prompt::BuildSystemPromptOptions;
