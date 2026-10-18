//! Extension runner (`core/extensions/runner.ts`).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use indexmap::IndexMap;
use pi_agent_core::harness::types::ThinkingLevel;
use pi_ai::types::{ImageContent, Model, ProviderHeaders};
use pi_tui::keybindings::{KeyId, KeybindingsConfig};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::core::diagnostics::{ResourceDiagnostic, ResourceDiagnosticType};
use crate::core::footer_data_provider::FooterDataProvider;
use crate::core::model_registry::ModelRegistry;
use crate::core::model_resolver::ScopedModel;
use crate::core::session_manager::SessionManager;
use crate::core::settings_manager::SettingsManager;
use crate::core::source_info::{SourceInfo, SourceOrigin, SourceScope};
use crate::core::system_prompt::BuildSystemPromptOptions;

use super::extension_runtime::ExtensionRuntime;
use super::types::*;

const RESERVED_KEYBINDINGS_FOR_EXTENSION_CONFLICTS: &[&str] = &[
    "app.interrupt",
    "app.clear",
    "app.exit",
    "app.suspend",
    "app.thinking.cycle",
    "app.model.cycleForward",
    "app.model.cycleBackward",
    "app.model.select",
    "app.tools.expand",
    "app.thinking.toggle",
    "app.editor.external",
    "app.message.followUp",
    "tui.input.submit",
    "tui.select.confirm",
    "tui.select.cancel",
    "tui.input.copy",
    "tui.editor.deleteToLineEnd",
];

#[derive(Debug, Clone, PartialEq, Eq)]
struct BuiltInKeyBinding {
    keybinding: String,
    restrict_override: bool,
}

fn build_builtin_keybindings(resolved: &KeybindingsConfig) -> IndexMap<KeyId, BuiltInKeyBinding> {
    let mut builtin = IndexMap::new();
    for (keybinding, keys) in resolved {
        let restrict_override = RESERVED_KEYBINDINGS_FOR_EXTENSION_CONFLICTS.contains(&keybinding.as_str());
        for key in keys {
            let normalized = key.to_lowercase();
            if builtin
                .get(&normalized)
                .is_some_and(|existing: &BuiltInKeyBinding| existing.restrict_override && !restrict_override)
            {
                continue;
            }
            builtin.insert(
                normalized,
                BuiltInKeyBinding {
                    keybinding: keybinding.clone(),
                    restrict_override,
                },
            );
        }
    }
    builtin
}

pub type ExtensionErrorListener = Arc<dyn Fn(&ExtensionError) + Send + Sync>;

/// Unsubscribe handle for [`ExtensionRunner::on_error`].
pub struct ErrorSubscription {
    listeners: Arc<Mutex<Vec<(u64, ExtensionErrorListener)>>>,
    id: u64,
}

impl ErrorSubscription {
    pub fn unsubscribe(self) {
        self.listeners
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .retain(|(id, _)| *id != self.id);
    }
}

/// Combined result from all `before_agent_start` handlers.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BeforeAgentStartCombinedResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub messages: Option<Vec<CustomMessagePayload>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
}

/// Path returned by `resources_discover` together with its extension.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredPath {
    pub path: String,
    pub extension_path: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourcesDiscoverPaths {
    pub skill_paths: Vec<DiscoveredPath>,
    pub prompt_paths: Vec<DiscoveredPath>,
    pub theme_paths: Vec<DiscoveredPath>,
}

/// Result of [`ExtensionRunner::emit`].
#[derive(Debug, Clone)]
pub enum EmitResult {
    None,
    SessionBeforeSwitch(SessionBeforeSwitchResult),
    SessionBeforeFork(SessionBeforeForkResult),
    SessionBeforeCompact(SessionBeforeCompactResult),
    Compaction(CompactionEventResult),
    SessionBeforeTree(SessionBeforeTreeResult),
    SessionCompactionSource(SessionCompactionSourceResult),
}

impl EmitResult {
    pub fn session_before_switch(&self) -> Option<&SessionBeforeSwitchResult> {
        match self {
            Self::SessionBeforeSwitch(result) => Some(result),
            _ => None,
        }
    }

    pub fn session_before_fork(&self) -> Option<&SessionBeforeForkResult> {
        match self {
            Self::SessionBeforeFork(result) => Some(result),
            _ => None,
        }
    }

    pub fn session_before_compact(&self) -> Option<&SessionBeforeCompactResult> {
        match self {
            Self::SessionBeforeCompact(result) => Some(result),
            _ => None,
        }
    }

    pub fn compaction(&self) -> Option<&CompactionEventResult> {
        match self {
            Self::Compaction(result) => Some(result),
            _ => None,
        }
    }

    pub fn session_before_tree(&self) -> Option<&SessionBeforeTreeResult> {
        match self {
            Self::SessionBeforeTree(result) => Some(result),
            _ => None,
        }
    }

    pub fn session_compaction_source(&self) -> Option<&SessionCompactionSourceResult> {
        match self {
            Self::SessionCompactionSource(result) => Some(result),
            _ => None,
        }
    }
}

/// Emit `session_shutdown`; returns whether any handler was registered.
pub async fn emit_session_shutdown_event(runner: &ExtensionRunner, event: SessionShutdownEvent) -> bool {
    if runner.has_handlers("session_shutdown") {
        runner.emit(&mut ExtensionEvent::SessionShutdown(event)).await;
        true
    } else {
        false
    }
}

/// Emit `project_trust`; the first yes/no decision wins and errors are collected.
pub async fn emit_project_trust_event(
    extensions_result: &LoadExtensionsResult,
    event: ProjectTrustEvent,
    ctx: &ProjectTrustContext,
) -> (Option<ProjectTrustEventResult>, Vec<ExtensionError>) {
    let mut errors = Vec::new();
    for extension in &extensions_result.extensions {
        for handler in &extension.handlers.project_trust {
            match handler.call(event.clone(), ctx.clone()).await {
                Ok(result) if result.trusted == ProjectTrustEventDecision::Undecided => {}
                Ok(result) => return (Some(result), errors),
                Err(error) => errors.push(ExtensionError {
                    extension_path: extension.path.clone(),
                    event: event_type_name("project_trust"),
                    error: error.message,
                    stack: error.stack,
                }),
            }
        }
    }
    (None, errors)
}

fn event_type_name(name: &str) -> String {
    name.to_owned()
}

