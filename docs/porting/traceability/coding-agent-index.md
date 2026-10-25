# coding-agent package index traceability

Pinned source: `packages/coding-agent/src/index.ts` at the revision in
[source-lock.json](../source-lock.json). Rust: `crates/pi-coding-agent/src/lib.rs`.

Every source export maps to a crate-root path below. Rust renames follow the
crate's snake_case conventions. JS-only type helpers and type-only tool
records are listed explicitly with their runtime representation.

| Source export | Rust crate-root item | Notes |
| --- | --- | --- |
| `Args` | `cli::args::Args` |  |
| `parseArgs` | `cli::args::parse_args` |  |
| `CONFIG_DIR_NAME` | `config::config_dir_name` |  |
| `getAgentDir` | `config::get_agent_dir` |  |
| `getDocsPath` | `config::get_docs_path` |  |
| `getExamplesPath` | `config::get_examples_path` |  |
| `getPackageDir` | `config::get_package_dir` |  |
| `getReadmePath` | `config::get_readme_path` |  |
| `VERSION` | `config::version` |  |
| `AgentSession` | `core::agent_session::AgentSession` |  |
| `AgentSessionConfig` | `core::agent_session::AgentSessionConfig` |  |
| `AgentSessionEvent` | `core::agent_session::AgentSessionEvent` |  |
| `AgentSessionEventListener` | `core::agent_session::AgentSessionEventListener` |  |
| `ModelCycleResult` | `core::agent_session::ModelCycleResult` |  |
| `PromptOptions` | `core::agent_session::PromptOptions` |  |
| `SessionStats` | `core::agent_session::SessionStats` |  |
| `AuthStorage` | `core::auth_storage::AuthStorage` |  |
| `AuthStorageBackend` | `core::auth_storage::AuthStorageBackend` |  |
| `FileAuthStorageBackend` | `core::auth_storage::FileAuthStorageBackend` |  |
| `InMemoryAuthStorageBackend` | `core::auth_storage::InMemoryAuthStorageBackend` |  |
| `readStoredCredential` | `core::auth_storage::read_stored_credential` |  |
| `BranchPreparation` | `core::compaction::branch_summarization::BranchPreparation` |  |
| `BranchSummaryResult` | `core::compaction::branch_summarization::BranchSummaryResult` |  |
| `CollectEntriesResult` | `core::compaction::branch_summarization::CollectEntriesResult` |  |
| `CompactionResult` | `core::compaction::CompactionResult` |  |
| `CutPointResult` | `core::compaction::CutPointResult` |  |
| `calculateContextTokens` | `core::compaction::calculate_context_tokens` |  |
| `collectEntriesForBranchSummary` | `core::compaction::branch_summarization::collect_entries_for_branch_summary` |  |
| `compact` | `core::compaction::compact` |  |
| `DEFAULT_COMPACTION_SETTINGS` | `pi_agent_core::harness::compaction::DEFAULT_COMPACTION_SETTINGS` |  |
| `estimateTokens` | `core::compaction::estimate_tokens` |  |
| `FileOperations` | `core::compaction::FileOperations` |  |
| `findCutPoint` | `core::compaction::find_cut_point` |  |
| `findTurnStartIndex` | `core::compaction::find_turn_start_index` |  |
| `GenerateBranchSummaryOptions` | `core::compaction::branch_summarization::GenerateBranchSummaryOptions` |  |
| `generateBranchSummary` | `core::compaction::branch_summarization::generate_branch_summary` |  |
| `generateSummary` | `core::compaction::generate_summary` |  |
| `generateSummaryWithUsage` | `core::compaction::generate_summary_with_usage` |  |
| `getLastAssistantUsage` | `core::compaction::get_last_assistant_usage` |  |
| `prepareBranchEntries` | `core::compaction::branch_summarization::prepare_branch_entries` |  |
| `serializeConversation` | `core::compaction::utils::serialize_conversation` |  |
| `shouldCompact` | `core::compaction::should_compact` |  |
| `createEventBus` | `core::event_bus::create_event_bus` |  |
| `EventBus` | `core::event_bus::EventBus` |  |
| `EventBusController` | `core::event_bus::EventBusController` |  |
| `AgentEndEvent` | `core::agent_session::AgentEndEvent` |  |
| `AgentStartEvent` | `core::extensions::types::AgentStartEvent` |  |
| `AgentToolResult` | `pi_agent_core::agent::types::AgentToolResult` |  |
| `AgentToolUpdateCallback` | `pi_agent_core::agent::types::AgentToolUpdateCallback` |  |
| `AppKeybinding` | `pi_tui::keybindings::Keybinding` |  |
| `AutocompleteProviderFactory` | `core::extensions::types::AutocompleteProviderFactory` |  |
| `BashToolCallEvent` | — | excluded: per-tool type alias over `ToolCallEvent`; Rust keeps the single struct plus `is_tool_call_event_type`. |
| `BeforeAgentStartEvent` | `core::extensions::types::BeforeAgentStartEvent` |  |
| `BeforeAgentStartEventResult` | `core::extensions::types::BeforeAgentStartEventResult` |  |
| `BeforeProviderRequestEvent` | `core::extensions::types::BeforeProviderRequestEvent` |  |
| `BeforeProviderRequestEventResult` | `core::extensions::types::BeforeProviderRequestEventResult` |  |
| `BuildSystemPromptOptions` | `core::system_prompt::BuildSystemPromptOptions` |  |
| `CompactOptions` | `core::extensions::types::CompactOptions` |  |
| `ContextEvent` | `core::extensions::types::ContextEvent` |  |
| `ContextUsage` | `core::extensions::types::ContextUsage` |  |
| `CustomToolCallEvent` | — | excluded: per-tool type alias over `ToolCallEvent`. |
| `EditToolCallEvent` | — | excluded: per-tool type alias over `ToolCallEvent`. |
| `EntryRenderer` | `core::extensions::types::EntryRenderer` |  |
| `EntryRenderOptions` | `core::extensions::types::EntryRenderOptions` |  |
| `ExecOptions` | `core::exec::ExecOptions` |  |
| `ExecResult` | `core::exec::ExecResult` |  |
| `Extension` | `core::extensions::types::Extension` |  |
| `ExtensionActions` | `core::extensions::types::ExtensionActions` |  |
| `ExtensionAPI` | `core::extensions::types::ExtensionApi` |  |
| `ExtensionCommandContext` | `core::extensions::types::ExtensionCommandContext` |  |
| `ExtensionCommandContextActions` | `core::extensions::types::ExtensionCommandContextActions` |  |
| `ExtensionContext` | `core::extensions::types::ExtensionContext` |  |
| `ExtensionContextActions` | `core::extensions::types::ExtensionContextActions` |  |
| `ExtensionError` | `core::extensions::types::ExtensionError` |  |
| `ExtensionEvent` | `core::extensions::types::ExtensionEvent` |  |
| `ExtensionFactory` | `core::extensions::loader::ExtensionFactory` |  |
| `ExtensionFlag` | `core::extensions::types::ExtensionFlag` |  |
| `ExtensionHandler` | `core::extensions::types::Handler` |  |
| `ExtensionRuntime` | `core::extensions::extension_runtime::ExtensionRuntime` |  |
| `ExtensionShortcut` | `core::extensions::types::ExtensionShortcut` |  |
| `ExtensionUIContext` | `core::extensions::types::ExtensionUiContext` |  |
| `ExtensionUIDialogOptions` | `core::extensions::types::ExtensionUiDialogOptions` |  |
| `ExtensionWidgetOptions` | `core::extensions::types::ExtensionWidgetOptions` |  |
| `FindToolCallEvent` | — | excluded: per-tool type alias over `ToolCallEvent`. |
| `GrepToolCallEvent` | — | excluded: per-tool type alias over `ToolCallEvent`. |
| `InlineExtension` | `core::resource_loader::InlineExtension` |  |
| `InputEvent` | `core::extensions::types::InputEvent` |  |
| `InputEventResult` | `core::extensions::types::InputEventResult` |  |
| `InputSource` | `core::extensions::types::InputSource` |  |
| `KeybindingsManager` | `core::keybindings::KeybindingsManager` |  |
| `LoadExtensionsResult` | `core::extensions::types::LoadExtensionsResult` |  |
| `LsToolCallEvent` | — | excluded: per-tool type alias over `ToolCallEvent`. |
| Old-only `MarkdownTransformContext` / `MarkdownTransformer` | Removed from public barrel; absent at frozen `4f9e6f8a4` | [removal ledger](extensions-types.md#old-only-markdown-api-removal-2026-10-25) |
| `MessageEndEvent` | `core::extensions::types::MessageEndEvent` |  |
| `MessageRenderer` | `core::extensions::types::MessageRenderer` |  |
| `MessageRenderOptions` | `core::extensions::types::MessageRenderOptions` |  |
| `MessageStartEvent` | `core::extensions::types::MessageStartEvent` |  |
| `MessageUpdateEvent` | `core::extensions::types::MessageUpdateEvent` |  |
| `ProjectTrustContext` | `core::extensions::types::ProjectTrustContext` |  |
| `ProjectTrustEvent` | `core::extensions::types::ProjectTrustEvent` |  |
| `ProjectTrustEventDecision` | `core::extensions::types::ProjectTrustEventDecision` |  |
| `ProjectTrustEventResult` | `core::extensions::types::ProjectTrustEventResult` |  |
| `ProjectTrustHandler` | `core::extensions::types::ProjectTrustHandler` |  |
| `ProviderConfig` | `core::extensions::types::ProviderConfig` |  |
| `ProviderModelConfig` | `core::extensions::types::ProviderModelConfig` |  |
| `ReadToolCallEvent` | — | excluded: per-tool type alias over `ToolCallEvent`. |
| `RegisteredCommand` | `core::extensions::types::RegisteredCommand` |  |
| `RegisteredTool` | `core::extensions::types::RegisteredTool` |  |
| `ResolvedCommand` | `core::extensions::types::ResolvedCommand` |  |
| `SessionBeforeCompactEvent` | `core::extensions::types::SessionBeforeCompactEvent` |  |
| `SessionBeforeForkEvent` | `core::extensions::types::SessionBeforeForkEvent` |  |
| `SessionBeforeSwitchEvent` | `core::extensions::types::SessionBeforeSwitchEvent` |  |
| `SessionBeforeTreeEvent` | `core::extensions::types::SessionBeforeTreeEvent` |  |
| `SessionCompactEvent` | `core::extensions::types::SessionCompactEvent` |  |
| `SessionInfoChangedEvent` | `core::agent_session::SessionInfoChangedEvent` |  |
| `SessionMutationTarget` | `core::extensions::types::SessionMutationTarget` |  |
| `SessionShutdownEvent` | `core::extensions::types::SessionShutdownEvent` |  |
| `SessionStartEvent` | `core::extensions::types::SessionStartEvent` |  |
| `SessionTreeEvent` | `core::extensions::types::SessionTreeEvent` |  |
| `SlashCommandInfo` | `core::slash_commands::SlashCommandInfo` |  |
| `SlashCommandSource` | `core::slash_commands::SlashCommandSource` |  |
| `SourceInfo` | `core::source_info::SourceInfo` |  |
| `TerminalInputHandler` | `core::extensions::types::TerminalInputHandler` |  |
| `ToolCallEvent` | `core::extensions::types::ToolCallEvent` |  |
| `ToolCallEventResult` | `core::extensions::types::ToolCallEventResult` |  |
| `ToolDefinition` | `core::extensions::types::ToolDefinition` |  |
| `ToolExecutionEndEvent` | `core::extensions::types::ToolExecutionEndEvent` |  |
| `ToolExecutionMode` | `pi_agent_core::harness::types::ToolExecutionMode` |  |
| `ToolExecutionStartEvent` | `core::extensions::types::ToolExecutionStartEvent` |  |
| `ToolExecutionUpdateEvent` | `core::extensions::types::ToolExecutionUpdateEvent` |  |
| `ToolGate` | `core::extensions::types::ToolGate` |  |
| `ToolInfo` | `core::extensions::types::ToolInfo` |  |
| `ToolRenderResultOptions` | `core::extensions::types::ToolRenderResultOptions` |  |
| `ToolResultEvent` | `core::extensions::types::ToolResultEvent` |  |
| `TurnEndEvent` | `core::extensions::types::TurnEndEvent` |  |
| `TurnStartEvent` | `core::extensions::types::TurnStartEvent` |  |
| `UserBashEvent` | `core::extensions::types::UserBashEvent` |  |
| `UserBashEventResult` | `core::extensions::types::UserBashEventResult` |  |
| `WidgetPlacement` | `core::extensions::types::WidgetPlacement` |  |
| `WorkingIndicatorOptions` | `core::extensions::types::WorkingIndicatorOptions` |  |
| `WriteToolCallEvent` | — | excluded: per-tool type alias over `ToolCallEvent`. |
| `createExtensionRuntime` | `core::extensions::extension_runtime::create_extension_runtime` |  |
| `defineTool` | `core::extensions::types::define_tool` |  |
| `discoverAndLoadExtensions` | `core::extensions::loader::discover_and_load_extensions` |  |
| `ExtensionRunner` | `core::extensions::runner::ExtensionRunner` |  |
| `isBashToolResult` | `core::extensions::types::is_bash_tool_result` |  |
| `isEditToolResult` | `core::extensions::types::is_edit_tool_result` |  |
| `isFindToolResult` | `core::extensions::types::is_find_tool_result` |  |
| `isGrepToolResult` | `core::extensions::types::is_grep_tool_result` |  |
| `isLsToolResult` | `core::extensions::types::is_ls_tool_result` |  |
| `isReadToolResult` | `core::extensions::types::is_read_tool_result` |  |
| `isToolCallEventType` | `core::extensions::types::is_tool_call_event_type` |  |
| `isWriteToolResult` | `core::extensions::types::is_write_tool_result` |  |
| `wrapRegisteredTool` | `core::extensions::wrapper::wrap_registered_tool` |  |
| `wrapRegisteredTools` | `core::extensions::wrapper::wrap_registered_tools` |  |
| `FooterSessionOverride` | `core::footer_data_provider::FooterSessionOverride` |  |
| `ReadonlyFooterDataProvider` | `core::footer_data_provider::FooterDataProvider` |  |
| `convertToLlm` | `core::messages::convert_to_llm` |  |
| `ModelRegistry` | `core::model_registry::ModelRegistry` |  |
| `ModelScopeDiagnostic` | `core::model_resolver::ModelScopeDiagnostic` |  |
| `ResolveCliModelResult` | `core::model_resolver::ResolveCliModelResult` |  |
| `ResolveModelScopeResult` | `core::model_resolver::ResolveModelScopeResult` |  |
| `resolveCliModel` | `core::model_resolver::resolve_cli_model` |  |
| `resolveModelScopeWithDiagnostics` | `core::model_resolver::resolve_model_scope_with_diagnostics` |  |
| `ScopedModel` | `core::model_resolver::ScopedModel` |  |
| `CreateModelRuntimeOptions` | `core::model_runtime::CreateModelRuntimeOptions` |  |
| `CredentialSynchronizationError` | `core::model_runtime::CredentialSynchronizationError` |  |
| `CredentialSynchronizationOperation` | `core::model_runtime::CredentialSynchronizationOperation` |  |
| `ModelRuntime` | `core::model_runtime::ModelRuntime` |  |
| `ModelRuntimeAuthOverrides` | `pi_ai::auth::resolve::AuthResolutionOverrides` |  |
| `PackageManager` | `core::package_manager::DefaultPackageManager` |  |
| `PathMetadata` | `core::package_manager::PathMetadata` |  |
| `ProgressCallback` | `core::package_manager::ProgressCallback` |  |
| `ProgressEvent` | `core::package_manager::ProgressEvent` |  |
| `ResolvedPaths` | `core::package_manager::ResolvedPaths` |  |
| `ResolvedResource` | `core::package_manager::ResolvedResource` |  |
| `DefaultPackageManager` | `core::package_manager::DefaultPackageManager` |  |
| `ResourceCollision` | `core::diagnostics::ResourceCollision` |  |
| `ResourceDiagnostic` | `core::diagnostics::ResourceDiagnostic` |  |
| `ResourceLoader` | `core::resource_loader::ResourceLoader` |  |
| `DefaultResourceLoader` | `core::resource_loader::DefaultResourceLoader` |  |
| `loadProjectContextFiles` | `core::resource_loader::load_project_context_files` |  |
| `AgentSessionRuntime` | `core::agent_session_runtime::AgentSessionRuntime` |  |
| `AgentSessionRuntimeDiagnostic` | `core::agent_session_services::AgentSessionRuntimeDiagnostic` |  |
| `AgentSessionServices` | `core::agent_session_services::AgentSessionServices` |  |
| `CreateAgentSessionFromServicesOptions` | `core::agent_session_services::CreateAgentSessionFromServicesOptions` |  |
| `CreateAgentSessionOptions` | `core::sdk::CreateAgentSessionOptions` |  |
| `CreateAgentSessionResult` | `core::sdk::CreateAgentSessionResult` |  |
| `CreateAgentSessionRuntimeFactory` | `core::agent_session_runtime::CreateAgentSessionRuntimeFactory` |  |
| `CreateAgentSessionRuntimeResult` | `core::agent_session_runtime::CreateAgentSessionRuntimeResult` |  |
| `CreateAgentSessionServicesOptions` | `core::agent_session_services::CreateAgentSessionServicesOptions` |  |
| `createAgentSession` | `core::sdk::create_agent_session` |  |
| `createAgentSessionFromServices` | `core::agent_session_services::create_agent_session_from_services` |  |
| `createAgentSessionRuntime` | `core::agent_session_runtime::create_agent_session_runtime` |  |
| `createAgentSessionServices` | `core::agent_session_services::create_agent_session_services` |  |
| `createBashTool` | `core::tools::bash::create_bash_tool` |  |
| `createCodingTools` | `core::tools::create_coding_tools` | takes an additional `&ToolContextFactory` (Rust execution-context carrier) |
| `createEditTool` | `core::tools::edit::create_edit_tool` |  |
| `createFindTool` | `core::tools::find::create_find_tool` |  |
| `createGrepTool` | `core::tools::grep::create_grep_tool` |  |
| `createLsTool` | `core::tools::ls::create_ls_tool` |  |
| `createOutlineTool` | `core::tools::code_index::create_outline_tool` |  |
| `createReadOnlyTools` | `core::tools::create_read_only_tools` | takes an additional `&ToolContextFactory` |
| `createReadTool` | `core::tools::read::create_read_tool` |  |
| `createReferencesTool` | `core::tools::code_index::create_references_tool` |  |
| `createSymbolTool` | `core::tools::code_index::create_symbol_tool` |  |
| `createWriteTool` | `core::tools::write::create_write_tool` |  |
| `PromptTemplate` | `pi_agent_core::harness::types::PromptTemplate` |  |
| `BranchSummaryEntry` | `core::session_manager::BranchSummaryEntry` |  |
| `buildContextEntries` | `core::session_manager::build_context_entries` |  |
| `buildSessionContext` | `core::session_manager::build_session_context` |  |
| `CompactionEntry` | `core::session_manager::CompactionEntry` |  |
| `CURRENT_SESSION_VERSION` | `core::session_manager::CURRENT_SESSION_VERSION` |  |
| `CustomEntry` | `core::session_manager::CustomEntry` |  |
| `CustomMessageEntry` | `core::session_manager::CustomMessageEntry` |  |
| `FileEntry` | `core::session_manager::FileEntry` |  |
| `getLatestCompactionEntry` | `core::session_manager::get_latest_compaction_entry` |  |
| `ModelChangeEntry` | `core::session_manager::ModelChangeEntry` |  |
| `migrateSessionEntries` | `core::session_manager::migrate_session_entries` |  |
| `NewSessionOptions` | `core::agent_session_runtime::NewSessionOptions` |  |
| `parseSessionEntries` | `core::session_manager::parse_session_entries` |  |
| `SessionContext` | `core::session_manager::SessionContext` |  |
| `SessionEntry` | `core::session_manager::SessionEntry` |  |
| `SessionEntryBase` | — | excluded: structural base interface; Rust entry structs carry `id`/`parentId`/`timestamp` directly. |
| `SessionHeader` | `core::session_manager::SessionHeader` |  |
| `SessionInfo` | `core::session_manager::SessionInfo` |  |
| `SessionInfoEntry` | `core::session_manager::SessionInfoEntry` |  |
| `SessionManager` | `core::session_manager::SessionManager` |  |
| `SessionMessageEntry` | `core::session_manager::SessionMessageEntry` |  |
| `SessionTreeNode` | `core::session_manager::SessionTreeNode` |  |
| `sessionEntryToContextMessages` | `core::session_manager::session_entry_to_context_messages` |  |
| `ThinkingLevelChangeEntry` | `core::session_manager::ThinkingLevelChangeEntry` |  |
| `MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES` | `core::session_tool_output::MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES` |  |
| `migrateToolResultSessionFiles` | `core::session_tool_output::migrate_tool_result_session_files` |  |
| `serializeSessionEntryForPersistence` | `core::session_tool_output::serialize_session_entry_for_persistence` |  |
| `TOOL_RESULT_TRUNCATION_MARKER` | `core::session_tool_output::TOOL_RESULT_TRUNCATION_MARKER` |  |
| `ToolResultSessionMigrationReport` | `core::session_tool_output::ToolResultSessionMigrationReport` |  |
| `truncateToolResultForPersistence` | `core::session_tool_output::truncate_tool_result_for_persistence` |  |
| `CompactionSettings` | `core::settings_manager::CompactionSettings` |  |
| `DefaultProjectTrust` | `core::settings_manager::DefaultProjectTrust` |  |
| `FullscreenExitOutput` | `core::settings_manager::FullscreenExitOutput` |  |
| `ImageSettings` | `core::settings_manager::ImageSettings` |  |
| `PackageSource` | `core::settings_manager::PackageSource` |  |
| `RetrySettings` | `core::settings_manager::RetrySettings` |  |
| `SettingsManager` | `core::settings_manager::SettingsManager` |  |
| `SettingsManagerCreateOptions` | `core::settings_manager::SettingsManagerCreateOptions` |  |
| `TuiMode` | `pi_tui::tui::TuiMode` |  |
| `ParsedSkillBlock` | `core::ParsedSkillBlock` |  |
| `parseSkillBlock` | `core::parse_skill_block` |  |
| `formatSkillsForPrompt` | `core::skills::format_skills_for_prompt` |  |
| `LoadSkillsFromDirOptions` | `core::skills::LoadSkillsFromDirOptions` |  |
| `LoadSkillsResult` | `core::skills::LoadSkillsResult` |  |
| `loadSkills` | `core::skills::load_skills` |  |
| `loadSkillsFromDir` | `core::skills::load_skills_from_dir` |  |
| `Skill` | `core::skills::Skill` |  |
| `SkillFrontmatter` | `core::skills::SkillFrontmatter` |  |
| `createSyntheticSourceInfo` | `core::source_info::create_synthetic_source_info` |  |
| `EditDiffResult` | `core::tools::edit_diff::EditDiffResult` |  |
| `generateDiffString` | `core::tools::edit_diff::generate_diff_string` |  |
| `generateUnifiedPatch` | `core::tools::edit_diff::generate_unified_patch` |  |
| `AskArchitectToolDetails` | `core::tools::ask_architect::AskArchitectToolDetails` |  |
| `AskArchitectToolInput` | `core::tools::ask_architect::AskArchitectToolInput` |  |
| `AskQuestionsToolDetails` | — | type-only export; represented by the tool's JSON schema + `serde_json::Value` |
| `AskQuestionsToolInput` | — | type-only export; represented by the tool's JSON schema + `serde_json::Value` |
| `BashOperations` | `core::tools::bash::BashOperations` |  |
| `BashSpawnContext` | `core::tools::bash::BashSpawnContext` |  |
| `BashSpawnHook` | `core::tools::bash::BashSpawnHook` |  |
| `BashToolDetails` | — | type-only export; represented by the tool's JSON schema + `serde_json::Value` |
| `BashToolInput` | — | type-only export; represented by the tool's JSON schema + `serde_json::Value` |
| `BashToolOptions` | `core::tools::bash::BashToolOptions` |  |
| `CodeIndexCommandResult` | `core::tools::code_index::CodeIndexCommandResult` |  |
| `CodeIndexOperations` | `core::tools::code_index::CodeIndexOperations` |  |
| `CodeIndexToolDetails` | — | type-only export; represented by the tool's JSON schema + `serde_json::Value` |
| `CodeIndexToolOptions` | `core::tools::code_index::CodeIndexToolOptions` |  |
| `createAskArchitectToolDefinition` | `core::tools::ask_architect::create_ask_architect_tool_definition` |  |
| `createAskQuestionsToolDefinition` | `core::tools::ask_questions::create_ask_questions_tool_definition` |  |
| `createBashToolDefinition` | `core::tools::bash::create_bash_tool_definition` |  |
| `createEditToolDefinition` | `core::tools::edit::create_edit_tool_definition` |  |
| `createFindToolDefinition` | `core::tools::find::create_find_tool_definition` |  |
| `createGrepToolDefinition` | `core::tools::grep::create_grep_tool_definition` |  |
| `createLocalBashOperations` | `core::tools::bash::create_local_bash_operations` |  |
| `createLsToolDefinition` | `core::tools::ls::create_ls_tool_definition` |  |
| `createOutlineToolDefinition` | `core::tools::code_index::create_outline_tool_definition` |  |
| `createReadToolDefinition` | `core::tools::read::create_read_tool_definition` |  |
| `createReferencesToolDefinition` | `core::tools::code_index::create_references_tool_definition` |  |
| `createSymbolToolDefinition` | `core::tools::code_index::create_symbol_tool_definition` |  |
| `createWriteToolDefinition` | `core::tools::write::create_write_tool_definition` |  |
| `DEFAULT_MAX_BYTES` | `core::tools::DEFAULT_MAX_BYTES` |  |
| `DEFAULT_MAX_LINES` | `core::tools::DEFAULT_MAX_LINES` |  |
| `EditOperations` | `core::tools::edit::EditOperations` |  |
| `EditToolDetails` | — | type-only export; represented by the tool's JSON schema + `serde_json::Value` |
| `EditToolInput` | — | type-only export; represented by the tool's JSON schema + `serde_json::Value` |
| `EditToolOptions` | `core::tools::edit::EditToolOptions` |  |
| `FindOperations` | `core::tools::find::FindOperations` |  |
| `FindToolDetails` | — | type-only export; represented by the tool's JSON schema + `serde_json::Value` |
| `FindToolInput` | — | type-only export; represented by the tool's JSON schema + `serde_json::Value` |
| `FindToolOptions` | `core::tools::find::FindToolOptions` |  |
| `formatSize` | `core::tools::format_size` |  |
| `GrepOperations` | `core::tools::grep::GrepOperations` |  |
| `GrepToolDetails` | — | type-only export; represented by the tool's JSON schema + `serde_json::Value` |
| `GrepToolInput` | — | type-only export; represented by the tool's JSON schema + `serde_json::Value` |
| `GrepToolOptions` | `core::tools::grep::GrepToolOptions` |  |
| `LsOperations` | `core::tools::ls::LsOperations` |  |
| `LsToolDetails` | — | type-only export; represented by the tool's JSON schema + `serde_json::Value` |
| `LsToolInput` | — | type-only export; represented by the tool's JSON schema + `serde_json::Value` |
| `LsToolOptions` | `core::tools::ls::LsToolOptions` |  |
| `OutlineToolInput` | — | type-only export; represented by the tool's JSON schema + `serde_json::Value` |
| `ReadOperations` | `core::tools::read::ReadOperations` |  |
| `ReadToolDetails` | — | type-only export; represented by the tool's JSON schema + `serde_json::Value` |
| `ReadToolInput` | — | type-only export; represented by the tool's JSON schema + `serde_json::Value` |
| `ReadToolOptions` | `core::tools::read::ReadToolOptions` |  |
| `ReferencesToolInput` | — | type-only export; represented by the tool's JSON schema + `serde_json::Value` |
| `SymbolToolInput` | — | type-only export; represented by the tool's JSON schema + `serde_json::Value` |
| `ToolsOptions` | `core::tools::ToolsOptions` |  |
| `TruncationOptions` | `core::tools::TruncationOptions` |  |
| `TruncationResult` | `core::tools::TruncationResult` |  |
| `truncateHead` | `core::tools::truncate_head` |  |
| `truncateLine` | `core::tools::truncate_line` |  |
| `truncateTail` | `core::tools::truncate_tail` |  |
| `WriteOperations` | `core::tools::write::WriteOperations` |  |
| `WriteToolInput` | — | type-only export; represented by the tool's JSON schema + `serde_json::Value` |
| `WriteToolOptions` | `core::tools::write::WriteToolOptions` |  |
| `withFileMutationQueue` | `core::tools::file_mutation_queue::with_file_mutation_queue` |  |
| `hasTrustRequiringProjectResources` | `core::trust_manager::has_trust_requiring_project_resources` |  |
| `ProjectTrustDecision` | `core::trust_manager::ProjectTrustDecision` |  |
| `ProjectTrustStore` | `core::trust_manager::ProjectTrustStore` |  |
| `ProjectTrustStoreEntry` | `core::trust_manager::ProjectTrustStoreEntry` |  |
| `ProjectTrustUpdate` | `core::trust_manager::ProjectTrustUpdate` |  |
| `MainOptions` | `main_runner::MainOptions` |  |
| `main` | `main_runner::main` |  |
| `InteractiveMode` | `modes::interactive::interactive_mode::InteractiveMode` |  |
| `InteractiveModeOptions` | `modes::interactive::interactive_mode::InteractiveModeOptions` |  |
| `ModelInfo` | — | excluded: annotation only; the client returns the full `Model` (see `rpc-client.md`). |
| `PrintModeOptions` | `modes::print_mode::PrintModeOptions` |  |
| `RpcClient` | `modes::rpc::rpc_client::RpcClient` |  |
| `RpcClientOptions` | `modes::rpc::rpc_client::RpcClientOptions` |  |
| `RpcCommand` | `modes::rpc::rpc_types::RpcCommand` |  |
| `RpcCommandBody` | `modes::rpc::rpc_types::RpcCommandBody` |  |
| `RpcEventListener` | `modes::rpc::rpc_client::RpcEventListener` |  |
| `RpcExtensionUIRequest` | `modes::rpc::rpc_types::RpcExtensionUiRequest` |  |
| `RpcExtensionUIResponse` | `modes::rpc::rpc_types::RpcExtensionUiResponse` |  |
| `RpcResponse` | `modes::rpc::rpc_types::RpcResponse` |  |
| `RpcSessionState` | `modes::rpc::rpc_types::RpcSessionState` |  |
| `runPrintMode` | `modes::print_mode::run_print_mode` |  |
| `runRpcMode` | `modes::rpc::rpc_mode::run_rpc_mode` |  |
| `ArminComponent` | `modes::interactive::components::armin::ArminComponent` |  |
| `AssistantMessageComponent` | `modes::interactive::components::assistant_message::AssistantMessageComponent` |  |
| `BashExecutionComponent` | `modes::interactive::components::bash_execution::BashExecutionComponent` |  |
| `BorderedLoader` | `modes::interactive::components::bordered_loader::BorderedLoader` |  |
| `BranchSummaryMessageComponent` | `modes::interactive::components::branch_summary_message::BranchSummaryMessageComponent` |  |
| `CompactionSummaryMessageComponent` | `modes::interactive::components::compaction_summary_message::CompactionSummaryMessageComponent` |  |
| `CustomEditor` | `modes::interactive::components::custom_editor::CustomEditor` |  |
| `CustomMessageComponent` | `modes::interactive::components::custom_message::CustomMessageComponent` |  |
| `DynamicBorder` | `pi_tui::components::dynamic_border::DynamicBorder` |  |
| `ExtensionEditorComponent` | `modes::interactive::components::extension_editor::ExtensionEditorComponent` |  |
| `ExtensionInputComponent` | `modes::interactive::components::extension_input::ExtensionInputComponent` |  |
| `ExtensionSelectorComponent` | `modes::interactive::components::extension_selector::ExtensionSelectorComponent` |  |
| `FooterComponent` | `modes::interactive::components::footer::FooterComponent` |  |
| `keyHint` | `modes::interactive::components::keybinding_hints::key_hint` |  |
| `keyText` | `modes::interactive::components::keybinding_hints::key_text` |  |
| `LoginDialogComponent` | `modes::interactive::components::login_dialog::LoginDialogComponent` |  |
| `ModelSelectorComponent` | `modes::interactive::components::model_selector::ModelSelectorComponent` |  |
| `OAuthSelectorComponent` | `modes::interactive::components::oauth_selector::OAuthSelectorComponent` |  |
| `RenderDiffOptions` | — | excluded: the only field (`filePath`) is unused; `render_diff(text)` takes the text only. |
| `rawKeyHint` | `modes::interactive::components::keybinding_hints::raw_key_hint` |  |
| `renderDiff` | `modes::interactive::components::diff::render_diff` |  |
| `SessionSelectorComponent` | `modes::interactive::components::session_selector::SessionSelectorComponent` |  |
| `SettingsCallbacks` | `modes::interactive::components::settings_selector::SettingsCallbacks` |  |
| `SettingsConfig` | `modes::interactive::components::settings_selector::SettingsConfig` |  |
| `SettingsSelectorComponent` | `modes::interactive::components::settings_selector::SettingsSelectorComponent` |  |
| `ShowImagesSelectorComponent` | `modes::interactive::components::show_images_selector::ShowImagesSelectorComponent` |  |
| `SkillInvocationMessageComponent` | `modes::interactive::components::skill_invocation_message::SkillInvocationMessageComponent` |  |
| `ThemeSelectorComponent` | `modes::interactive::components::theme_selector::ThemeSelectorComponent` |  |
| `ThinkingSelectorComponent` | `modes::interactive::components::thinking_selector::ThinkingSelectorComponent` |  |
| `ToolExecutionComponent` | `modes::interactive::components::tool_execution::ToolExecutionComponent` |  |
| `ToolExecutionOptions` | `modes::interactive::components::tool_execution::ToolExecutionOptions` |  |
| `TreeSelectorComponent` | `modes::interactive::components::tree_selector::TreeSelectorComponent` |  |
| `truncateToVisualLines` | `modes::interactive::components::visual_truncate::truncate_to_visual_lines` |  |
| `UserMessageComponent` | `modes::interactive::components::user_message::UserMessageComponent` |  |
| `UserMessageSelectorComponent` | `modes::interactive::components::user_message_selector::UserMessageSelectorComponent` |  |
| `VisualTruncateResult` | `modes::interactive::components::visual_truncate::VisualTruncateResult` |  |
| `getLanguageFromPath` | `modes::interactive::theme::get_language_from_path` |  |
| `getMarkdownTheme` | `modes::interactive::theme::get_markdown_theme` |  |
| `getSelectListTheme` | `modes::interactive::theme::get_select_list_theme` |  |
| `getSettingsListTheme` | `modes::interactive::theme::get_settings_list_theme` |  |
| `highlightCode` | `modes::interactive::theme::highlight_code` |  |
| `initTheme` | `modes::interactive::theme::init_theme` |  |
| `Theme` | `modes::interactive::theme::Theme` |  |
| `ThemeColor` | `modes::interactive::theme::ThemeColor` |  |
| `copyToClipboard` | `utils::clipboard::copy_to_clipboard` |  |
| `parseFrontmatter` | `utils::frontmatter::parse_frontmatter` |  |
| `stripFrontmatter` | `utils::frontmatter::strip_frontmatter` |  |
| `convertToPng` | `utils::image_convert::convert_to_png` |  |
| `formatDimensionNote` | `utils::image_resize::format_dimension_note` |  |
| `ResizedImage` | `utils::image_resize_core::ResizedImage` |  |
| `resizeImage` | `utils::image_resize::resize_image` |  |
| `getShellConfig` | `utils::shell::get_shell_config` |  |

## Exclusions

Listed in the table above. They are TypeScript conditional/structural types,
per-tool event aliases, or type-only tool records whose runtime contract is
the tool's JSON schema plus `serde_json::Value` (per the ca-tools-finish API).
