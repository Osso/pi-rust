pub mod agent_session;
pub mod agent_session_runtime;
pub mod agent_session_services;
pub mod auth_guidance;
pub mod auth_storage;
pub mod bash_executor;
pub mod cache_stats;
pub mod compaction;
pub mod compaction_threshold;
mod config_value;
pub mod defaults;
pub mod desktop_notification;
pub mod detached_bash_runner;
pub mod detached_job_bootstrap;
pub mod detached_job_cleanup;
pub mod detached_job_control;
pub mod detached_job_retention;
pub mod detached_job_runner;
pub mod diagnostics;
pub mod event_bus;
pub mod exec;
pub mod experimental;
pub mod export_html;
pub mod extensions;
pub mod external_session_importer;
pub mod footer_data_provider;
pub mod http_dispatcher;
mod js_string;
pub mod keybindings;
pub mod lifecycle_coordinator;
pub mod messages;
pub mod model_config;
pub mod model_registry;
pub mod model_resolver;
pub mod model_runtime;
pub mod models_store;
pub mod multi_agent_store;
pub mod output_guard;
pub mod package_manager;
pub mod permissions;
pub mod pi_manifest;
pub mod prompt_templates;
pub mod provider_attribution;
pub mod provider_composer;
pub mod provider_display_names;
pub mod radius;
pub mod remote_catalog_provider;
pub mod resident_console_transport;
pub mod resource_loader;
pub mod retry_delay;
pub mod rule_store;
pub mod runtime_coordination_format;
pub mod runtime_credentials;
pub mod runtime_mailbox_transcript;
pub mod runtime_process;
pub mod sdk;
pub mod self_restart;
pub mod session_control_db;
pub mod session_cwd;
pub mod session_directory;
pub mod session_errors;
pub mod session_health;
pub mod session_manager;
pub mod session_tool_output;
pub mod settings_manager;
mod skill_block;
pub mod skills;
pub mod slash_commands;
pub mod source_info;
pub mod sqlite;
pub mod system_prompt;
pub mod telemetry;
pub mod terminal_outbox_delivery;
pub mod timings;
pub mod tool_capabilities;
pub mod tool_detach_registry;
pub mod tools;
pub mod trust_manager;
pub mod usage_totals;

pub use config_value::{
    ConfigValueError, clear_config_value_cache, get_config_value_env_var_name, get_config_value_env_var_names,
    get_missing_config_value_env_var_names, is_command_config_value, is_config_value_configured, resolve_config_value,
    resolve_config_value_or_throw, resolve_config_value_uncached, resolve_headers, resolve_headers_or_throw,
};
pub use skill_block::{ParsedSkillBlock, parse_skill_block};
pub mod project_trust;
pub mod runtime_message_markers;

// `core/index.ts` barrel: the public core surface re-exported from one place.
pub use agent_session::{
    AgentSession, AgentSessionConfig, AgentSessionEvent, AgentSessionEventListener, ModelCycleResult, PromptOptions,
    SessionStats,
};
pub use agent_session_runtime::{
    AgentSessionRuntime, CreateAgentSessionRuntimeFactory, CreateAgentSessionRuntimeResult,
    create_agent_session_runtime, share_agent_session_runtime,
};
pub use agent_session_services::{
    AgentSessionRuntimeDiagnostic, AgentSessionServices, CreateAgentSessionFromServicesOptions,
    CreateAgentSessionServicesOptions, create_agent_session_from_services, create_agent_session_services,
};
pub use compaction::CompactionResult;
pub use event_bus::{EventBus, EventBusController, create_event_bus};
pub use experimental::are_experimental_features_enabled;
pub use extensions::{
    AgentEndEvent, AgentSettledEvent, AgentStartEvent, BeforeAgentStartEvent, BeforeAgentStartEventResult,
    ContextEvent, Extension, ExtensionApi, ExtensionCommandContext, ExtensionContext, ExtensionError, ExtensionEvent,
    ExtensionFactory, ExtensionFlag, ExtensionRunner, ExtensionShortcut, ExtensionUiComponent,
    ExtensionUiComponentFactory, ExtensionUiContext, ExtensionUiCustomFactory, ExtensionUiCustomOptions,
    ExtensionUiDone, ExtensionUiFactoryContext, ExtensionUiFooterContext, ExtensionUiFooterFactory,
    ExtensionUiOverlayHandle, ExtensionUiOverlayOptions, ExtensionUiTui, ExtensionUiTuiBackend, LoadExtensionsResult,
    MessageRenderer, RegisteredCommand, SessionBeforeCompactEvent, SessionBeforeForkEvent, SessionBeforeSwitchEvent,
    SessionBeforeTreeEvent, SessionCompactEvent, SessionShutdownEvent, SessionStartEvent, SessionTreeEvent,
    ToolCallEvent, ToolCallEventResult, ToolDefinition, ToolRenderResultOptions, ToolResultEvent, TurnEndEvent,
    TurnStartEvent, WorkingIndicatorOptions, define_tool, discover_and_load_extensions,
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

pub mod session_archive_storage;
pub mod session_history;
pub mod session_transcript;
pub mod session_transcript_spool;