/// Executes extensions and manages their lifecycle.
#[derive(Clone)]
pub struct ExtensionRunner {
    extensions: Vec<Extension>,
    runtime: ExtensionRuntime,
    ui_context: Arc<dyn ExtensionUiContext>,
    has_ui: bool,
    mode: ExtensionMode,
    cwd: String,
    session_manager: Arc<SessionManager>,
    model_registry: Arc<ModelRegistry>,
    settings_manager: Option<Arc<Mutex<SettingsManager>>>,
    error_listeners: Arc<Mutex<Vec<(u64, ExtensionErrorListener)>>>,
    error_listener_id: Arc<AtomicU64>,
    get_model: Arc<dyn Fn() -> Option<Model> + Send + Sync>,
    get_thinking_level_fn: Arc<dyn Fn() -> ThinkingLevel + Send + Sync>,
    get_scoped_models: Arc<dyn Fn() -> Vec<ScopedModel> + Send + Sync>,
    get_footer_data: Arc<dyn Fn() -> Option<Arc<FooterDataProvider>> + Send + Sync>,
    is_idle_fn: Arc<dyn Fn() -> bool + Send + Sync>,
    has_active_retry_fn: Arc<dyn Fn() -> bool + Send + Sync>,
    is_project_trusted_fn: Arc<dyn Fn() -> bool + Send + Sync>,
    get_signal_fn: Arc<dyn Fn() -> Option<CancellationToken> + Send + Sync>,
    abort_fn: Arc<dyn Fn() + Send + Sync>,
    has_pending_messages_fn: Arc<dyn Fn() -> bool + Send + Sync>,
    shutdown_handler: Arc<dyn Fn() + Send + Sync>,
    restart_fn: Arc<dyn Fn(Option<RestartOptions>) -> BoxFuture<'static, ()> + Send + Sync>,
    get_control_db_path_fn: Arc<dyn Fn() -> Option<String> + Send + Sync>,
    get_context_usage_fn: Arc<dyn Fn() -> Option<ContextUsage> + Send + Sync>,
    get_scoped_models_fn: Arc<dyn Fn() -> Vec<ScopedModel> + Send + Sync>,
    get_multi_agent_agent_id_fn: Arc<dyn Fn() -> Option<String> + Send + Sync>,
    get_multi_agent_parent_session_id_fn: Arc<dyn Fn() -> Option<String> + Send + Sync>,
    get_multi_agent_requires_agent_id_fn: Arc<dyn Fn() -> bool + Send + Sync>,
    get_multi_agent_store_fn:
        Arc<dyn Fn() -> Option<Arc<crate::core::multi_agent_store::MultiAgentStore>> + Send + Sync>,
    get_tool_detach_registry_fn:
        Arc<dyn Fn() -> Option<Arc<crate::core::tool_detach_registry::ToolDetachRegistry>> + Send + Sync>,
    compact_fn: Arc<dyn Fn(Option<CompactOptions>) + Send + Sync>,
    get_system_prompt_fn: Arc<dyn Fn() -> String + Send + Sync>,
    get_system_prompt_options_fn: Arc<dyn Fn() -> BuildSystemPromptOptions + Send + Sync>,
    relocate_handler: RelocateCallback,
    prepare_tool_result_relocation_handler: RelocateCallback,
    activate_tool_result_relocation_handler: Arc<dyn Fn() -> BoxFuture<'static, ()> + Send + Sync>,
    pending_tool_result_relocations: Arc<Mutex<IndexMap<String, String>>>,
    is_tool_result_relocation_prepared: Arc<AtomicBool>,
    show_approval_selector_handler: Arc<dyn Fn() -> Result<(), HandlerError> + Send + Sync>,
    show_sandbox_selector_handler: Arc<dyn Fn() -> Result<(), HandlerError> + Send + Sync>,
    wait_for_idle_fn: Arc<dyn Fn() -> BoxFuture<'static, ()> + Send + Sync>,
    new_session_handler: NewSessionCallback,
    fork_handler: ForkCallback,
    navigate_tree_handler: NavigateTreeCallback,
    switch_session_handler: SwitchSessionCallback,
    reload_handler: Arc<dyn Fn() -> BoxFuture<'static, ()> + Send + Sync>,
    shortcut_diagnostics: Arc<Mutex<Vec<ResourceDiagnostic>>>,
    command_diagnostics: Arc<Mutex<Vec<ResourceDiagnostic>>>,
    stale_message: Arc<Mutex<Option<String>>>,
    internal_commands: IndexMap<String, InternalCommandHandler>,
}

impl ExtensionRunner {
    pub fn new(
        extensions: Vec<Extension>,
        runtime: ExtensionRuntime,
        cwd: impl Into<String>,
        session_manager: Arc<SessionManager>,
        model_registry: Arc<ModelRegistry>,
        settings_manager: Option<Arc<Mutex<SettingsManager>>>,
    ) -> Self {
        let cwd = cwd.into();
        let default_cwd = cwd.clone();
        Self {
            extensions,
            runtime,
            ui_context: Arc::new(NoOpUiContext),
            has_ui: false,
            mode: ExtensionMode::Print,
            cwd,
            session_manager,
            model_registry,
            settings_manager,
            error_listeners: Arc::new(Mutex::new(Vec::new())),
            error_listener_id: Arc::new(AtomicU64::new(0)),
            get_model: Arc::new(|| None),
            get_thinking_level_fn: Arc::new(|| ThinkingLevel::Off),
            get_scoped_models: Arc::new(Vec::new),
            get_footer_data: Arc::new(|| None),
            is_idle_fn: Arc::new(|| true),
            has_active_retry_fn: Arc::new(|| false),
            is_project_trusted_fn: Arc::new(|| true),
            get_signal_fn: Arc::new(|| None),
            abort_fn: Arc::new(|| {}),
            has_pending_messages_fn: Arc::new(|| false),
            shutdown_handler: Arc::new(|| {}),
            restart_fn: Arc::new(|_| Box::pin(async {})),
            get_control_db_path_fn: Arc::new(|| None),
            get_context_usage_fn: Arc::new(|| None),
            get_scoped_models_fn: Arc::new(Vec::new),
            get_multi_agent_agent_id_fn: Arc::new(|| None),
            get_multi_agent_parent_session_id_fn: Arc::new(|| None),
            get_multi_agent_requires_agent_id_fn: Arc::new(|| false),
            get_multi_agent_store_fn: Arc::new(|| None),
            get_tool_detach_registry_fn: Arc::new(|| None),
            compact_fn: Arc::new(|_| {}),
            get_system_prompt_fn: Arc::new(String::new),
            get_system_prompt_options_fn: {
                let default_cwd = default_cwd.clone();
                Arc::new(move || BuildSystemPromptOptions {
                    cwd: default_cwd.clone(),
                    ..BuildSystemPromptOptions::default()
                })
            },
            relocate_handler: Arc::new(|_| {
                Box::pin(async {
                    Err(HandlerError::new(
                        "Working-directory changes are not available in this session mode",
                    ))
                })
            }),
            prepare_tool_result_relocation_handler: Arc::new(|_| {
                Box::pin(async {
                    Err(HandlerError::new(
                        "Tool-result working-directory changes are not available in this session mode",
                    ))
                })
            }),
            activate_tool_result_relocation_handler: Arc::new(|| Box::pin(async {})),
            pending_tool_result_relocations: Arc::new(Mutex::new(IndexMap::new())),
            is_tool_result_relocation_prepared: Arc::new(AtomicBool::new(false)),
            show_approval_selector_handler: Arc::new(|| Ok(())),
            show_sandbox_selector_handler: Arc::new(|| Ok(())),
            wait_for_idle_fn: Arc::new(|| Box::pin(async {})),
            new_session_handler: Arc::new(|_| Box::pin(async { Ok(SessionTransitionResult { cancelled: false }) })),
            fork_handler: Arc::new(|_, _| Box::pin(async { Ok(SessionTransitionResult { cancelled: false }) })),
            navigate_tree_handler: Arc::new(|_, _| {
                Box::pin(async { Ok(SessionTransitionResult { cancelled: false }) })
            }),
            switch_session_handler: Arc::new(|_, _| {
                Box::pin(async {
                    Err(HandlerError::new(
                        "Session resume is not available in this session mode",
                    ))
                })
            }),
            reload_handler: Arc::new(|| Box::pin(async {})),
            shortcut_diagnostics: Arc::new(Mutex::new(Vec::new())),
            command_diagnostics: Arc::new(Mutex::new(Vec::new())),
            stale_message: Arc::new(Mutex::new(None)),
            internal_commands: IndexMap::new(),
        }
    }

    pub fn set_internal_commands(&mut self, commands: IndexMap<String, InternalCommandHandler>) {
        self.internal_commands = commands;
    }

    pub fn bind_core(
        &mut self,
        actions: ExtensionActions,
        context_actions: ExtensionContextActions,
        provider_actions: Option<ProviderActions>,
    ) {
        self.bind_runtime_actions(actions);
        self.bind_context_actions(context_actions);
        self.flush_pending_provider_registrations(provider_actions.as_ref());
        self.bind_immediate_provider_actions(provider_actions);
    }

    fn bind_runtime_actions(&self, actions: ExtensionActions) {
        self.runtime.set_actions(actions);
    }

