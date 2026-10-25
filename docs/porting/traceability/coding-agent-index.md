# Coding-agent public root index

Frozen source: `packages/coding-agent/src/index.ts` at `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`.
Native barrel: `crates/pi-coding-agent/src/lib.rs`.

The frozen file contains only named re-exports: 388 source names. All runtime functions, classes and constants forward directly to the existing native implementations. Nine existing native auth/event/footer/model types were missing from the root; external import RED demonstrated the missing public contract before forwarding them. No wrapper carrier or alternate runtime path is added.

TypeScript-only narrowing/schema/base interfaces map to the existing native representations below. Native enums carry literal discriminators; JSON-schema argument/detail aliases stay lossless JsValue. Source-absent old table entries (including readStoredCredential and generateSummaryWithUsage) are not counted as frozen exports. Runtime semantics and approved gaps remain owned by each implementation module's trace.

| Frozen export | Native public item/representation | Implementation authority | Notes |
|---|---|---|---|
| `Args` | `Args` | `crate::cli::args::Args` |  |
| `parseArgs` | `parse_args` | `crate::cli::args::parse_args` |  |
| `CONFIG_DIR_NAME` | `config_dir_name` | `crate::config::config_dir_name` |  |
| `getAgentDir` | `get_agent_dir` | `crate::config::get_agent_dir` |  |
| `getDocsPath` | `get_docs_path` | `crate::config::get_docs_path` |  |
| `getExamplesPath` | `get_examples_path` | `crate::config::get_examples_path` |  |
| `getPackageDir` | `get_package_dir` | `crate::config::get_package_dir` |  |
| `getReadmePath` | `get_readme_path` | `crate::config::get_readme_path` |  |
| `VERSION` | `version` | `crate::config::version` |  |
| `AgentSession` | `AgentSession` | `crate::core::agent_session::AgentSession` |  |
| `AgentSessionConfig` | `AgentSessionConfig` | `crate::core::agent_session::AgentSessionConfig` |  |
| `AgentSessionEvent` | `AgentSessionEvent` | `crate::core::agent_session::AgentSessionEvent` |  |
| `AgentSessionEventListener` | `AgentSessionEventListener` | `crate::core::agent_session::AgentSessionEventListener` |  |
| `ModelCycleResult` | `ModelCycleResult` | `crate::core::agent_session::ModelCycleResult` |  |
| `PromptOptions` | `PromptOptions` | `crate::core::agent_session::PromptOptions` |  |
| `SessionStats` | `SessionStats` | `crate::core::agent_session::SessionStats` |  |
| `ApiKeyCredential` | `ApiKeyCredential` | `pi_ai::auth::types::ApiKeyCredential` | Payload struct; AuthCredential::ApiKey carries the literal type discriminator. |
| `AuthCredential` | `AuthCredential` | `pi_ai::auth::types::Credential` | Alias of the existing Credential enum, not a new carrier. |
| `AuthStatus` | `AuthStatus` | `crate::core::auth_storage::AuthStatus` |  |
| `AuthStorage` | `AuthStorage` | `crate::core::auth_storage::AuthStorage` |  |
| `AuthStorageBackend` | `AuthStorageBackend` | `crate::core::auth_storage::AuthStorageBackend` |  |
| `FileAuthStorageBackend` | `FileAuthStorageBackend` | `crate::core::auth_storage::FileAuthStorageBackend` |  |
| `InMemoryAuthStorageBackend` | `InMemoryAuthStorageBackend` | `crate::core::auth_storage::InMemoryAuthStorageBackend` |  |
| `OAuthCredential` | `OAuthCredential` | `pi_ai::auth::types::OAuthCredential` |  |
| `BranchPreparation` | `BranchPreparation` | `crate::core::compaction::branch_summarization::BranchPreparation` |  |
| `BranchSummaryResult` | `BranchSummaryResult` | `crate::core::compaction::branch_summarization::BranchSummaryResult` |  |
| `CollectEntriesResult` | `CollectEntriesResult` | `crate::core::compaction::branch_summarization::CollectEntriesResult` |  |
| `CompactionResult` | `CompactionResult` | `crate::core::compaction::CompactionResult` |  |
| `CutPointResult` | `CutPointResult` | `crate::core::compaction::CutPointResult` |  |
| `calculateContextTokens` | `calculate_context_tokens` | `crate::core::compaction::calculate_context_tokens` |  |
| `collectEntriesForBranchSummary` | `collect_entries_for_branch_summary` | `crate::core::compaction::branch_summarization::collect_entries_for_branch_summary` |  |
| `compact` | `compact` | `crate::core::compaction::compact` |  |
| `DEFAULT_COMPACTION_SETTINGS` | `DEFAULT_COMPACTION_SETTINGS` | `pi_agent_core::harness::compaction::DEFAULT_COMPACTION_SETTINGS` |  |
| `estimateTokens` | `estimate_tokens` | `crate::core::compaction::estimate_tokens` |  |
| `FileOperations` | `FileOperations` | `crate::core::compaction::FileOperations` |  |
| `findCutPoint` | `find_cut_point` | `crate::core::compaction::find_cut_point` |  |
| `findTurnStartIndex` | `find_turn_start_index` | `crate::core::compaction::find_turn_start_index` |  |
| `GenerateBranchSummaryOptions` | `GenerateBranchSummaryOptions` | `crate::core::compaction::branch_summarization::GenerateBranchSummaryOptions` |  |
| `generateBranchSummary` | `generate_branch_summary` | `crate::core::compaction::branch_summarization::generate_branch_summary` |  |
| `generateSummary` | `generate_summary` | `crate::core::compaction::generate_summary` |  |
| `getLastAssistantUsage` | `get_last_assistant_usage` | `crate::core::compaction::get_last_assistant_usage` |  |
| `prepareBranchEntries` | `prepare_branch_entries` | `crate::core::compaction::branch_summarization::prepare_branch_entries` |  |
| `serializeConversation` | `serialize_conversation` | `crate::core::compaction::utils::serialize_conversation` |  |
| `shouldCompact` | `should_compact` | `crate::core::compaction::should_compact` |  |
| `createEventBus` | `create_event_bus` | `crate::core::event_bus::create_event_bus` |  |
| `EventBus` | `EventBus` | `crate::core::event_bus::EventBus` |  |
| `EventBusController` | `EventBusController` | `crate::core::event_bus::EventBusController` |  |
| `AgentEndEvent` | `AgentEndEvent` | `crate::core::agent_session::AgentEndEvent` |  |
| `AgentStartEvent` | `AgentStartEvent` | `crate::core::extensions::types::AgentStartEvent` |  |
| `AgentToolResult` | `AgentToolResult` | `pi_agent_core::agent::types::AgentToolResult` |  |
| `AgentToolUpdateCallback` | `AgentToolUpdateCallback` | `pi_agent_core::agent::types::AgentToolUpdateCallback` |  |
| `AppKeybinding` | `Keybinding` | `pi_tui::keybindings::Keybinding` | Existing native representation; no wrapper type. |
| `AutocompleteProviderFactory` | `AutocompleteProviderFactory` | `crate::core::extensions::types::AutocompleteProviderFactory` |  |
| `BashToolCallEvent` | `ToolCallEvent` | `core::extensions::types::ToolCallEvent` | TS-only narrowed alias; existing event and is_tool_call_event_type preserve runtime behavior. |
| `BeforeAgentStartEvent` | `BeforeAgentStartEvent` | `crate::core::extensions::types::BeforeAgentStartEvent` |  |
| `BeforeAgentStartEventResult` | `BeforeAgentStartEventResult` | `crate::core::extensions::types::BeforeAgentStartEventResult` |  |
| `BeforeProviderRequestEvent` | `BeforeProviderRequestEvent` | `crate::core::extensions::types::BeforeProviderRequestEvent` |  |
| `BeforeProviderRequestEventResult` | `BeforeProviderRequestEventResult` | `crate::core::extensions::types::BeforeProviderRequestEventResult` |  |
| `BuildSystemPromptOptions` | `BuildSystemPromptOptions` | `crate::core::system_prompt::BuildSystemPromptOptions` |  |
| `CompactionReason` | `CompactionReason` | `crate::core::extensions::types::CompactionReason` |  |
| `CompactOptions` | `CompactOptions` | `crate::core::extensions::types::CompactOptions` |  |
| `ContextEvent` | `ContextEvent` | `crate::core::extensions::types::ContextEvent` |  |
| `ContextUsage` | `ContextUsage` | `crate::core::extensions::types::ContextUsage` |  |
| `CustomToolCallEvent` | `ToolCallEvent` | `core::extensions::types::ToolCallEvent` | TS-only narrowed alias; existing event and is_tool_call_event_type preserve runtime behavior. |
| `EditToolCallEvent` | `ToolCallEvent` | `core::extensions::types::ToolCallEvent` | TS-only narrowed alias; existing event and is_tool_call_event_type preserve runtime behavior. |
| `EntryRenderer` | `EntryRenderer` | `crate::core::extensions::types::EntryRenderer` |  |
| `EntryRenderOptions` | `EntryRenderOptions` | `crate::core::extensions::types::EntryRenderOptions` |  |
| `ExecOptions` | `ExecOptions` | `crate::core::exec::ExecOptions` |  |
| `ExecResult` | `ExecResult` | `crate::core::exec::ExecResult` |  |
| `Extension` | `Extension` | `crate::core::extensions::types::Extension` |  |
| `ExtensionActions` | `ExtensionActions` | `crate::core::extensions::types::ExtensionActions` |  |
| `ExtensionAPI` | `ExtensionApi` | `crate::core::extensions::types::ExtensionApi` |  |
| `ExtensionCommandContext` | `ExtensionCommandContext` | `crate::core::extensions::types::ExtensionCommandContext` |  |
| `ExtensionCommandContextActions` | `ExtensionCommandContextActions` | `crate::core::extensions::types::ExtensionCommandContextActions` |  |
| `ExtensionContext` | `ExtensionContext` | `crate::core::extensions::types::ExtensionContext` |  |
| `ExtensionContextActions` | `ExtensionContextActions` | `crate::core::extensions::types::ExtensionContextActions` |  |
| `ExtensionError` | `ExtensionError` | `crate::core::extensions::types::ExtensionError` |  |
| `ExtensionEvent` | `ExtensionEvent` | `crate::core::extensions::types::ExtensionEvent` |  |
| `ExtensionFactory` | `ExtensionFactory` | `crate::core::extensions::loader::ExtensionFactory` |  |
| `ExtensionFlag` | `ExtensionFlag` | `crate::core::extensions::types::ExtensionFlag` |  |
| `ExtensionHandler` | `Handler` | `crate::core::extensions::types::Handler` | Existing native representation; no wrapper type. |
| `ExtensionRuntime` | `ExtensionRuntime` | `crate::core::extensions::extension_runtime::ExtensionRuntime` |  |
| `ExtensionShortcut` | `ExtensionShortcut` | `crate::core::extensions::types::ExtensionShortcut` |  |
| `ExtensionUIContext` | `ExtensionUiContext` | `crate::core::extensions::types::ExtensionUiContext` |  |
| `ExtensionUIDialogOptions` | `ExtensionUiDialogOptions` | `crate::core::extensions::types::ExtensionUiDialogOptions` |  |
| `ExtensionWidgetOptions` | `ExtensionWidgetOptions` | `crate::core::extensions::types::ExtensionWidgetOptions` |  |
| `FindToolCallEvent` | `ToolCallEvent` | `core::extensions::types::ToolCallEvent` | TS-only narrowed alias; existing event and is_tool_call_event_type preserve runtime behavior. |
| `GrepToolCallEvent` | `ToolCallEvent` | `core::extensions::types::ToolCallEvent` | TS-only narrowed alias; existing event and is_tool_call_event_type preserve runtime behavior. |
| `InputEvent` | `InputEvent` | `crate::core::extensions::types::InputEvent` |  |
| `InputEventResult` | `InputEventResult` | `crate::core::extensions::types::InputEventResult` |  |
| `InputSource` | `InputSource` | `crate::core::extensions::types::InputSource` |  |
| `KeybindingsManager` | `KeybindingsManager` | `crate::core::keybindings::KeybindingsManager` |  |
| `LoadExtensionsResult` | `LoadExtensionsResult` | `crate::core::extensions::types::LoadExtensionsResult` |  |
| `LsToolCallEvent` | `ToolCallEvent` | `core::extensions::types::ToolCallEvent` | TS-only narrowed alias; existing event and is_tool_call_event_type preserve runtime behavior. |
| `MessageRenderer` | `MessageRenderer` | `crate::core::extensions::types::MessageRenderer` |  |
| `MessageRenderOptions` | `MessageRenderOptions` | `crate::core::extensions::types::MessageRenderOptions` |  |
| `ModelRequestEndEvent` | `ModelRequestEndEvent` | `crate::core::extensions::types::ModelRequestEndEvent` |  |
| `ModelRequestStartEvent` | `ModelRequestStartEvent` | `crate::core::extensions::types::ModelRequestStartEvent` |  |
| `ProjectTrustContext` | `ProjectTrustContext` | `crate::core::extensions::types::ProjectTrustContext` |  |
| `ProjectTrustEvent` | `ProjectTrustEvent` | `crate::core::extensions::types::ProjectTrustEvent` |  |
| `ProjectTrustEventDecision` | `ProjectTrustEventDecision` | `crate::core::extensions::types::ProjectTrustEventDecision` |  |
| `ProjectTrustEventResult` | `ProjectTrustEventResult` | `crate::core::extensions::types::ProjectTrustEventResult` |  |
| `ProjectTrustHandler` | `ProjectTrustHandler` | `crate::core::extensions::types::ProjectTrustHandler` |  |
| `ProviderConfig` | `ProviderConfig` | `crate::core::extensions::types::ProviderConfig` |  |
| `ProviderModelConfig` | `ProviderModelConfig` | `crate::core::extensions::types::ProviderModelConfig` |  |
| `ReadToolCallEvent` | `ToolCallEvent` | `core::extensions::types::ToolCallEvent` | TS-only narrowed alias; existing event and is_tool_call_event_type preserve runtime behavior. |
| `RegisteredCommand` | `RegisteredCommand` | `crate::core::extensions::types::RegisteredCommand` |  |
| `RegisteredTool` | `RegisteredTool` | `crate::core::extensions::types::RegisteredTool` |  |
| `ResolvedCommand` | `ResolvedCommand` | `crate::core::extensions::types::ResolvedCommand` |  |
| `SessionBeforeCompactEvent` | `SessionBeforeCompactEvent` | `crate::core::extensions::types::SessionBeforeCompactEvent` |  |
| `SessionBeforeForkEvent` | `SessionBeforeForkEvent` | `crate::core::extensions::types::SessionBeforeForkEvent` |  |
| `SessionBeforeSwitchEvent` | `SessionBeforeSwitchEvent` | `crate::core::extensions::types::SessionBeforeSwitchEvent` |  |
| `SessionBeforeTreeEvent` | `SessionBeforeTreeEvent` | `crate::core::extensions::types::SessionBeforeTreeEvent` |  |
| `SessionCompactEvent` | `SessionCompactEvent` | `crate::core::extensions::types::SessionCompactEvent` |  |
| `SessionInfoChangedEvent` | `SessionInfoChangedEvent` | `crate::core::agent_session::SessionInfoChangedEvent` |  |
| `SessionMutationTarget` | `SessionMutationTarget` | `crate::core::extensions::types::SessionMutationTarget` |  |
| `SessionShutdownEvent` | `SessionShutdownEvent` | `crate::core::extensions::types::SessionShutdownEvent` |  |
| `SessionStartEvent` | `SessionStartEvent` | `crate::core::extensions::types::SessionStartEvent` |  |
| `SessionTreeEvent` | `SessionTreeEvent` | `crate::core::extensions::types::SessionTreeEvent` |  |
| `SlashCommandInfo` | `SlashCommandInfo` | `crate::core::slash_commands::SlashCommandInfo` |  |
| `SlashCommandSource` | `SlashCommandSource` | `crate::core::slash_commands::SlashCommandSource` |  |
| `SourceInfo` | `SourceInfo` | `crate::core::source_info::SourceInfo` |  |
| `TerminalInputHandler` | `TerminalInputHandler` | `crate::core::extensions::types::TerminalInputHandler` |  |
| `ToolCallEvent` | `ToolCallEvent` | `crate::core::extensions::types::ToolCallEvent` |  |
| `ToolCallEventResult` | `ToolCallEventResult` | `crate::core::extensions::types::ToolCallEventResult` |  |
| `ToolDefinition` | `ToolDefinition` | `crate::core::extensions::types::ToolDefinition` |  |
| `ToolExecutionMode` | `ToolExecutionMode` | `pi_agent_core::harness::types::ToolExecutionMode` |  |
| `ToolGate` | `ToolGate` | `crate::core::extensions::types::ToolGate` |  |
| `ToolInfo` | `ToolInfo` | `crate::core::extensions::types::ToolInfo` |  |
| `ToolRenderResultOptions` | `ToolRenderResultOptions` | `crate::core::extensions::types::ToolRenderResultOptions` |  |
| `ToolResultEvent` | `ToolResultEvent` | `crate::core::extensions::types::ToolResultEvent` |  |
| `TurnEndEvent` | `TurnEndEvent` | `crate::core::extensions::types::TurnEndEvent` |  |
| `TurnStartEvent` | `TurnStartEvent` | `crate::core::extensions::types::TurnStartEvent` |  |
| `UserBashEvent` | `UserBashEvent` | `crate::core::extensions::types::UserBashEvent` |  |
| `UserBashEventResult` | `UserBashEventResult` | `crate::core::extensions::types::UserBashEventResult` |  |
| `WidgetPlacement` | `WidgetPlacement` | `crate::core::extensions::types::WidgetPlacement` |  |
| `WorkingIndicatorOptions` | `WorkingIndicatorOptions` | `crate::core::extensions::types::WorkingIndicatorOptions` |  |
| `WriteToolCallEvent` | `ToolCallEvent` | `core::extensions::types::ToolCallEvent` | TS-only narrowed alias; existing event and is_tool_call_event_type preserve runtime behavior. |
| `createExtensionRuntime` | `create_extension_runtime` | `crate::core::extensions::extension_runtime::create_extension_runtime` |  |
| `defineTool` | `define_tool` | `crate::core::extensions::types::define_tool` |  |
| `discoverAndLoadExtensions` | `discover_and_load_extensions` | `crate::core::extensions::loader::discover_and_load_extensions` |  |
| `ExtensionRunner` | `ExtensionRunner` | `crate::core::extensions::runner::ExtensionRunner` |  |
| `isBashToolResult` | `is_bash_tool_result` | `crate::core::extensions::types::is_bash_tool_result` |  |
| `isEditToolResult` | `is_edit_tool_result` | `crate::core::extensions::types::is_edit_tool_result` |  |
| `isFindToolResult` | `is_find_tool_result` | `crate::core::extensions::types::is_find_tool_result` |  |
| `isGrepToolResult` | `is_grep_tool_result` | `crate::core::extensions::types::is_grep_tool_result` |  |
| `isLsToolResult` | `is_ls_tool_result` | `crate::core::extensions::types::is_ls_tool_result` |  |
| `isReadToolResult` | `is_read_tool_result` | `crate::core::extensions::types::is_read_tool_result` |  |
| `isToolCallEventType` | `is_tool_call_event_type` | `crate::core::extensions::types::is_tool_call_event_type` |  |
| `isWriteToolResult` | `is_write_tool_result` | `crate::core::extensions::types::is_write_tool_result` |  |
| `wrapRegisteredTool` | `wrap_registered_tool` | `crate::core::extensions::wrapper::wrap_registered_tool` |  |
| `wrapRegisteredTools` | `wrap_registered_tools` | `crate::core::extensions::wrapper::wrap_registered_tools` |  |
| `ExtensionStatus` | `ExtensionStatus` | `crate::core::footer_data_provider::ExtensionStatus` |  |
| `FooterSessionOverride` | `FooterSessionOverride` | `crate::core::footer_data_provider::FooterSessionOverride` |  |
| `ReadonlyFooterDataProvider` | `FooterDataProvider` | `crate::core::footer_data_provider::FooterDataProvider` | Existing native representation; no wrapper type. |
| `convertToLlm` | `convert_to_llm` | `crate::core::messages::convert_to_llm` |  |
| `ModelRegistry` | `ModelRegistry` | `crate::core::model_registry::ModelRegistry` |  |
| `ModelScopeDiagnostic` | `ModelScopeDiagnostic` | `crate::core::model_resolver::ModelScopeDiagnostic` |  |
| `ResolveCliModelResult` | `ResolveCliModelResult` | `crate::core::model_resolver::ResolveCliModelResult` |  |
| `ResolveModelScopeResult` | `ResolveModelScopeResult` | `crate::core::model_resolver::ResolveModelScopeResult` |  |
| `resolveCliModel` | `resolve_cli_model` | `crate::core::model_resolver::resolve_cli_model` |  |
| `resolveModelScopeWithDiagnostics` | `resolve_model_scope_with_diagnostics` | `crate::core::model_resolver::resolve_model_scope_with_diagnostics` |  |
| `ScopedModel` | `ScopedModel` | `crate::core::model_resolver::ScopedModel` |  |
| `PackageManager` | `DefaultPackageManager` | `crate::core::package_manager::DefaultPackageManager` | Existing native representation; no wrapper type. |
| `PathMetadata` | `PathMetadata` | `crate::core::package_manager::PathMetadata` |  |
| `ProgressCallback` | `ProgressCallback` | `crate::core::package_manager::ProgressCallback` |  |
| `ProgressEvent` | `ProgressEvent` | `crate::core::package_manager::ProgressEvent` |  |
| `ResolvedPaths` | `ResolvedPaths` | `crate::core::package_manager::ResolvedPaths` |  |
| `ResolvedResource` | `ResolvedResource` | `crate::core::package_manager::ResolvedResource` |  |
| `DefaultPackageManager` | `DefaultPackageManager` | `crate::core::package_manager::DefaultPackageManager` |  |
| `ResourceCollision` | `ResourceCollision` | `crate::core::diagnostics::ResourceCollision` |  |
| `ResourceDiagnostic` | `ResourceDiagnostic` | `crate::core::diagnostics::ResourceDiagnostic` |  |
| `ResourceLoader` | `ResourceLoader` | `crate::core::resource_loader::ResourceLoader` |  |
| `DefaultResourceLoader` | `DefaultResourceLoader` | `crate::core::resource_loader::DefaultResourceLoader` |  |
| `loadProjectContextFiles` | `load_project_context_files` | `crate::core::resource_loader::load_project_context_files` |  |
| `AgentSessionRuntime` | `AgentSessionRuntime` | `crate::core::agent_session_runtime::AgentSessionRuntime` |  |
| `AgentSessionRuntimeDiagnostic` | `AgentSessionRuntimeDiagnostic` | `crate::core::agent_session_services::AgentSessionRuntimeDiagnostic` |  |
| `AgentSessionServices` | `AgentSessionServices` | `crate::core::agent_session_services::AgentSessionServices` |  |
| `CreateAgentSessionFromServicesOptions` | `CreateAgentSessionFromServicesOptions` | `crate::core::agent_session_services::CreateAgentSessionFromServicesOptions` |  |
| `CreateAgentSessionOptions` | `CreateAgentSessionOptions` | `crate::core::sdk::CreateAgentSessionOptions` |  |
| `CreateAgentSessionResult` | `CreateAgentSessionResult` | `crate::core::sdk::CreateAgentSessionResult` |  |
| `CreateAgentSessionRuntimeFactory` | `CreateAgentSessionRuntimeFactory` | `crate::core::agent_session_runtime::CreateAgentSessionRuntimeFactory` |  |
| `CreateAgentSessionRuntimeResult` | `CreateAgentSessionRuntimeResult` | `crate::core::agent_session_runtime::CreateAgentSessionRuntimeResult` |  |
| `CreateAgentSessionServicesOptions` | `CreateAgentSessionServicesOptions` | `crate::core::agent_session_services::CreateAgentSessionServicesOptions` |  |
| `createAgentSession` | `create_agent_session` | `crate::core::sdk::create_agent_session` |  |
| `createAgentSessionFromServices` | `create_agent_session_from_services` | `crate::core::agent_session_services::create_agent_session_from_services` |  |
| `createAgentSessionRuntime` | `create_agent_session_runtime` | `crate::core::agent_session_runtime::create_agent_session_runtime` |  |
| `createAgentSessionServices` | `create_agent_session_services` | `crate::core::agent_session_services::create_agent_session_services` |  |
| `createBashTool` | `create_bash_tool` | `crate::core::tools::bash::create_bash_tool` |  |
| `createCodingTools` | `create_coding_tools` | `crate::core::tools::create_coding_tools` |  |
| `createEditTool` | `create_edit_tool` | `crate::core::tools::edit::create_edit_tool` |  |
| `createFindTool` | `create_find_tool` | `crate::core::tools::find::create_find_tool` |  |
| `createGrepTool` | `create_grep_tool` | `crate::core::tools::grep::create_grep_tool` |  |
| `createLsTool` | `create_ls_tool` | `crate::core::tools::ls::create_ls_tool` |  |
| `createOutlineTool` | `create_outline_tool` | `crate::core::tools::code_index::create_outline_tool` |  |
| `createReadOnlyTools` | `create_read_only_tools` | `crate::core::tools::create_read_only_tools` |  |
| `createReadTool` | `create_read_tool` | `crate::core::tools::read::create_read_tool` |  |
| `createReferencesTool` | `create_references_tool` | `crate::core::tools::code_index::create_references_tool` |  |
| `createSymbolTool` | `create_symbol_tool` | `crate::core::tools::code_index::create_symbol_tool` |  |
| `createWriteTool` | `create_write_tool` | `crate::core::tools::write::create_write_tool` |  |
| `PromptTemplate` | `PromptTemplate` | `pi_agent_core::harness::types::PromptTemplate` |  |
| `BranchSummaryEntry` | `BranchSummaryEntry` | `crate::core::session_manager::BranchSummaryEntry` |  |
| `buildContextEntries` | `build_context_entries` | `crate::core::session_manager::build_context_entries` |  |
| `buildSessionContext` | `build_session_context` | `crate::core::session_manager::build_session_context` |  |
| `CompactionEntry` | `CompactionEntry` | `crate::core::session_manager::CompactionEntry` |  |
| `CURRENT_SESSION_VERSION` | `CURRENT_SESSION_VERSION` | `crate::core::session_manager::CURRENT_SESSION_VERSION` |  |
| `CustomEntry` | `CustomEntry` | `crate::core::session_manager::CustomEntry` |  |
| `CustomMessageEntry` | `CustomMessageEntry` | `crate::core::session_manager::CustomMessageEntry` |  |
| `FileEntry` | `FileEntry` | `crate::core::session_manager::FileEntry` |  |
| `getLatestCompactionEntry` | `get_latest_compaction_entry` | `crate::core::session_manager::get_latest_compaction_entry` |  |
| `ModelChangeEntry` | `ModelChangeEntry` | `crate::core::session_manager::ModelChangeEntry` |  |
| `migrateSessionEntries` | `migrate_session_entries` | `crate::core::session_manager::migrate_session_entries` |  |
| `NewSessionOptions` | `NewSessionOptions` | `crate::core::agent_session_runtime::NewSessionOptions` |  |
| `parseSessionEntries` | `parse_session_entries` | `crate::core::session_manager::parse_session_entries` |  |
| `SessionContext` | `SessionContext` | `crate::core::session_manager::SessionContext` |  |
| `SessionEntry` | `SessionEntry` | `crate::core::session_manager::SessionEntry` |  |
| `SessionEntryBase` | `SessionEntry` variants | `core::session_manager` | TS structural base fields live on the concrete native entry records; no fake base class. |
| `SessionHeader` | `SessionHeader` | `crate::core::session_manager::SessionHeader` |  |
| `SessionInfo` | `SessionInfo` | `crate::core::session_manager::SessionInfo` |  |
| `SessionInfoEntry` | `SessionInfoEntry` | `crate::core::session_manager::SessionInfoEntry` |  |
| `SessionManager` | `SessionManager` | `crate::core::session_manager::SessionManager` |  |
| `SessionMessageEntry` | `SessionMessageEntry` | `crate::core::session_manager::SessionMessageEntry` |  |
| `SessionTreeNode` | `SessionTreeNode` | `crate::core::session_manager::SessionTreeNode` |  |
| `sessionEntryToContextMessages` | `session_entry_to_context_messages` | `crate::core::session_manager::session_entry_to_context_messages` |  |
| `ThinkingLevelChangeEntry` | `ThinkingLevelChangeEntry` | `crate::core::session_manager::ThinkingLevelChangeEntry` |  |
| `MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES` | `MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES` | `crate::core::session_tool_output::MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES` |  |
| `migrateToolResultSessionFiles` | `migrate_tool_result_session_files` | `crate::core::session_tool_output::migrate_tool_result_session_files` |  |
| `serializeSessionEntryForPersistence` | `serialize_session_entry_for_persistence` | `crate::core::session_tool_output::serialize_session_entry_for_persistence` |  |
| `TOOL_RESULT_TRUNCATION_MARKER` | `TOOL_RESULT_TRUNCATION_MARKER` | `crate::core::session_tool_output::TOOL_RESULT_TRUNCATION_MARKER` |  |
| `ToolResultSessionMigrationReport` | `ToolResultSessionMigrationReport` | `crate::core::session_tool_output::ToolResultSessionMigrationReport` |  |
| `truncateToolResultForPersistence` | `truncate_tool_result_for_persistence` | `crate::core::session_tool_output::truncate_tool_result_for_persistence` |  |
| `CompactionSettings` | `CompactionSettings` | `crate::core::settings_manager::CompactionSettings` |  |
| `DefaultProjectTrust` | `DefaultProjectTrust` | `crate::core::settings_manager::DefaultProjectTrust` |  |
| `ImageSettings` | `ImageSettings` | `crate::core::settings_manager::ImageSettings` |  |
| `PackageSource` | `PackageSource` | `crate::core::settings_manager::PackageSource` |  |
| `RetrySettings` | `RetrySettings` | `crate::core::settings_manager::RetrySettings` |  |
| `SettingsManager` | `SettingsManager` | `crate::core::settings_manager::SettingsManager` |  |
| `SettingsManagerCreateOptions` | `SettingsManagerCreateOptions` | `crate::core::settings_manager::SettingsManagerCreateOptions` |  |
| `ParsedSkillBlock` | `ParsedSkillBlock` | `crate::core::ParsedSkillBlock` |  |
| `parseSkillBlock` | `parse_skill_block` | `crate::core::parse_skill_block` |  |
| `formatSkillsForPrompt` | `format_skills_for_prompt` | `crate::core::skills::format_skills_for_prompt` |  |
| `LoadSkillsFromDirOptions` | `LoadSkillsFromDirOptions` | `crate::core::skills::LoadSkillsFromDirOptions` |  |
| `LoadSkillsResult` | `LoadSkillsResult` | `crate::core::skills::LoadSkillsResult` |  |
| `loadSkills` | `load_skills` | `crate::core::skills::load_skills` |  |
| `loadSkillsFromDir` | `load_skills_from_dir` | `crate::core::skills::load_skills_from_dir` |  |
| `Skill` | `Skill` | `crate::core::skills::Skill` |  |
| `SkillFrontmatter` | `SkillFrontmatter` | `crate::core::skills::SkillFrontmatter` |  |
| `createSyntheticSourceInfo` | `create_synthetic_source_info` | `crate::core::source_info::create_synthetic_source_info` |  |
| `EditDiffResult` | `EditDiffResult` | `crate::core::tools::edit_diff::EditDiffResult` |  |
| `generateDiffString` | `generate_diff_string` | `crate::core::tools::edit_diff::generate_diff_string` |  |
| `generateUnifiedPatch` | `generate_unified_patch` | `crate::core::tools::edit_diff::generate_unified_patch` |  |
| `AskArchitectToolDetails` | `AskArchitectToolDetails` | `crate::core::tools::ask_architect::AskArchitectToolDetails` |  |
| `AskArchitectToolInput` | `AskArchitectToolInput` | `crate::core::tools::ask_architect::AskArchitectToolInput` |  |
| `AskQuestionsToolDetails` | Tool arguments/details | `pi_js_compat::js_json::JsValue` | TS schema-only alias; native tool schema validation and lossless values are the runtime contract. |
| `AskQuestionsToolInput` | Tool arguments/details | `pi_js_compat::js_json::JsValue` | TS schema-only alias; native tool schema validation and lossless values are the runtime contract. |
| `BashOperations` | `BashOperations` | `crate::core::tools::bash::BashOperations` |  |
| `BashSpawnContext` | `BashSpawnContext` | `crate::core::tools::bash::BashSpawnContext` |  |
| `BashSpawnHook` | `BashSpawnHook` | `crate::core::tools::bash::BashSpawnHook` |  |
| `BashToolDetails` | Tool arguments/details | `pi_js_compat::js_json::JsValue` | TS schema-only alias; native tool schema validation and lossless values are the runtime contract. |
| `BashToolInput` | Tool arguments/details | `pi_js_compat::js_json::JsValue` | TS schema-only alias; native tool schema validation and lossless values are the runtime contract. |
| `BashToolOptions` | `BashToolOptions` | `crate::core::tools::bash::BashToolOptions` |  |
| `CodeIndexCommandResult` | `CodeIndexCommandResult` | `crate::core::tools::code_index::CodeIndexCommandResult` |  |
| `CodeIndexOperations` | `CodeIndexOperations` | `crate::core::tools::code_index::CodeIndexOperations` |  |
| `CodeIndexToolDetails` | Tool arguments/details | `pi_js_compat::js_json::JsValue` | TS schema-only alias; native tool schema validation and lossless values are the runtime contract. |
| `CodeIndexToolOptions` | `CodeIndexToolOptions` | `crate::core::tools::code_index::CodeIndexToolOptions` |  |
| `createAskArchitectToolDefinition` | `create_ask_architect_tool_definition` | `crate::core::tools::ask_architect::create_ask_architect_tool_definition` |  |
| `createAskQuestionsToolDefinition` | `create_ask_questions_tool_definition` | `crate::core::tools::ask_questions::create_ask_questions_tool_definition` |  |
| `createBashToolDefinition` | `create_bash_tool_definition` | `crate::core::tools::bash::create_bash_tool_definition` |  |
| `createEditToolDefinition` | `create_edit_tool_definition` | `crate::core::tools::edit::create_edit_tool_definition` |  |
| `createFindToolDefinition` | `create_find_tool_definition` | `crate::core::tools::find::create_find_tool_definition` |  |
| `createGrepToolDefinition` | `create_grep_tool_definition` | `crate::core::tools::grep::create_grep_tool_definition` |  |
| `createLocalBashOperations` | `create_local_bash_operations` | `crate::core::tools::bash::create_local_bash_operations` |  |
| `createLsToolDefinition` | `create_ls_tool_definition` | `crate::core::tools::ls::create_ls_tool_definition` |  |
| `createOutlineToolDefinition` | `create_outline_tool_definition` | `crate::core::tools::code_index::create_outline_tool_definition` |  |
| `createReadToolDefinition` | `create_read_tool_definition` | `crate::core::tools::read::create_read_tool_definition` |  |
| `createReferencesToolDefinition` | `create_references_tool_definition` | `crate::core::tools::code_index::create_references_tool_definition` |  |
| `createSymbolToolDefinition` | `create_symbol_tool_definition` | `crate::core::tools::code_index::create_symbol_tool_definition` |  |
| `createWriteToolDefinition` | `create_write_tool_definition` | `crate::core::tools::write::create_write_tool_definition` |  |
| `DEFAULT_MAX_BYTES` | `DEFAULT_MAX_BYTES` | `crate::core::tools::DEFAULT_MAX_BYTES` |  |
| `DEFAULT_MAX_LINES` | `DEFAULT_MAX_LINES` | `crate::core::tools::DEFAULT_MAX_LINES` |  |
| `EditOperations` | `EditOperations` | `crate::core::tools::edit::EditOperations` |  |
| `EditToolDetails` | Tool arguments/details | `pi_js_compat::js_json::JsValue` | TS schema-only alias; native tool schema validation and lossless values are the runtime contract. |
| `EditToolInput` | Tool arguments/details | `pi_js_compat::js_json::JsValue` | TS schema-only alias; native tool schema validation and lossless values are the runtime contract. |
| `EditToolOptions` | `EditToolOptions` | `crate::core::tools::edit::EditToolOptions` |  |
| `FindOperations` | `FindOperations` | `crate::core::tools::find::FindOperations` |  |
| `FindToolDetails` | Tool arguments/details | `pi_js_compat::js_json::JsValue` | TS schema-only alias; native tool schema validation and lossless values are the runtime contract. |
| `FindToolInput` | Tool arguments/details | `pi_js_compat::js_json::JsValue` | TS schema-only alias; native tool schema validation and lossless values are the runtime contract. |
| `FindToolOptions` | `FindToolOptions` | `crate::core::tools::find::FindToolOptions` |  |
| `formatSize` | `format_size` | `crate::core::tools::format_size` |  |
| `GrepOperations` | `GrepOperations` | `crate::core::tools::grep::GrepOperations` |  |
| `GrepToolDetails` | Tool arguments/details | `pi_js_compat::js_json::JsValue` | TS schema-only alias; native tool schema validation and lossless values are the runtime contract. |
| `GrepToolInput` | Tool arguments/details | `pi_js_compat::js_json::JsValue` | TS schema-only alias; native tool schema validation and lossless values are the runtime contract. |
| `GrepToolOptions` | `GrepToolOptions` | `crate::core::tools::grep::GrepToolOptions` |  |
| `LsOperations` | `LsOperations` | `crate::core::tools::ls::LsOperations` |  |
| `LsToolDetails` | Tool arguments/details | `pi_js_compat::js_json::JsValue` | TS schema-only alias; native tool schema validation and lossless values are the runtime contract. |
| `LsToolInput` | Tool arguments/details | `pi_js_compat::js_json::JsValue` | TS schema-only alias; native tool schema validation and lossless values are the runtime contract. |
| `LsToolOptions` | `LsToolOptions` | `crate::core::tools::ls::LsToolOptions` |  |
| `OutlineToolInput` | Tool arguments/details | `pi_js_compat::js_json::JsValue` | TS schema-only alias; native tool schema validation and lossless values are the runtime contract. |
| `ReadOperations` | `ReadOperations` | `crate::core::tools::read::ReadOperations` |  |
| `ReadToolDetails` | Tool arguments/details | `pi_js_compat::js_json::JsValue` | TS schema-only alias; native tool schema validation and lossless values are the runtime contract. |
| `ReadToolInput` | Tool arguments/details | `pi_js_compat::js_json::JsValue` | TS schema-only alias; native tool schema validation and lossless values are the runtime contract. |
| `ReadToolOptions` | `ReadToolOptions` | `crate::core::tools::read::ReadToolOptions` |  |
| `ReferencesToolInput` | Tool arguments/details | `pi_js_compat::js_json::JsValue` | TS schema-only alias; native tool schema validation and lossless values are the runtime contract. |
| `SymbolToolInput` | Tool arguments/details | `pi_js_compat::js_json::JsValue` | TS schema-only alias; native tool schema validation and lossless values are the runtime contract. |
| `ToolsOptions` | `ToolsOptions` | `crate::core::tools::ToolsOptions` |  |
| `TruncationOptions` | `TruncationOptions` | `crate::core::tools::TruncationOptions` |  |
| `TruncationResult` | `TruncationResult` | `crate::core::tools::TruncationResult` |  |
| `truncateHead` | `truncate_head` | `crate::core::tools::truncate_head` |  |
| `truncateLine` | `truncate_line` | `crate::core::tools::truncate_line` |  |
| `truncateTail` | `truncate_tail` | `crate::core::tools::truncate_tail` |  |
| `WriteOperations` | `WriteOperations` | `crate::core::tools::write::WriteOperations` |  |
| `WriteToolInput` | Tool arguments/details | `pi_js_compat::js_json::JsValue` | TS schema-only alias; native tool schema validation and lossless values are the runtime contract. |
| `WriteToolOptions` | `WriteToolOptions` | `crate::core::tools::write::WriteToolOptions` |  |
| `withFileMutationQueue` | `with_file_mutation_queue` | `crate::core::tools::file_mutation_queue::with_file_mutation_queue` |  |
| `hasTrustRequiringProjectResources` | `has_trust_requiring_project_resources` | `crate::core::trust_manager::has_trust_requiring_project_resources` |  |
| `ProjectTrustDecision` | `ProjectTrustDecision` | `crate::core::trust_manager::ProjectTrustDecision` |  |
| `ProjectTrustStore` | `ProjectTrustStore` | `crate::core::trust_manager::ProjectTrustStore` |  |
| `ProjectTrustStoreEntry` | `ProjectTrustStoreEntry` | `crate::core::trust_manager::ProjectTrustStoreEntry` |  |
| `ProjectTrustUpdate` | `ProjectTrustUpdate` | `crate::core::trust_manager::ProjectTrustUpdate` |  |
| `MainOptions` | `MainOptions` | `crate::main_runner::MainOptions` |  |
| `main` | `main` | `crate::main_runner::main` |  |
| `InteractiveMode` | `InteractiveMode` | `crate::modes::interactive::interactive_mode::InteractiveMode` |  |
| `InteractiveModeOptions` | `InteractiveModeOptions` | `crate::modes::interactive::interactive_mode::InteractiveModeOptions` |  |
| `ModelInfo` | `ModelInfo` | `crate::modes::rpc::rpc_client::ModelInfo` |  |
| `PrintModeOptions` | `PrintModeOptions` | `crate::modes::print_mode::PrintModeOptions` |  |
| `RpcClient` | `RpcClient` | `crate::modes::rpc::rpc_client::RpcClient` |  |
| `RpcClientOptions` | `RpcClientOptions` | `crate::modes::rpc::rpc_client::RpcClientOptions` |  |
| `RpcCommand` | `RpcCommand` | `crate::modes::rpc::rpc_types::RpcCommand` |  |
| `RpcCommandBody` | `RpcCommandBody` | `crate::modes::rpc::rpc_types::RpcCommandBody` |  |
| `RpcEventListener` | `RpcEventListener` | `crate::modes::rpc::rpc_client::RpcEventListener` |  |
| `RpcExtensionUIRequest` | `RpcExtensionUiRequest` | `crate::modes::rpc::rpc_types::RpcExtensionUiRequest` |  |
| `RpcExtensionUIResponse` | `RpcExtensionUiResponse` | `crate::modes::rpc::rpc_types::RpcExtensionUiResponse` |  |
| `RpcResponse` | `RpcResponse` | `crate::modes::rpc::rpc_types::RpcResponse` |  |
| `RpcSessionState` | `RpcSessionState` | `crate::modes::rpc::rpc_types::RpcSessionState` |  |
| `runPrintMode` | `run_print_mode` | `crate::modes::print_mode::run_print_mode` |  |
| `runRpcMode` | `run_rpc_mode` | `crate::modes::rpc::rpc_mode::run_rpc_mode` |  |
| `ArminComponent` | `ArminComponent` | `crate::modes::interactive::components::armin::ArminComponent` |  |
| `AssistantMessageComponent` | `AssistantMessageComponent` | `crate::modes::interactive::components::assistant_message::AssistantMessageComponent` |  |
| `BashExecutionComponent` | `BashExecutionComponent` | `crate::modes::interactive::components::bash_execution::BashExecutionComponent` |  |
| `BorderedLoader` | `BorderedLoader` | `crate::modes::interactive::components::bordered_loader::BorderedLoader` |  |
| `BranchSummaryMessageComponent` | `BranchSummaryMessageComponent` | `crate::modes::interactive::components::branch_summary_message::BranchSummaryMessageComponent` |  |
| `CompactionSummaryMessageComponent` | `CompactionSummaryMessageComponent` | `crate::modes::interactive::components::compaction_summary_message::CompactionSummaryMessageComponent` |  |
| `CustomEditor` | `CustomEditor` | `crate::modes::interactive::components::custom_editor::CustomEditor` |  |
| `CustomMessageComponent` | `CustomMessageComponent` | `crate::modes::interactive::components::custom_message::CustomMessageComponent` |  |
| `DynamicBorder` | `DynamicBorder` | `pi_tui::components::dynamic_border::DynamicBorder` |  |
| `ExtensionEditorComponent` | `ExtensionEditorComponent` | `crate::modes::interactive::components::extension_editor::ExtensionEditorComponent` |  |
| `ExtensionInputComponent` | `ExtensionInputComponent` | `crate::modes::interactive::components::extension_input::ExtensionInputComponent` |  |
| `ExtensionSelectorComponent` | `ExtensionSelectorComponent` | `crate::modes::interactive::components::extension_selector::ExtensionSelectorComponent` |  |
| `FooterComponent` | `FooterComponent` | `crate::modes::interactive::components::footer::FooterComponent` |  |
| `keyHint` | `key_hint` | `crate::modes::interactive::components::keybinding_hints::key_hint` |  |
| `keyText` | `key_text` | `crate::modes::interactive::components::keybinding_hints::key_text` |  |
| `LoginDialogComponent` | `LoginDialogComponent` | `crate::modes::interactive::components::login_dialog::LoginDialogComponent` |  |
| `ModelSelectorComponent` | `ModelSelectorComponent` | `crate::modes::interactive::components::model_selector::ModelSelectorComponent` |  |
| `OAuthSelectorComponent` | `OAuthSelectorComponent` | `crate::modes::interactive::components::oauth_selector::OAuthSelectorComponent` |  |
| `RenderDiffOptions` | `render_diff` | `modes::interactive::components::diff` | Source filePath option is unused; no new carrier for an unobserved field. |
| `rawKeyHint` | `raw_key_hint` | `crate::modes::interactive::components::keybinding_hints::raw_key_hint` |  |
| `renderDiff` | `render_diff` | `crate::modes::interactive::components::diff::render_diff` |  |
| `SessionSelectorComponent` | `SessionSelectorComponent` | `crate::modes::interactive::components::session_selector::SessionSelectorComponent` |  |
| `SettingsCallbacks` | `SettingsCallbacks` | `crate::modes::interactive::components::settings_selector::SettingsCallbacks` |  |
| `SettingsConfig` | `SettingsConfig` | `crate::modes::interactive::components::settings_selector::SettingsConfig` |  |
| `SettingsSelectorComponent` | `SettingsSelectorComponent` | `crate::modes::interactive::components::settings_selector::SettingsSelectorComponent` |  |
| `ShowImagesSelectorComponent` | `ShowImagesSelectorComponent` | `crate::modes::interactive::components::show_images_selector::ShowImagesSelectorComponent` |  |
| `SkillInvocationMessageComponent` | `SkillInvocationMessageComponent` | `crate::modes::interactive::components::skill_invocation_message::SkillInvocationMessageComponent` |  |
| `ThemeSelectorComponent` | `ThemeSelectorComponent` | `crate::modes::interactive::components::theme_selector::ThemeSelectorComponent` |  |
| `ThinkingSelectorComponent` | `ThinkingSelectorComponent` | `crate::modes::interactive::components::thinking_selector::ThinkingSelectorComponent` |  |
| `ToolExecutionComponent` | `ToolExecutionComponent` | `crate::modes::interactive::components::tool_execution::ToolExecutionComponent` |  |
| `ToolExecutionOptions` | `ToolExecutionOptions` | `crate::modes::interactive::components::tool_execution::ToolExecutionOptions` |  |
| `TreeSelectorComponent` | `TreeSelectorComponent` | `crate::modes::interactive::components::tree_selector::TreeSelectorComponent` |  |
| `truncateToVisualLines` | `truncate_to_visual_lines` | `crate::modes::interactive::components::visual_truncate::truncate_to_visual_lines` |  |
| `UserMessageComponent` | `UserMessageComponent` | `crate::modes::interactive::components::user_message::UserMessageComponent` |  |
| `UserMessageSelectorComponent` | `UserMessageSelectorComponent` | `crate::modes::interactive::components::user_message_selector::UserMessageSelectorComponent` |  |
| `VisualTruncateResult` | `VisualTruncateResult` | `crate::modes::interactive::components::visual_truncate::VisualTruncateResult` |  |
| `getLanguageFromPath` | `get_language_from_path` | `crate::modes::interactive::theme::get_language_from_path` |  |
| `getMarkdownTheme` | `get_markdown_theme` | `crate::modes::interactive::theme::get_markdown_theme` |  |
| `getSelectListTheme` | `get_select_list_theme` | `crate::modes::interactive::theme::get_select_list_theme` |  |
| `getSettingsListTheme` | `get_settings_list_theme` | `crate::modes::interactive::theme::get_settings_list_theme` |  |
| `highlightCode` | `highlight_code` | `crate::modes::interactive::theme::highlight_code` |  |
| `initTheme` | `init_theme` | `crate::modes::interactive::theme::init_theme` |  |
| `Theme` | `Theme` | `crate::modes::interactive::theme::Theme` |  |
| `ThemeColor` | `ThemeColor` | `crate::modes::interactive::theme::ThemeColor` |  |
| `copyToClipboard` | `copy_to_clipboard` | `crate::utils::clipboard::copy_to_clipboard` |  |
| `parseFrontmatter` | `parse_frontmatter` | `crate::utils::frontmatter::parse_frontmatter` |  |
| `stripFrontmatter` | `strip_frontmatter` | `crate::utils::frontmatter::strip_frontmatter` |  |
| `convertToPng` | `convert_to_png` | `crate::utils::image_convert::convert_to_png` |  |
| `formatDimensionNote` | `format_dimension_note` | `crate::utils::image_resize::format_dimension_note` |  |
| `ResizedImage` | `ResizedImage` | `crate::utils::image_resize_core::ResizedImage` |  |
| `resizeImage` | `resize_image` | `crate::utils::image_resize::resize_image` |  |
| `getShellConfig` | `get_shell_config` | `crate::utils::shell::get_shell_config` |  |

## Proof

`tests/package_index.rs` imports the crate as an external consumer. The existing root test imports argument/config/truncation/session/tool/theme/event/SDK forwards and checks representative constants/formatting; the new test exercises full credential wire values and in-memory auth/status through root exports, native footer text and OAuth token access, and imports the newly forwarded event/model types. Public-import RED: `/tmp/settings-models-root-index-red.log`. Focused GREEN and changed-line clippy/readability pending.

## Gap

Adversarial/exotic inputs, unsupported OS/runtime faults and numeric extremes follow the repository parity bar; no new root carrier is introduced for them. Existing native-extension and module-specific approvals apply unchanged.