    fn bind_context_actions(&mut self, actions: ExtensionContextActions) {
        self.get_model = actions.get_model;
        self.get_thinking_level_fn = actions
            .get_thinking_level
            .unwrap_or_else(|| Arc::new(|| ThinkingLevel::Off));
        self.get_footer_data = actions.get_footer_data.unwrap_or_else(|| Arc::new(|| None));
        self.get_scoped_models = actions.get_scoped_models.clone().unwrap_or_else(|| Arc::new(Vec::new));
        self.is_idle_fn = actions.is_idle;
        self.has_active_retry_fn = actions.has_active_retry;
        self.is_project_trusted_fn = actions.is_project_trusted;
        self.get_signal_fn = actions.get_signal;
        self.abort_fn = actions.abort;
        self.has_pending_messages_fn = actions.has_pending_messages;
        self.shutdown_handler = actions.shutdown;
        self.restart_fn = actions.restart;
        self.get_control_db_path_fn = actions.get_control_db_path.unwrap_or_else(|| Arc::new(|| None));
        self.get_context_usage_fn = actions.get_context_usage;
        self.get_scoped_models_fn = actions.get_scoped_models.unwrap_or_else(|| Arc::new(Vec::new));
        self.get_multi_agent_agent_id_fn = actions.get_multi_agent_agent_id.unwrap_or_else(|| Arc::new(|| None));
        self.get_multi_agent_parent_session_id_fn = actions
            .get_multi_agent_parent_session_id
            .unwrap_or_else(|| Arc::new(|| None));
        self.get_multi_agent_requires_agent_id_fn = actions
            .get_multi_agent_requires_agent_id
            .unwrap_or_else(|| Arc::new(|| false));
        self.get_multi_agent_store_fn = actions.get_multi_agent_store.unwrap_or_else(|| Arc::new(|| None));
        self.get_tool_detach_registry_fn = actions.get_tool_detach_registry.unwrap_or_else(|| Arc::new(|| None));
        self.compact_fn = actions.compact;
        self.get_system_prompt_fn = actions.get_system_prompt;
        let default_cwd = self.cwd.clone();
        self.get_system_prompt_options_fn = actions.get_system_prompt_options.unwrap_or_else(|| {
            Arc::new(move || BuildSystemPromptOptions {
                cwd: default_cwd.clone(),
                ..BuildSystemPromptOptions::default()
            })
        });
    }

    fn flush_pending_provider_registrations(&self, provider_actions: Option<&ProviderActions>) {
        for registration in self.runtime.pending_provider_registrations() {
            if let Err(error) = self.register_provider(provider_actions, &registration.name, &registration.config) {
                self.report_provider_registration_error(&registration.extension_path, &error);
            }
        }
        for registration in self.runtime.pending_native_provider_registrations() {
            if let Err(error) = self.register_native_provider(provider_actions, registration.provider) {
                self.report_provider_registration_error(&registration.extension_path, &error);
            }
        }
        self.runtime.clear_pending_provider_registrations();
    }

    fn bind_immediate_provider_actions(&self, provider_actions: Option<ProviderActions>) {
        let register_runner = self.clone();
        let register_actions = provider_actions.clone();
        let native_runner = self.clone();
        let native_actions = provider_actions.clone();
        let unregister_runner = self.clone();
        let unregister_actions = provider_actions;
        self.runtime
            .set_provider_handlers(super::extension_runtime::ProviderHandlers {
                register_provider: Arc::new(move |name, config, _extension_path| {
                    register_runner.register_provider(register_actions.as_ref(), name, config)
                }),
                register_native_provider: Arc::new(move |provider, _extension_path| {
                    native_runner.register_native_provider(native_actions.as_ref(), provider)
                }),
                unregister_provider: Arc::new(move |name, _extension_path| {
                    unregister_runner.unregister_provider(unregister_actions.as_ref(), name)
                }),
            });
    }

    fn register_provider(
        &self,
        provider_actions: Option<&ProviderActions>,
        name: &str,
        config: &ProviderConfig,
    ) -> Result<(), String> {
        if let Some(register) = provider_actions.and_then(|actions| actions.register_provider.as_ref()) {
            register(name, config)?;
            return Ok(());
        }
        self.model_registry
            .register_provider(name, config.clone())
            .map_err(|error| error.to_string())
    }

    fn register_native_provider(
        &self,
        provider_actions: Option<&ProviderActions>,
        provider: NativeProvider,
    ) -> Result<(), String> {
        let Some(register) = provider_actions.and_then(|actions| actions.register_native_provider.as_ref()) else {
            return Err("Native provider registration is unavailable in this session mode.".to_owned());
        };
        register(provider);
        Ok(())
    }

    fn unregister_provider(&self, provider_actions: Option<&ProviderActions>, name: &str) -> Result<(), String> {
        if let Some(unregister) = provider_actions.and_then(|actions| actions.unregister_provider.as_ref()) {
            unregister(name);
            return Ok(());
        }
        self.model_registry.unregister_provider(name);
        Ok(())
    }

    fn report_provider_registration_error(&self, extension_path: &str, error: &str) {
        self.emit_error(ExtensionError {
            extension_path: extension_path.to_owned(),
            event: "register_provider".to_owned(),
            error: error.to_owned(),
            stack: None,
        });
    }

    pub fn set_relocate_handler(&mut self, handler: RelocateCallback) {
        self.relocate_handler = handler;
    }

    pub fn set_tool_result_relocation_handlers(
        &mut self,
        prepare: RelocateCallback,
        activate: Arc<dyn Fn() -> BoxFuture<'static, ()> + Send + Sync>,
    ) {
        self.prepare_tool_result_relocation_handler = prepare;
        self.activate_tool_result_relocation_handler = activate;
    }

    fn schedule_tool_result_relocation(&self, tool_call_id: &str, target_cwd: &str) -> Result<(), HandlerError> {
        self.assert_active()?;
        let mut pending = self
            .pending_tool_result_relocations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if pending.contains_key(tool_call_id) {
            return Err(HandlerError::new(format!(
                "Working-directory relocation already scheduled for tool call {tool_call_id}"
            )));
        }
        pending.insert(tool_call_id.to_owned(), target_cwd.to_owned());
        Ok(())
    }

    pub async fn deliver_tool_result_relocation(&self, tool_call_id: &str) -> Result<(), HandlerError> {
        let target_cwd = self
            .pending_tool_result_relocations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .shift_remove(tool_call_id);
        let Some(target_cwd) = target_cwd else {
            return Ok(());
        };
        (self.prepare_tool_result_relocation_handler)(&target_cwd).await?;
        self.is_tool_result_relocation_prepared.store(true, Ordering::SeqCst);
        Ok(())
    }

    pub fn has_prepared_tool_result_relocation(&self) -> bool {
        self.is_tool_result_relocation_prepared.load(Ordering::SeqCst)
    }

    pub async fn activate_tool_result_relocation(&self) {
        if !self.is_tool_result_relocation_prepared.swap(false, Ordering::SeqCst) {
            return;
        }
        (self.activate_tool_result_relocation_handler)().await;
    }

    pub fn bind_command_context(&mut self, actions: Option<ExtensionCommandContextActions>) {
        let Some(actions) = actions else {
            self.show_approval_selector_handler = Arc::new(|| Ok(()));
            self.show_sandbox_selector_handler = Arc::new(|| Ok(()));
            self.wait_for_idle_fn = Arc::new(|| Box::pin(async {}));
            self.new_session_handler =
                Arc::new(|_| Box::pin(async { Ok(SessionTransitionResult { cancelled: false }) }));
            self.fork_handler = Arc::new(|_, _| Box::pin(async { Ok(SessionTransitionResult { cancelled: false }) }));
            self.navigate_tree_handler =
                Arc::new(|_, _| Box::pin(async { Ok(SessionTransitionResult { cancelled: false }) }));
            self.switch_session_handler = Arc::new(|_, _| {
                Box::pin(async {
                    Err(HandlerError::new(
                        "Session resume is not available in this session mode",
                    ))
                })
            });
            self.reload_handler = Arc::new(|| Box::pin(async {}));
            self.restart_fn = Arc::new(|_| Box::pin(async {}));
            return;
        };
        self.show_approval_selector_handler = actions.show_approval_selector;
        self.show_sandbox_selector_handler = actions.show_sandbox_selector;
        self.wait_for_idle_fn = actions.wait_for_idle;
        self.new_session_handler = actions.new_session;
        self.fork_handler = actions.fork;
        self.navigate_tree_handler = actions.navigate_tree;
        self.switch_session_handler = actions.switch_session;
        self.reload_handler = actions.reload;
        self.restart_fn = actions.restart;
    }

    pub fn set_ui_context(&mut self, ui_context: Option<Arc<dyn ExtensionUiContext>>, mode: ExtensionMode) {
        match ui_context {
            Some(ui_context) => {
                self.ui_context = ui_context;
                self.has_ui = true;
            }
            None => {
                self.ui_context = Arc::new(NoOpUiContext);
                self.has_ui = false;
            }
        }
        self.mode = mode;
    }

    pub fn get_ui_context(&self) -> Arc<dyn ExtensionUiContext> {
        Arc::clone(&self.ui_context)
    }

    pub fn has_ui(&self) -> bool {
        self.has_ui
    }

    pub fn get_extension_paths(&self) -> Vec<String> {
        self.extensions.iter().map(|extension| extension.path.clone()).collect()
    }

    /// All registered tools; the first registration per name wins.
    pub fn get_all_registered_tools(&self) -> Vec<RegisteredTool> {
        let mut tools_by_name: IndexMap<String, RegisteredTool> = IndexMap::new();
        for extension in &self.extensions {
            for tool in extension.tools.values() {
                if !tools_by_name.contains_key(&tool.definition.name) {
                    tools_by_name.insert(tool.definition.name.clone(), tool.clone());
                }
            }
        }
        tools_by_name.into_values().collect()
    }

    pub fn get_tool_definition(&self, tool_name: &str) -> Option<ToolDefinition> {
        for extension in &self.extensions {
            if let Some(tool) = extension.tools.get(tool_name) {
                return Some(tool.definition.clone());
            }
        }
        None
    }

    pub fn get_flags(&self) -> IndexMap<String, ExtensionFlag> {
        let mut all_flags = IndexMap::new();
        for extension in &self.extensions {
            for (name, flag) in &extension.flags {
                if !all_flags.contains_key(name) {
                    all_flags.insert(name.clone(), flag.clone());
                }
            }
        }
        all_flags
    }

    pub fn set_flag_value(&self, name: &str, value: FlagValue) {
        self.runtime.set_flag_value(name, value);
    }

    pub fn get_flag_values(&self) -> IndexMap<String, FlagValue> {
        self.runtime.flag_values()
    }

    pub fn get_shortcuts(&self, resolved_keybindings: &KeybindingsConfig) -> IndexMap<KeyId, ExtensionShortcut> {
        *self
            .shortcut_diagnostics
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Vec::new();
        let builtin = build_builtin_keybindings(resolved_keybindings);
        let mut extension_shortcuts: IndexMap<KeyId, ExtensionShortcut> = IndexMap::new();

        let add_diagnostic = |message: String, extension_path: &str| {
            self.shortcut_diagnostics
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(ResourceDiagnostic {
                    diagnostic_type: ResourceDiagnosticType::Warning,
                    message: message.clone(),
                    path: Some(extension_path.to_owned()),
                    collision: None,
                });
            if !self.has_ui() {
                eprintln!("{message}");
            }
        };

        for extension in &self.extensions {
            for (key, shortcut) in &extension.shortcuts {
                let normalized = key.to_lowercase();
                if let Some(built_in) = builtin.get(&normalized) {
                    if built_in.restrict_override {
                        add_diagnostic(
                            format!(
                                "Extension shortcut '{key}' from {} conflicts with built-in shortcut. Skipping.",
                                shortcut.extension_path
                            ),
                            &shortcut.extension_path,
                        );
                        continue;
                    }
                    add_diagnostic(
                        format!(
                            "Extension shortcut conflict: '{key}' is built-in shortcut for {} and {}. Using {}.",
                            built_in.keybinding, shortcut.extension_path, shortcut.extension_path
                        ),
                        &shortcut.extension_path,
                    );
                }

                if let Some(existing) = extension_shortcuts.get(&normalized) {
                    add_diagnostic(
                        format!(
                            "Extension shortcut conflict: '{key}' registered by both {} and {}. Using {}.",
                            existing.extension_path, shortcut.extension_path, shortcut.extension_path
                        ),
                        &shortcut.extension_path,
                    );
                }
                extension_shortcuts.insert(normalized, shortcut.clone());
            }
        }
        extension_shortcuts
    }

    pub fn get_shortcut_diagnostics(&self) -> Vec<ResourceDiagnostic> {
        self.shortcut_diagnostics
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    pub fn invalidate(&self, message: Option<&str>) {
        let mut stale = self
            .stale_message
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if stale.is_some() {
            return;
        }
        let message = message.map_or_else(
            || {
                "This extension ctx is stale after session replacement or reload. Do not use a captured pi or command ctx after ctx.newSession(), ctx.fork(), ctx.switchSession(), or ctx.reload(). For newSession, fork, and switchSession, move post-replacement work into withSession and use the ctx passed to withSession. For reload, do not use the old ctx after await ctx.reload().".to_owned()
            },
            str::to_owned,
        );
        *stale = Some(message.clone());
        drop(stale);
        self.runtime.invalidate(Some(&message));
        self.pending_tool_result_relocations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
    }

    pub fn assert_active(&self) -> Result<(), HandlerError> {
        match self
            .stale_message
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
        {
            Some(message) => Err(HandlerError::new(message.clone())),
            None => Ok(()),
        }
    }

    pub fn on_error(&self, listener: ExtensionErrorListener) -> ErrorSubscription {
        let id = self.error_listener_id.fetch_add(1, Ordering::SeqCst) + 1;
        self.error_listeners
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push((id, listener));
        ErrorSubscription {
            listeners: Arc::clone(&self.error_listeners),
            id,
        }
    }

    pub fn emit_error(&self, error: ExtensionError) {
        let listeners: Vec<ExtensionErrorListener> = self
            .error_listeners
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .map(|(_, listener)| Arc::clone(listener))
            .collect();
        for listener in listeners {
            listener(&error);
        }
    }

    pub fn has_handlers(&self, event_type: &str) -> bool {
        self.extensions.iter().any(|extension| {
            let handlers = &extension.handlers;
            match event_type {
                "project_trust" => !handlers.project_trust.is_empty(),
                "resources_discover" => !handlers.resources_discover.is_empty(),
                "session_start" => !handlers.session_start.is_empty(),
                "session_info_changed" => !handlers.session_info_changed.is_empty(),
                "session_before_switch" => !handlers.session_before_switch.is_empty(),
                "session_before_fork" => !handlers.session_before_fork.is_empty(),
                "session_before_compact" => !handlers.session_before_compact.is_empty(),
                "compaction" => !handlers.compaction.is_empty(),
                "session_compaction_source" => !handlers.session_compaction_source.is_empty(),
                "session_compact" => !handlers.session_compact.is_empty(),
                "session_shutdown" => !handlers.session_shutdown.is_empty(),
                "runtime_mailbox" => !handlers.runtime_mailbox.is_empty(),
                "session_before_tree" => !handlers.session_before_tree.is_empty(),
                "session_tree" => !handlers.session_tree.is_empty(),
                "context" => !handlers.context.is_empty(),
                "before_provider_request" => !handlers.before_provider_request.is_empty(),
                "before_provider_headers" => !handlers.before_provider_headers.is_empty(),
                "after_provider_response" => !handlers.after_provider_response.is_empty(),
                "before_agent_start" => !handlers.before_agent_start.is_empty(),
                "agent_start" => !handlers.agent_start.is_empty(),
                "agent_end" => !handlers.agent_end.is_empty(),
                "agent_settled" => !handlers.agent_settled.is_empty(),
                "turn_start" => !handlers.turn_start.is_empty(),
                "turn_end" => !handlers.turn_end.is_empty(),
                "message_start" => !handlers.message_start.is_empty(),
                "message_update" => !handlers.message_update.is_empty(),
                "message_end" => !handlers.message_end.is_empty(),
                "tool_execution_start" => !handlers.tool_execution_start.is_empty(),
                "tool_execution_update" => !handlers.tool_execution_update.is_empty(),
                "tool_execution_end" => !handlers.tool_execution_end.is_empty(),
                "model_select" => !handlers.model_select.is_empty(),
                "thinking_level_select" => !handlers.thinking_level_select.is_empty(),
                "tool_call" => !handlers.tool_call.is_empty(),
                "tool_result" => !handlers.tool_result.is_empty(),
                "user_bash" => !handlers.user_bash.is_empty(),
                "input" => !handlers.input.is_empty(),
                _ => false,
            }
        })
    }

    pub fn has_approval_reviewers(&self) -> bool {
        self.extensions
            .iter()
            .any(|extension| !extension.approval_reviewers.is_empty())
    }

    pub fn has_tool_gates(&self) -> bool {
        self.extensions.iter().any(|extension| !extension.tool_gates.is_empty())
    }

    pub fn get_message_renderer(&self, custom_type: &str) -> Option<MessageRenderer> {
        for extension in &self.extensions {
            if let Some(renderer) = extension.message_renderers.get(custom_type) {
                return Some(Arc::clone(renderer));
            }
        }
        None
    }

    pub fn get_markdown_transformers(&self) -> Vec<MarkdownTransformer> {
        self.extensions
            .iter()
            .filter_map(|extension| extension.markdown_transformer.clone())
            .collect()
    }

    pub fn get_entry_renderer(&self, custom_type: &str) -> Option<EntryRenderer> {
        for extension in &self.extensions {
            if let Some(renderer) = extension
                .entry_renderers
                .as_ref()
                .and_then(|renderers| renderers.get(custom_type))
            {
                return Some(Arc::clone(renderer));
            }
        }
        None
    }

    fn resolve_registered_commands(&self) -> Vec<ResolvedCommand> {
        let mut commands: Vec<RegisteredCommand> = Vec::new();
        let mut counts: IndexMap<String, usize> = IndexMap::new();
        for extension in &self.extensions {
            for command in extension.commands.values() {
                commands.push(command.clone());
                *counts.entry(command.name.clone()).or_insert(0) += 1;
            }
        }

        let mut seen: IndexMap<String, usize> = IndexMap::new();
        let mut taken_invocation_names: Vec<String> = Vec::new();

        commands
            .into_iter()
            .map(|command| {
                let occurrence = seen.get(&command.name).copied().unwrap_or(0) + 1;
                seen.insert(command.name.clone(), occurrence);

                let mut invocation_name = if counts.get(&command.name).copied().unwrap_or(0) > 1 {
                    format!("{}:{occurrence}", command.name)
                } else {
                    command.name.clone()
                };

                if taken_invocation_names.contains(&invocation_name) {
                    let mut suffix = occurrence;
                    loop {
                        suffix += 1;
                        invocation_name = format!("{}:{suffix}", command.name);
                        if !taken_invocation_names.contains(&invocation_name) {
                            break;
                        }
                    }
                }

                taken_invocation_names.push(invocation_name.clone());
                ResolvedCommand {
                    name: command.name,
                    source_info: command.source_info,
                    description: command.description,
                    get_argument_completions: command.get_argument_completions,
                    handler: command.handler,
                    invocation_name,
                }
            })
            .collect()
    }

    pub fn get_registered_commands(&self) -> Vec<ResolvedCommand> {
        *self
            .command_diagnostics
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Vec::new();
        self.resolve_registered_commands()
    }

    pub fn get_command_diagnostics(&self) -> Vec<ResourceDiagnostic> {
        self.command_diagnostics
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    pub fn get_command(&self, name: &str) -> Option<ResolvedCommand> {
        self.resolve_registered_commands()
            .into_iter()
            .find(|command| command.invocation_name == name)
    }

    pub fn get_prompt_command(&self, name: &str) -> Option<ResolvedCommand> {
        if let Some(command) = self.get_command(name) {
            return Some(command);
        }
        let handler = self.internal_commands.get(name)?;
        Some(ResolvedCommand {
            name: name.to_owned(),
            source_info: SourceInfo {
                path: "<internal>".to_owned(),
                source: "internal".to_owned(),
                scope: SourceScope::Temporary,
                origin: SourceOrigin::TopLevel,
                base_dir: None,
            },
            description: None,
            get_argument_completions: None,
            handler: Arc::clone(handler),
            invocation_name: name.to_owned(),
        })
    }

    pub fn shutdown(&self) {
        (self.shutdown_handler)();
    }

    pub fn get_active_tools(&self) -> Result<Vec<String>, HandlerError> {
        self.assert_active()?;
        (self.runtime.actions().get_active_tools)()
    }

    fn context_callbacks(&self) -> Arc<ExtensionContextCallbacks> {
        let runner = self.clone();
        Arc::new(ExtensionContextCallbacks {
            get_model: Arc::clone(&self.get_model),
            get_thinking_level: Arc::clone(&self.get_thinking_level_fn),
            get_scoped_models: Arc::clone(&self.get_scoped_models),
            get_footer_data: Arc::clone(&self.get_footer_data),
            is_idle: Arc::clone(&self.is_idle_fn),
            has_active_retry: Arc::clone(&self.has_active_retry_fn),
            is_project_trusted: Arc::clone(&self.is_project_trusted_fn),
            get_signal: Arc::clone(&self.get_signal_fn),
            abort: Arc::clone(&self.abort_fn),
            has_pending_messages: Arc::clone(&self.has_pending_messages_fn),
            shutdown: Arc::clone(&self.shutdown_handler),
            restart: Arc::clone(&self.restart_fn),
            get_control_db_path: Arc::clone(&self.get_control_db_path_fn),
            get_context_usage: Arc::clone(&self.get_context_usage_fn),
            get_multi_agent_agent_id: Arc::clone(&self.get_multi_agent_agent_id_fn),
            get_multi_agent_parent_session_id: Arc::clone(&self.get_multi_agent_parent_session_id_fn),
            get_multi_agent_requires_agent_id: Arc::clone(&self.get_multi_agent_requires_agent_id_fn),
            get_multi_agent_store: Arc::clone(&self.get_multi_agent_store_fn),
            get_tool_detach_registry: Arc::clone(&self.get_tool_detach_registry_fn),
            compact: Arc::clone(&self.compact_fn),
            get_system_prompt: Arc::clone(&self.get_system_prompt_fn),
            get_system_prompt_options: Arc::clone(&self.get_system_prompt_options_fn),
            relocate: {
                let runner = runner.clone();
                let handler = Arc::clone(&self.relocate_handler);
                Arc::new(move |target_cwd: &str| {
                    let target_cwd = target_cwd.to_owned();
                    let handler = Arc::clone(&handler);
                    let runner = runner.clone();
                    Box::pin(async move {
                        runner.assert_active()?;
                        handler(&target_cwd).await
                    })
                })
            },
            relocate_after_tool_result: {
                let runner = runner.clone();
                Arc::new(move |tool_call_id: &str, target_cwd: &str| {
                    runner.schedule_tool_result_relocation(tool_call_id, target_cwd)
                })
            },
        })
    }

    /// Create an `ExtensionContext` for event handlers and tool execution.
    pub fn create_context(&self) -> ExtensionContext {
        let callbacks = self.context_callbacks();
        let mut context = ExtensionContext::new(
            Arc::clone(&self.ui_context),
            self.mode,
            self.has_ui,
            self.cwd.clone(),
            Arc::clone(&self.session_manager),
            Arc::clone(&self.model_registry),
            callbacks,
        );
        context.set_settings_manager(self.settings_manager.clone());
        context
    }

    /// Create a command context; guarded getters stay live through the shared callbacks.
    pub fn create_command_context(&self) -> ExtensionCommandContext {
        let runner = self.clone();
        let context = self.create_context();
        ExtensionCommandContext {
            context,
            show_approval_selector: {
                let runner = runner.clone();
                Arc::new(move || {
                    runner.assert_active()?;
                    (runner.show_approval_selector_handler)()
                })
            },
            show_sandbox_selector: {
                let runner = runner.clone();
                Arc::new(move || {
                    runner.assert_active()?;
                    (runner.show_sandbox_selector_handler)()
                })
            },
            wait_for_idle: {
                let runner = runner.clone();
                Arc::new(move || {
                    let runner = runner.clone();
                    Box::pin(async move {
                        let _ = runner.assert_active();
                        (runner.wait_for_idle_fn)().await;
                    })
                })
            },
            new_session: {
                let runner = runner.clone();
                Arc::new(move |options| {
                    let runner = runner.clone();
                    Box::pin(async move {
                        runner.assert_active()?;
                        (runner.new_session_handler)(options).await
                    })
                })
            },
            fork: {
                let runner = runner.clone();
                Arc::new(move |entry_id, options| {
                    let runner = runner.clone();
                    Box::pin(async move {
                        runner.assert_active()?;
                        (runner.fork_handler)(entry_id, options).await
                    })
                })
            },
            navigate_tree: {
                let runner = runner.clone();
                Arc::new(move |target_id, options| {
                    let runner = runner.clone();
                    Box::pin(async move {
                        runner.assert_active()?;
                        (runner.navigate_tree_handler)(target_id, options).await
                    })
                })
            },
            switch_session: {
                let runner = runner.clone();
                Arc::new(move |session_path, options| {
                    let runner = runner.clone();
                    Box::pin(async move {
                        runner.assert_active()?;
                        (runner.switch_session_handler)(session_path, options).await
                    })
                })
            },
            reload: {
                let runner = runner.clone();
                Arc::new(move || {
                    let runner = runner.clone();
                    Box::pin(async move {
                        let _ = runner.assert_active();
                        (runner.reload_handler)().await;
                    })
                })
            },
            restart: {
                let runner = runner.clone();
                Arc::new(move |options| {
                    let runner = runner.clone();
                    Box::pin(async move {
                        let _ = runner.assert_active();
                        (runner.restart_fn)(options).await;
                    })
                })
            },
            get_system_prompt_options: {
                let runner = runner.clone();
                Arc::new(move || (runner.get_system_prompt_options_fn)())
            },
        }
    }

    async fn emit_session_before_switch(
        &self,
        event: &mut SessionBeforeSwitchEvent,
        ctx: &ExtensionContext,
    ) -> EmitResult {
        let mut result = None;
        for extension in &self.extensions {
            for handler in &extension.handlers.session_before_switch {
                match handler.call(event.clone(), ctx.clone()).await {
                    Ok(Some(handler_result)) => {
                        if handler_result.cancel == Some(true) {
                            return EmitResult::SessionBeforeSwitch(handler_result);
                        }
                        result = Some(handler_result);
                    }
                    Ok(None) => {}
                    Err(error) => self.emit_handler_error(extension, "session_before_switch", error),
                }
            }
        }
        match result {
            Some(result) => EmitResult::SessionBeforeSwitch(result),
            None => EmitResult::None,
        }
    }

    async fn emit_session_before_fork(&self, event: &mut SessionBeforeForkEvent, ctx: &ExtensionContext) -> EmitResult {
        let mut result = None;
        for extension in &self.extensions {
            for handler in &extension.handlers.session_before_fork {
                match handler.call(event.clone(), ctx.clone()).await {
                    Ok(Some(handler_result)) => {
                        if handler_result.cancel == Some(true) {
                            return EmitResult::SessionBeforeFork(handler_result);
                        }
                        result = Some(handler_result);
                    }
                    Ok(None) => {}
                    Err(error) => self.emit_handler_error(extension, "session_before_fork", error),
                }
            }
        }
        match result {
            Some(result) => EmitResult::SessionBeforeFork(result),
            None => EmitResult::None,
        }
    }

    async fn emit_session_before_compact(
        &self,
        event: &mut SessionBeforeCompactEvent,
        ctx: &ExtensionContext,
    ) -> EmitResult {
        let mut result = None;
        for extension in &self.extensions {
            for handler in &extension.handlers.session_before_compact {
                match handler.call(event.clone(), ctx.clone()).await {
                    Ok(Some(handler_result)) => {
                        if handler_result.cancel == Some(true) {
                            return EmitResult::SessionBeforeCompact(handler_result);
                        }
                        result = Some(handler_result);
                    }
                    Ok(None) => {}
                    Err(error) => self.emit_handler_error(extension, "session_before_compact", error),
                }
            }
        }
        match result {
            Some(result) => EmitResult::SessionBeforeCompact(result),
            None => EmitResult::None,
        }
    }

    async fn emit_session_before_tree(&self, event: &mut SessionBeforeTreeEvent, ctx: &ExtensionContext) -> EmitResult {
        let mut result = None;
        for extension in &self.extensions {
            for handler in &extension.handlers.session_before_tree {
                match handler.call(event.clone(), ctx.clone()).await {
                    Ok(Some(handler_result)) => {
                        if handler_result.cancel == Some(true) {
                            return EmitResult::SessionBeforeTree(handler_result);
                        }
                        result = Some(handler_result);
                    }
                    Ok(None) => {}
                    Err(error) => self.emit_handler_error(extension, "session_before_tree", error),
                }
            }
        }
        match result {
            Some(result) => EmitResult::SessionBeforeTree(result),
            None => EmitResult::None,
        }
    }

    async fn emit_compaction(&self, event: &mut CompactionEvent, ctx: &ExtensionContext) -> EmitResult {
        let mut result = None;
        for extension in &self.extensions {
            for handler in &extension.handlers.compaction {
                match handler.call(event.clone(), ctx.clone()).await {
                    Ok(Some(handler_result)) => {
                        if handler_result.cancel == Some(true) || handler_result.compaction.is_some() {
                            return EmitResult::Compaction(handler_result);
                        }
                        result = Some(handler_result);
                    }
                    Ok(None) => {}
                    Err(error) => self.emit_handler_error(extension, "compaction", error),
                }
            }
        }
        match result {
            Some(result) => EmitResult::Compaction(result),
            None => EmitResult::None,
        }
    }

    fn emit_handler_error(&self, extension: &Extension, event: &str, error: HandlerError) {
        self.emit_error(ExtensionError {
            extension_path: extension.path.clone(),
            event: event.to_owned(),
            error: error.message,
            stack: error.stack,
        });
    }

    /// Emit a generic runner event, preserving extension order and error isolation.
    pub async fn emit(&self, event: &mut ExtensionEvent) -> EmitResult {
        let ctx = self.create_context();
        match event {
            ExtensionEvent::SessionBeforeSwitch(event) => {
                return self.emit_session_before_switch(event, &ctx).await;
            }
            ExtensionEvent::SessionBeforeFork(event) => {
                return self.emit_session_before_fork(event, &ctx).await;
            }
            ExtensionEvent::SessionBeforeCompact(event) => {
                return self.emit_session_before_compact(event, &ctx).await;
            }
            ExtensionEvent::SessionBeforeTree(event) => {
                return self.emit_session_before_tree(event, &ctx).await;
            }
            ExtensionEvent::Compaction(event) => {
                return self.emit_compaction(event, &ctx).await;
            }
            ExtensionEvent::BeforeProviderHeaders(event) => {
                for extension in &self.extensions {
                    for handler in &extension.handlers.before_provider_headers {
                        match handler.call(event.clone(), ctx.clone()).await {
                            Ok(Some(headers)) => event.headers = headers,
                            Ok(None) => {}
                            Err(error) => {
                                self.emit_handler_error(extension, "before_provider_headers", error);
                            }
                        }
                    }
                }
                return EmitResult::None;
            }
            ExtensionEvent::SessionCompactionSource(event) => {
                let mut result = None;
                for extension in &self.extensions {
                    for handler in &extension.handlers.session_compaction_source {
                        match handler.call(event.clone(), ctx.clone()).await {
                            Ok(Some(handler_result)) => result = Some(handler_result),
                            Ok(None) => {}
                            Err(error) => {
                                self.emit_handler_error(extension, "session_compaction_source", error);
                            }
                        }
                    }
                }
                return match result {
                    Some(result) => EmitResult::SessionCompactionSource(result),
                    None => EmitResult::None,
                };
            }
            _ => {}
        }
        self.emit_void(event, &ctx).await;
        EmitResult::None
    }

    async fn emit_void(&self, event: &mut ExtensionEvent, ctx: &ExtensionContext) {
        macro_rules! dispatch {
            ($payload:expr, $field:ident, $name:literal) => {{
                for extension in &self.extensions {
                    for handler in &extension.handlers.$field {
                        if let Err(error) = handler.call((*$payload).clone(), ctx.clone()).await {
                            self.emit_handler_error(extension, $name, error);
                        }
                    }
                }
            }};
        }
        match event {
            ExtensionEvent::SessionStart(event) => dispatch!(event, session_start, "session_start"),
            ExtensionEvent::SessionInfoChanged(event) => {
                dispatch!(event, session_info_changed, "session_info_changed")
            }
            ExtensionEvent::SessionCompact(event) => dispatch!(event, session_compact, "session_compact"),
            ExtensionEvent::SessionShutdown(event) => dispatch!(event, session_shutdown, "session_shutdown"),
            ExtensionEvent::SessionTree(event) => dispatch!(event, session_tree, "session_tree"),
            ExtensionEvent::AfterProviderResponse(event) => {
                dispatch!(event, after_provider_response, "after_provider_response")
            }
            ExtensionEvent::AgentStart(event) => dispatch!(event, agent_start, "agent_start"),
            ExtensionEvent::AgentEnd(event) => dispatch!(event, agent_end, "agent_end"),
            ExtensionEvent::AgentSettled(event) => dispatch!(event, agent_settled, "agent_settled"),
            ExtensionEvent::TurnStart(event) => dispatch!(event, turn_start, "turn_start"),
            ExtensionEvent::TurnEnd(event) => dispatch!(event, turn_end, "turn_end"),
            ExtensionEvent::MessageStart(event) => dispatch!(event, message_start, "message_start"),
            ExtensionEvent::MessageUpdate(event) => dispatch!(event.as_mut(), message_update, "message_update"),
            ExtensionEvent::ToolExecutionStart(event) => {
                dispatch!(event, tool_execution_start, "tool_execution_start")
            }
            ExtensionEvent::ToolExecutionUpdate(event) => {
                dispatch!(event, tool_execution_update, "tool_execution_update")
            }
            ExtensionEvent::ToolExecutionEnd(event) => {
                dispatch!(event, tool_execution_end, "tool_execution_end")
            }
            ExtensionEvent::ModelSelect(event) => dispatch!(event.as_mut(), model_select, "model_select"),
            ExtensionEvent::ThinkingLevelSelect(event) => {
                dispatch!(event, thinking_level_select, "thinking_level_select")
            }
            // Events with dedicated emit methods are not handled here.
            ExtensionEvent::ProjectTrust(_)
            | ExtensionEvent::BeforeProviderHeaders(_)
            | ExtensionEvent::ResourcesDiscover(_)
            | ExtensionEvent::SessionBeforeSwitch(_)
            | ExtensionEvent::SessionBeforeFork(_)
            | ExtensionEvent::SessionBeforeCompact(_)
            | ExtensionEvent::Compaction(_)
            | ExtensionEvent::SessionCompactionSource(_)
            | ExtensionEvent::RuntimeMailbox(_)
            | ExtensionEvent::SessionBeforeTree(_)
            | ExtensionEvent::Context(_)
            | ExtensionEvent::BeforeProviderRequest(_)
            | ExtensionEvent::BeforeAgentStart(_)
            | ExtensionEvent::MessageEnd(_)
            | ExtensionEvent::UserBash(_)
            | ExtensionEvent::Input(_)
            | ExtensionEvent::ToolCall(_)
            | ExtensionEvent::ToolResult(_) => {}
        }
    }

    pub async fn emit_runtime_mailbox(
        &self,
        event: RuntimeMailboxEvent,
    ) -> Result<RuntimeMailboxEventResult, HandlerError> {
        let ctx = self.create_context();
        let mut registrations = Vec::new();
        for extension in &self.extensions {
            for handler in &extension.handlers.runtime_mailbox {
                registrations.push((extension, handler));
            }
        }
        for (extension, handler) in registrations {
            match handler.call(event.clone(), ctx.clone()).await {
                Ok(Some(result)) if result.handled => return Ok(result),
                Ok(_) => {}
                Err(error) => {
                    self.report_runtime_mailbox_handler_error(&extension.path, &event, &error);
                    return Err(error);
                }
            }
        }
        Ok(RuntimeMailboxEventResult { handled: false })
    }

    fn report_runtime_mailbox_handler_error(
        &self,
        extension_path: &str,
        event: &RuntimeMailboxEvent,
        error: &HandlerError,
    ) {
        let _ = event;
        self.emit_error(ExtensionError {
            extension_path: extension_path.to_owned(),
            event: "runtime_mailbox".to_owned(),
            error: error.message.clone(),
            stack: error.stack.clone(),
        });
    }

    pub async fn emit_message_end(&self, event: MessageEndEvent) -> Option<pi_agent_core::AgentMessage> {
        let ctx = self.create_context();
        let mut current_message = event.message.clone();
        let mut modified = false;

        for extension in &self.extensions {
            for handler in &extension.handlers.message_end {
                let current_event = MessageEndEvent {
                    message: current_message.clone(),
                };
                match handler.call(current_event.clone(), ctx.clone()).await {
                    Ok(Some(handler_result)) => {
                        let Some(message) = handler_result.message else {
                            continue;
                        };
                        if message.role() != current_message.role() {
                            self.emit_error(ExtensionError {
                                extension_path: extension.path.clone(),
                                event: "message_end".to_owned(),
                                error: "message_end handlers must return a message with the same role".to_owned(),
                                stack: None,
                            });
                            continue;
                        }
                        current_message = message;
                        modified = true;
                    }
                    Ok(None) => {}
                    Err(error) => self.emit_handler_error(extension, "message_end", error),
                }
            }
        }

        if modified { Some(current_message) } else { None }
    }

    pub async fn emit_tool_result(&self, event: ToolResultEvent) -> Option<ToolResultEventResult> {
        let ctx = self.create_context();
        let mut current_event = event;
        let mut modified = false;

        for extension in &self.extensions {
            for handler in &extension.handlers.tool_result {
                match handler.call(current_event.clone(), ctx.clone()).await {
                    Ok(Some(handler_result)) => {
                        if let Some(content) = handler_result.content {
                            current_event.content = content;
                            modified = true;
                        }
                        if let Some(details) = handler_result.details {
                            current_event.details = Some(details);
                            modified = true;
                        }
                        if let Some(is_error) = handler_result.is_error {
                            current_event.is_error = is_error;
                            modified = true;
                        }
                    }
                    Ok(None) => {}
                    Err(error) => self.emit_handler_error(extension, "tool_result", error),
                }
            }
        }

        if !modified {
            return None;
        }
        Some(ToolResultEventResult {
            content: Some(current_event.content),
            details: current_event.details,
            is_error: Some(current_event.is_error),
            usage: None,
        })
    }

    pub async fn emit_tool_call(&self, event: &mut ToolCallEvent) -> Result<Option<ToolCallEventResult>, HandlerError> {
        let ctx = self.create_context();
        let mut result = None;
        for extension in &self.extensions {
            for handler in &extension.handlers.tool_call {
                let handler_result = handler.call(event.clone(), ctx.clone()).await?;
                if let Some(handler_result) = handler_result {
                    if let Some(input) = &handler_result.input {
                        event.input = input.clone();
                    }
                    if handler_result.block == Some(true) {
                        return Ok(Some(handler_result));
                    }
                    result = Some(handler_result);
                }
            }
        }
        Ok(result)
    }

    pub async fn emit_tool_gates(
        &self,
        event: &mut ToolCallEvent,
    ) -> Result<Option<ToolCallEventResult>, HandlerError> {
        let ctx = self.create_context();
        for extension in &self.extensions {
            for gate in &extension.tool_gates {
                let result = gate.call(event.clone(), ctx.clone()).await?;
                if let Some(result) = &result
                    && let Some(input) = &result.input
                {
                    event.input = input.clone();
                }
                if result.as_ref().is_some_and(|result| result.block == Some(true)) {
                    return Ok(result);
                }
            }
        }
        Ok(None)
    }

    pub async fn emit_approval_reviewers(
        &self,
        event: &mut ToolCallEvent,
    ) -> Result<Option<ApprovalReviewerResult>, HandlerError> {
        let ctx = self.create_context();
        for extension in &self.extensions {
            for reviewer in &extension.approval_reviewers {
                let result = reviewer.call(event.clone(), ctx.clone()).await?;
                let decision = apply_approval_reviewer_result(event, result);
                if decision.is_some() {
                    return Ok(decision);
                }
            }
        }
        Ok(None)
    }

    pub async fn emit_user_bash(&self, event: &UserBashEvent) -> Option<UserBashEventResult> {
        let ctx = self.create_context();
        for extension in &self.extensions {
            for handler in &extension.handlers.user_bash {
                match handler.call(event.clone(), ctx.clone()).await {
                    Ok(Some(handler_result)) => return Some(handler_result),
                    Ok(None) => {}
                    Err(error) => self.emit_handler_error(extension, "user_bash", error),
                }
            }
        }
        None
    }

    pub async fn emit_context(
        &self,
        messages: Vec<pi_agent_core::AgentMessage>,
    ) -> Result<Vec<pi_agent_core::AgentMessage>, HandlerError> {
        let ctx = self.create_context();
        let signal = ctx.signal();
        let mut current_messages = messages;

        for extension in &self.extensions {
            for handler in &extension.handlers.context {
                let event = ContextEvent {
                    messages: current_messages.clone(),
                };
                let call = handler.call(event, ctx.clone());
                let result = match &signal {
                    Some(signal) => call_context_handler_abortable(call, signal).await,
                    None => call.await,
                };
                match result {
                    Ok(Some(handler_result)) => {
                        if let Some(messages) = handler_result.messages {
                            current_messages = messages;
                        }
                    }
                    Ok(None) => {}
                    Err(error) => {
                        if signal.as_ref().is_some_and(CancellationToken::is_cancelled) {
                            return Err(error);
                        }
                        self.emit_handler_error(extension, "context", error);
                    }
                }
            }
        }

        Ok(current_messages)
    }

    pub async fn emit_before_provider_request(&self, payload: Value) -> Value {
        let ctx = self.create_context();
        let mut current_payload = payload;
        for extension in &self.extensions {
            for handler in &extension.handlers.before_provider_request {
                let event = BeforeProviderRequestEvent {
                    payload: current_payload.clone(),
                };
                let result = handler.call(event, ctx.clone()).await;
                match result {
                    Ok(Some(handler_result)) => current_payload = handler_result,
                    Ok(None) => {}
                    Err(error) => self.emit_handler_error(extension, "before_provider_request", error),
                }
            }
        }
        current_payload
    }

    pub async fn emit_before_provider_headers(&self, headers: ProviderHeaders) -> ProviderHeaders {
        let mut event = ExtensionEvent::BeforeProviderHeaders(BeforeProviderHeadersEvent { headers });
        let _ = self.emit(&mut event).await;
        match event {
            ExtensionEvent::BeforeProviderHeaders(event) => event.headers,
            _ => unreachable!("event variant was preserved"),
        }
    }

    pub async fn emit_before_agent_start(
        &self,
        prompt: &str,
        images: Option<Vec<ImageContent>>,
        system_prompt: &str,
        system_prompt_options: BuildSystemPromptOptions,
    ) -> Option<BeforeAgentStartCombinedResult> {
        let current_system_prompt = Arc::new(Mutex::new(system_prompt.to_owned()));
        let getter = {
            let current_system_prompt = Arc::clone(&current_system_prompt);
            Arc::new(move || {
                current_system_prompt
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .clone()
            })
        };
        let ctx = self.create_context().with_system_prompt_override(getter);
        let mut messages = Vec::new();
        let mut system_prompt_modified = false;

        for extension in &self.extensions {
            for handler in &extension.handlers.before_agent_start {
                let event = BeforeAgentStartEvent {
                    prompt: prompt.to_owned(),
                    images: images.clone(),
                    system_prompt: current_system_prompt
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .clone(),
                    system_prompt_options: system_prompt_options.clone(),
                };
                match handler.call(event, ctx.clone()).await {
                    Ok(Some(handler_result)) => {
                        if let Some(message) = handler_result.message {
                            messages.push(message);
                        }
                        if let Some(system_prompt) = handler_result.system_prompt {
                            *current_system_prompt
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner) = system_prompt;
                            system_prompt_modified = true;
                        }
                    }
                    Ok(None) => {}
                    Err(error) => self.emit_handler_error(extension, "before_agent_start", error),
                }
            }
        }

        if messages.is_empty() && !system_prompt_modified {
            return None;
        }
        Some(BeforeAgentStartCombinedResult {
            messages: if messages.is_empty() { None } else { Some(messages) },
            system_prompt: if system_prompt_modified {
                Some(
                    current_system_prompt
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .clone(),
                )
            } else {
                None
            },
        })
    }

    pub async fn emit_resources_discover(&self, cwd: &str, reason: ResourcesDiscoverReason) -> ResourcesDiscoverPaths {
        let ctx = self.create_context();
        let mut result = ResourcesDiscoverPaths::default();
        for extension in &self.extensions {
            for handler in &extension.handlers.resources_discover {
                let event = ResourcesDiscoverEvent {
                    cwd: cwd.to_owned(),
                    reason,
                };
                match handler.call(event, ctx.clone()).await {
                    Ok(Some(handler_result)) => {
                        if let Some(paths) = handler_result.skill_paths.filter(|paths| !paths.is_empty()) {
                            result.skill_paths.extend(paths.into_iter().map(|path| DiscoveredPath {
                                path,
                                extension_path: extension.path.clone(),
                            }));
                        }
                        if let Some(paths) = handler_result.prompt_paths.filter(|paths| !paths.is_empty()) {
                            result.prompt_paths.extend(paths.into_iter().map(|path| DiscoveredPath {
                                path,
                                extension_path: extension.path.clone(),
                            }));
                        }
                        if let Some(paths) = handler_result.theme_paths.filter(|paths| !paths.is_empty()) {
                            result.theme_paths.extend(paths.into_iter().map(|path| DiscoveredPath {
                                path,
                                extension_path: extension.path.clone(),
                            }));
                        }
                    }
                    Ok(None) => {}
                    Err(error) => self.emit_handler_error(extension, "resources_discover", error),
                }
            }
        }
        result
    }

    /// Emit `input`; transforms chain and `handled` short-circuits.
    pub async fn emit_input(
        &self,
        text: &str,
        images: Option<Vec<ImageContent>>,
        source: InputSource,
        streaming_behavior: Option<StreamingBehavior>,
    ) -> InputEventResult {
        let ctx = self.create_context();
        let mut current_text = text.to_owned();
        let mut current_images = images.clone();

        for extension in &self.extensions {
            for handler in &extension.handlers.input {
                let event = InputEvent {
                    text: current_text.clone(),
                    images: current_images.clone(),
                    source,
                    streaming_behavior,
                };
                match handler.call(event, ctx.clone()).await {
                    Ok(Some(InputEventResult::Handled)) => return InputEventResult::Handled,
                    Ok(Some(InputEventResult::Transform { text, images })) => {
                        current_text = text;
                        if images.is_some() {
                            current_images = images;
                        }
                    }
                    Ok(Some(InputEventResult::Continue)) | Ok(None) => {}
                    Err(error) => self.emit_handler_error(extension, "input", error),
                }
            }
        }

        if current_text != text || current_images != images {
            InputEventResult::Transform {
                text: current_text,
                images: current_images,
            }
        } else {
            InputEventResult::Continue
        }
    }
}

async fn call_context_handler_abortable(
    future: BoxFuture<'_, Result<Option<ContextEventResult>, HandlerError>>,
    signal: &CancellationToken,
) -> Result<Option<ContextEventResult>, HandlerError> {
    if signal.is_cancelled() {
        return Err(HandlerError::new("Agent run aborted"));
    }
    tokio::select! {
        result = future => result,
        _ = signal.cancelled() => Err(HandlerError::new("Agent run aborted")),
    }
}

fn apply_approval_reviewer_result(
    event: &mut ToolCallEvent,
    result: Option<ApprovalReviewerResult>,
) -> Option<ApprovalReviewerResult> {
    let result = result?;
    if let ApprovalReviewerResult::Allow {
        updated_input: Some(updated_input),
    } = &result
    {
        replace_tool_input(&mut event.input, updated_input);
    }
    Some(result)
}

fn replace_tool_input(target: &mut serde_json::Map<String, Value>, source: &serde_json::Map<String, Value>) {
    target.clear();
    for (key, value) in source {
        target.insert(key.clone(), value.clone());
    }
}
