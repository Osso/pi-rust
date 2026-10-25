//! Extension runner (`core/extensions/runner.ts`).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use indexmap::IndexMap;
use pi_agent_core::harness::types::ThinkingLevel;
use pi_ai::request::CallbackError;
use pi_ai::types::ImageContent;
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_json::{JsObject, JsString, JsValue};
use pi_tui::keybindings::{KeyId, KeybindingsConfig};
use serde::{Deserialize, Serialize};

use crate::core::detached_job_runner::DetachedJobLifecycleController;
use crate::core::diagnostics::{ResourceDiagnostic, ResourceDiagnosticType};
use crate::core::model_registry::ModelRegistry;
use crate::core::session_manager::SharedSessionManager;
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
    keybinding: JsString,
    restrict_override: bool,
}

fn build_builtin_keybindings(resolved: &KeybindingsConfig) -> IndexMap<KeyId, BuiltInKeyBinding> {
    let mut builtin = IndexMap::new();
    for (keybinding, keys) in resolved {
        let restrict_override = RESERVED_KEYBINDINGS_FOR_EXTENSION_CONFLICTS
            .iter()
            .any(|reserved| keybinding == *reserved);
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
    pub system_prompt: Option<JsString>,
}

/// Path returned by `resources_discover` together with its extension.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredPath {
    pub path: JsString,
    pub extension_path: JsString,
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
        let mut callback_index = 0;
        while let Some(handler) = {
            let registration = extension.read();
            registration.handlers.project_trust.get(callback_index).cloned()
        } {
            callback_index += 1;
            match handler.call(event.clone(), ctx.clone()).await {
                Ok(result) if result.trusted == ProjectTrustEventDecision::Undecided => {}
                Ok(result) => return (Some(result), errors),
                Err(error) => errors.push(ExtensionError {
                    extension_path: extension.read().path.clone(),
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
    /// Live state shared with every context this runner creates.
    state: Arc<Mutex<ExtensionContextState>>,
    error_listeners: Arc<Mutex<Vec<(u64, ExtensionErrorListener)>>>,
    error_listener_id: Arc<AtomicU64>,
    is_tool_result_relocation_prepared: Arc<AtomicBool>,
    shortcut_diagnostics: Arc<Mutex<Vec<ResourceDiagnostic>>>,
    command_diagnostics: Arc<Mutex<Vec<ResourceDiagnostic>>>,
    internal_commands: IndexMap<String, InternalCommandHandler>,
}

impl ExtensionRunner {
    fn live(&self) -> std::sync::MutexGuard<'_, ExtensionContextState> {
        self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn new(
        extensions: Vec<Extension>,
        runtime: ExtensionRuntime,
        cwd: impl Into<JsString>,
        session_manager: SharedSessionManager,
        model_registry: Arc<ModelRegistry>,
        settings_manager: Option<Arc<Mutex<SettingsManager>>>,
    ) -> Self {
        let cwd = cwd.into();
        Self {
            extensions,
            runtime,
            state: Arc::new(Mutex::new(ExtensionContextState::new(
                cwd,
                session_manager,
                model_registry,
                settings_manager,
            ))),
            error_listeners: Arc::new(Mutex::new(Vec::new())),
            error_listener_id: Arc::new(AtomicU64::new(0)),
            is_tool_result_relocation_prepared: Arc::new(AtomicBool::new(false)),
            shortcut_diagnostics: Arc::new(Mutex::new(Vec::new())),
            command_diagnostics: Arc::new(Mutex::new(Vec::new())),
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
    ) -> Result<(), CallbackError> {
        let set_model = Arc::clone(&actions.set_model);
        self.bind_runtime_actions(actions);
        self.bind_context_actions(context_actions, set_model);
        self.flush_pending_provider_registrations(provider_actions.as_ref())?;
        self.bind_immediate_provider_actions(provider_actions);
        Ok(())
    }

    fn bind_runtime_actions(&self, actions: ExtensionActions) {
        self.runtime.set_actions(actions);
    }

    fn bind_context_actions(&mut self, actions: ExtensionContextActions, set_model: SetModelHandler) {
        let mut state = self.live();
        state.get_model = actions.get_model;
        state.set_model_fn = set_model;
        state.get_thinking_level_fn = actions
            .get_thinking_level
            .unwrap_or_else(|| Arc::new(|| ThinkingLevel::Off));
        state.set_thinking_level_fn = actions.set_thinking_level.unwrap_or_else(|| Arc::new(|_| Ok(())));
        state.get_footer_data = actions.get_footer_data.unwrap_or_else(|| Arc::new(|| None));
        state.get_scoped_models = actions.get_scoped_models.unwrap_or_else(|| Arc::new(Vec::new));
        state.is_idle_fn = actions.is_idle;
        state.has_active_retry_fn = actions.has_active_retry;
        state.is_project_trusted_fn = actions.is_project_trusted;
        state.get_signal_fn = actions.get_signal;
        state.abort_fn = actions.abort;
        state.has_pending_messages_fn = actions.has_pending_messages;
        state.shutdown_handler = actions.shutdown;
        state.restart_fn = actions.restart;
        state.get_control_db_path_fn = actions.get_control_db_path.unwrap_or_else(|| Arc::new(|| None));
        state.get_context_usage_fn = actions.get_context_usage;
        state.get_multi_agent_agent_id_fn = actions.get_multi_agent_agent_id.unwrap_or_else(|| Arc::new(|| None));
        state.get_multi_agent_parent_session_id_fn = actions
            .get_multi_agent_parent_session_id
            .unwrap_or_else(|| Arc::new(|| None));
        state.get_no_supervisor_fn = actions.get_no_supervisor.unwrap_or_else(|| Arc::new(|| false));
        state.get_multi_agent_requires_agent_id_fn = actions
            .get_multi_agent_requires_agent_id
            .unwrap_or_else(|| Arc::new(|| false));
        state.get_multi_agent_store_fn = actions.get_multi_agent_store.unwrap_or_else(|| Arc::new(|| None));
        state.get_tool_detach_registry_fn = actions.get_tool_detach_registry.unwrap_or_else(|| Arc::new(|| None));
        state.compact_fn = actions.compact;
        state.get_system_prompt_fn = actions.get_system_prompt;
        state.get_system_prompt_options_fn = actions.get_system_prompt_options;
    }

    /// Binds the session-current detached job lifecycle exposed as `ctx.detachedJobLifecycle`.
    ///
    /// The agent-session owner calls this after [`Self::bind_core`]; the context
    /// getter is available before binding and returns `None`.
    pub fn bind_detached_job_lifecycle(
        &mut self,
        provider: Arc<dyn Fn() -> Option<Arc<DetachedJobLifecycleController>> + Send + Sync>,
    ) {
        self.live().get_detached_job_lifecycle_fn = provider;
    }

    fn flush_pending_provider_registrations(
        &self,
        provider_actions: Option<&ProviderActions>,
    ) -> Result<(), CallbackError> {
        for registration in self.runtime.pending_provider_registrations() {
            if let Err(error) = self.register_provider(provider_actions, &registration.name, &registration.config) {
                // Frozen runner.ts:442–455 catches only pending registrations;
                // Error.message/String(err) conversion can itself reject.
                let message = error.error_or_string_message()?;
                self.report_provider_registration_error(&registration.extension_path, &message);
            }
        }
        self.runtime.clear_pending_provider_registrations();
        Ok(())
    }

    fn bind_immediate_provider_actions(&self, provider_actions: Option<ProviderActions>) {
        let register_runner = self.clone();
        let register_actions = provider_actions.clone();
        let unregister_runner = self.clone();
        let unregister_actions = provider_actions;
        self.runtime
            .set_provider_handlers(super::extension_runtime::ProviderHandlers {
                register_provider: Arc::new(move |name, config, _extension_path| {
                    register_runner.register_provider(register_actions.as_ref(), name, config)
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
    ) -> Result<(), CallbackError> {
        if let Some(register) = provider_actions.and_then(|actions| actions.register_provider.as_ref()) {
            register(name, config)?;
            return Ok(());
        }
        let registry = Arc::clone(&self.live().model_registry);
        // Frozen runner.ts:461–473 propagates immediate operations unchanged.
        registry.register_provider(name, config.clone())
    }

    fn unregister_provider(&self, provider_actions: Option<&ProviderActions>, name: &str) -> Result<(), CallbackError> {
        if let Some(unregister) = provider_actions.and_then(|actions| actions.unregister_provider.as_ref()) {
            return unregister(name);
        }
        let registry = Arc::clone(&self.live().model_registry);
        registry.unregister_provider(name)
    }

    fn report_provider_registration_error(&self, extension_path: &JsString, error: &JsString) {
        self.emit_error(ExtensionError {
            extension_path: extension_path.to_owned(),
            event: "register_provider".to_owned(),
            error: error.into(),
            stack: None,
        });
    }

    pub fn set_relocate_handler(&mut self, handler: RelocateCallback) {
        self.live().relocate_handler = handler;
    }

    pub fn set_tool_result_relocation_handlers(
        &mut self,
        prepare: RelocateCallback,
        activate: crate::core::extensions::types::ActivateToolResultRelocationHandler,
    ) {
        let mut state = self.live();
        state.prepare_tool_result_relocation_handler = prepare;
        state.activate_tool_result_relocation_handler = activate;
    }

    pub async fn deliver_tool_result_relocation(&self, tool_call_id: &str) -> Result<(), HandlerError> {
        let target_cwd = self.live().pending_tool_result_relocations.shift_remove(tool_call_id);
        let Some(target_cwd) = target_cwd else {
            return Ok(());
        };
        self.prepare_working_directory_relocation(&target_cwd).await
    }

    pub async fn prepare_working_directory_relocation(&self, target_cwd: &JsString) -> Result<(), HandlerError> {
        let prepare = self.live().prepare_tool_result_relocation_handler.clone();
        prepare(target_cwd).await?;
        self.is_tool_result_relocation_prepared.store(true, Ordering::SeqCst);
        Ok(())
    }

    pub fn has_prepared_tool_result_relocation(&self) -> bool {
        self.is_tool_result_relocation_prepared.load(Ordering::SeqCst)
    }

    pub async fn activate_tool_result_relocation(&self) -> Result<(), HandlerError> {
        if !self.is_tool_result_relocation_prepared.swap(false, Ordering::SeqCst) {
            return Ok(());
        }
        let activate = self.live().activate_tool_result_relocation_handler.clone();
        activate().await
    }

    pub fn bind_command_context(&mut self, actions: Option<ExtensionCommandContextActions>) {
        let mut state = self.live();
        let Some(actions) = actions else {
            state.show_approval_selector_handler = Arc::new(|| Ok(()));
            state.show_sandbox_selector_handler = Arc::new(|| Ok(()));
            state.set_sandbox_profile_handler =
                Arc::new(|_, _| Err(HandlerError::new("Sandbox profile settings are unavailable")));
            state.wait_for_idle_fn = Arc::new(|| Box::pin(async {}));
            state.new_session_handler =
                Arc::new(|_| Box::pin(async { Ok(SessionTransitionResult { cancelled: false }) }));
            state.fork_handler = Arc::new(|_, _| Box::pin(async { Ok(SessionTransitionResult { cancelled: false }) }));
            state.navigate_tree_handler =
                Arc::new(|_, _| Box::pin(async { Ok(SessionTransitionResult { cancelled: false }) }));
            state.switch_session_handler = Arc::new(|_, _| {
                Box::pin(async {
                    Err(HandlerError::new(
                        "Session resume is not available in this session mode",
                    ))
                })
            });
            state.reload_handler = Arc::new(|| Box::pin(async { Ok(()) }));
            state.restart_fn = Arc::new(|_| Box::pin(async { Ok(()) }));
            return;
        };
        state.show_approval_selector_handler = actions.show_approval_selector;
        state.show_sandbox_selector_handler = actions.show_sandbox_selector;
        state.set_sandbox_profile_handler = actions.set_sandbox_profile;
        state.wait_for_idle_fn = actions.wait_for_idle;
        state.new_session_handler = actions.new_session;
        state.fork_handler = actions.fork;
        state.navigate_tree_handler = actions.navigate_tree;
        state.switch_session_handler = actions.switch_session;
        state.reload_handler = actions.reload;
        state.restart_fn = actions.restart;
    }

    /// Update the working directory reported by live contexts.
    pub fn set_cwd(&mut self, cwd: impl Into<JsString>) {
        self.live().cwd = cwd.into();
    }

    pub fn set_ui_context(&mut self, ui_context: Option<Arc<dyn ExtensionUiContext>>, mode: ExtensionMode) {
        let mut state = self.live();
        match ui_context {
            Some(ui_context) => {
                state.ui_context = ui_context;
                state.has_ui = true;
            }
            None => {
                state.ui_context = Arc::new(NoOpUiContext);
                state.has_ui = false;
            }
        }
        state.mode = mode;
    }

    pub fn get_ui_context(&self) -> Arc<dyn ExtensionUiContext> {
        Arc::clone(&self.live().ui_context)
    }

    pub fn has_ui(&self) -> bool {
        self.live().has_ui
    }

    pub fn get_extension_paths(&self) -> Vec<JsString> {
        self.extensions
            .iter()
            .map(|extension| extension.read().path.clone())
            .collect()
    }

    /// All registered tools; the first registration per name wins.
    pub fn get_all_registered_tools(&self) -> Vec<RegisteredTool> {
        let mut tools_by_name: IndexMap<String, RegisteredTool> = IndexMap::new();
        for extension in &self.extensions {
            for tool in extension.read().tools.values() {
                if !tools_by_name.contains_key(&tool.definition.name) {
                    tools_by_name.insert(tool.definition.name.clone(), tool.clone());
                }
            }
        }
        tools_by_name.into_values().collect()
    }

    pub fn get_tool_definition(&self, tool_name: &str) -> Option<ToolDefinition> {
        for extension in &self.extensions {
            if let Some(tool) = extension.read().tools.get(tool_name) {
                return Some(tool.definition.clone());
            }
        }
        None
    }

    pub fn get_flags(&self) -> IndexMap<String, ExtensionFlag> {
        let mut all_flags = IndexMap::new();
        for extension in &self.extensions {
            for (name, flag) in &extension.read().flags {
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

        let add_diagnostic = |message: JsString, extension_path: &JsString| {
            self.shortcut_diagnostics
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(ResourceDiagnostic {
                    diagnostic_type: ResourceDiagnosticType::Warning,
                    message: message.clone(),
                    path: Some(extension_path.into()),
                    collision: None,
                });
            if !self.has_ui() {
                // Node encodes the diagnostic only when writing it to stderr.
                eprintln!("{}", pi_js_compat::js_json::encode_node_utf8(&message));
            }
        };

        for extension in &self.extensions {
            for (key, shortcut) in &extension.read().shortcuts {
                let normalized = key.to_lowercase();
                if let Some(built_in) = builtin.get(&normalized) {
                    if built_in.restrict_override {
                        add_diagnostic(
                            JsString::from("Extension shortcut '")
                                + key
                                + "' from "
                                + &shortcut.extension_path
                                + " conflicts with built-in shortcut. Skipping.",
                            &shortcut.extension_path,
                        );
                        continue;
                    }
                    add_diagnostic(
                        JsString::from("Extension shortcut conflict: '")
                            + key
                            + "' is built-in shortcut for "
                            + &built_in.keybinding
                            + " and "
                            + &shortcut.extension_path
                            + ". Using "
                            + &shortcut.extension_path
                            + ".",
                        &shortcut.extension_path,
                    );
                }

                if let Some(existing) = extension_shortcuts.get(&normalized) {
                    add_diagnostic(
                        JsString::from("Extension shortcut conflict: '")
                            + key
                            + "' registered by both "
                            + &existing.extension_path
                            + " and "
                            + &shortcut.extension_path
                            + ". Using "
                            + &shortcut.extension_path
                            + ".",
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
        let message = {
            let mut state = self.live();
            if state.stale_message.is_some() {
                return;
            }
            let message = message.map_or_else(
                || {
                    "This extension ctx is stale after session replacement or reload. Do not use a captured pi or command ctx after ctx.newSession(), ctx.fork(), ctx.switchSession(), or ctx.reload(). For newSession, fork, and switchSession, move post-replacement work into withSession and use the ctx passed to withSession. For reload, do not use the old ctx after await ctx.reload().".to_owned()
                },
                str::to_owned,
            );
            state.stale_message = Some(message.clone());
            state.pending_tool_result_relocations.clear();
            message
        };
        self.runtime.invalidate(Some(&message));
    }

    pub fn assert_active(&self) -> Result<(), HandlerError> {
        self.live().assert_active()
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
            let registration = extension.read();
            let handlers = &registration.handlers;
            match event_type {
                "project_trust" => !handlers.project_trust.is_empty(),
                "resources_discover" => !handlers.resources_discover.is_empty(),
                "session_start" => !handlers.session_start.is_empty(),
                "session_info_changed" => !handlers.session_info_changed.is_empty(),
                "session_before_switch" => !handlers.session_before_switch.is_empty(),
                "session_before_fork" => !handlers.session_before_fork.is_empty(),
                "session_before_compact" => !handlers.session_before_compact.is_empty(),
                "compaction" => !handlers.compaction.is_empty(),
                "background_compaction_start" => !handlers.background_compaction_start.is_empty(),
                "background_compaction_end" => !handlers.background_compaction_end.is_empty(),
                "session_compaction_source" => !handlers.session_compaction_source.is_empty(),
                "session_compact" => !handlers.session_compact.is_empty(),
                "session_shutdown" => !handlers.session_shutdown.is_empty(),
                "runtime_mailbox" => !handlers.runtime_mailbox.is_empty(),
                "session_before_tree" => !handlers.session_before_tree.is_empty(),
                "session_tree" => !handlers.session_tree.is_empty(),
                "context" => !handlers.context.is_empty(),
                "before_provider_request" => !handlers.before_provider_request.is_empty(),
                "after_provider_response" => !handlers.after_provider_response.is_empty(),
                "before_agent_start" => !handlers.before_agent_start.is_empty(),
                "agent_start" => !handlers.agent_start.is_empty(),
                "model_request_start" => !handlers.model_request_start.is_empty(),
                "model_request_end" => !handlers.model_request_end.is_empty(),
                "agent_end" => !handlers.agent_end.is_empty(),
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
            .any(|extension| !extension.read().approval_reviewers.is_empty())
    }

    pub fn has_tool_gates(&self) -> bool {
        self.extensions
            .iter()
            .any(|extension| !extension.read().tool_gates.is_empty())
    }

    pub fn get_message_renderer(&self, custom_type: impl Into<JsString>) -> Option<MessageRenderer> {
        let custom_type = custom_type.into();
        for extension in &self.extensions {
            if let Some(renderer) = extension.read().message_renderers.get(&custom_type) {
                return Some(Arc::clone(renderer));
            }
        }
        None
    }

    pub fn get_markdown_transformers(&self) -> Vec<MarkdownTransformer> {
        self.extensions
            .iter()
            .filter_map(|extension| extension.read().markdown_transformer.clone())
            .collect()
    }

    pub fn get_entry_renderer(&self, custom_type: impl Into<JsString>) -> Option<EntryRenderer> {
        let custom_type = custom_type.into();
        for extension in &self.extensions {
            if let Some(renderer) = extension
                .read()
                .entry_renderers
                .as_ref()
                .and_then(|renderers| renderers.get(&custom_type))
            {
                return Some(Arc::clone(renderer));
            }
        }
        None
    }

    fn resolve_registered_commands(&self) -> Vec<ResolvedCommand> {
        let mut commands: Vec<RegisteredCommand> = Vec::new();
        let mut counts: IndexMap<JsString, usize> = IndexMap::new();
        for extension in &self.extensions {
            for command in extension.read().commands.values() {
                commands.push(command.clone());
                *counts.entry(command.name.clone()).or_insert(0) += 1;
            }
        }

        let mut seen: IndexMap<JsString, usize> = IndexMap::new();
        let mut taken_invocation_names: Vec<JsString> = Vec::new();

        commands
            .into_iter()
            .map(|command| {
                let occurrence = seen.get(&command.name).copied().unwrap_or(0) + 1;
                seen.insert(command.name.clone(), occurrence);

                let mut invocation_name = if counts.get(&command.name).copied().unwrap_or(0) > 1 {
                    pi_js_compat::js_format!("{}:{}", command.name, occurrence)
                } else {
                    command.name.clone()
                };

                if taken_invocation_names.contains(&invocation_name) {
                    let mut suffix = occurrence;
                    loop {
                        suffix += 1;
                        invocation_name = pi_js_compat::js_format!("{}:{}", command.name, suffix);
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

    pub fn get_command(&self, name: impl Into<JsString>) -> Option<ResolvedCommand> {
        let name = name.into();
        self.resolve_registered_commands()
            .into_iter()
            .find(|command| JsString::from(&command.invocation_name) == name)
    }

    pub fn get_prompt_command(&self, name: impl Into<JsString>) -> Option<ResolvedCommand> {
        let name = name.into();
        if let Some(command) = self.get_command(&name) {
            return Some(command);
        }
        let (name, handler) = self
            .internal_commands
            .iter()
            .find(|(key, _)| JsString::from(*key) == name)?;
        Some(ResolvedCommand {
            name: name.into(),
            source_info: SourceInfo {
                path: "<internal>".into(),
                source: "internal".into(),
                scope: SourceScope::Temporary,
                origin: SourceOrigin::TopLevel,
                base_dir: None,
            },
            description: None,
            get_argument_completions: None,
            handler: Arc::clone(handler),
            invocation_name: name.into(),
        })
    }

    pub fn shutdown(&self) {
        let shutdown = self.live().shutdown_handler.clone();
        shutdown();
    }

    pub fn get_active_tools(&self) -> Result<Vec<String>, HandlerError> {
        self.assert_active()?;
        (self.runtime.actions().get_active_tools)()
    }

    /// Create an `ExtensionContext` for event handlers and tool execution.
    pub fn create_context(&self) -> ExtensionContext {
        ExtensionContext::new(Arc::clone(&self.state))
    }

    /// Create a command context; every handler is read from shared state on call.
    pub fn create_command_context(&self) -> ExtensionCommandContext {
        ExtensionCommandContext {
            context: self.create_context(),
        }
    }

    async fn emit_session_before_switch(
        &self,
        event: &mut SessionBeforeSwitchEvent,
        ctx: &ExtensionContext,
    ) -> EmitResult {
        let mut result = None;
        for extension in &self.extensions {
            let mut callback_index = 0;
            while let Some(handler) = {
                let registration = extension.read();
                registration.handlers.session_before_switch.get(callback_index).cloned()
            } {
                callback_index += 1;
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
            let mut callback_index = 0;
            while let Some(handler) = {
                let registration = extension.read();
                registration.handlers.session_before_fork.get(callback_index).cloned()
            } {
                callback_index += 1;
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
            let mut callback_index = 0;
            while let Some(handler) = {
                let registration = extension.read();
                registration
                    .handlers
                    .session_before_compact
                    .get(callback_index)
                    .cloned()
            } {
                callback_index += 1;
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
            let mut callback_index = 0;
            while let Some(handler) = {
                let registration = extension.read();
                registration.handlers.session_before_tree.get(callback_index).cloned()
            } {
                callback_index += 1;
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
            let mut callback_index = 0;
            while let Some(handler) = {
                let registration = extension.read();
                registration.handlers.compaction.get(callback_index).cloned()
            } {
                callback_index += 1;
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
        let extension_path = extension.read().path.clone();
        self.emit_error(ExtensionError {
            extension_path,
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
            ExtensionEvent::SessionCompactionSource(event) => {
                let mut result = None;
                for extension in &self.extensions {
                    let mut callback_index = 0;
                    while let Some(handler) = {
                        let registration = extension.read();
                        registration
                            .handlers
                            .session_compaction_source
                            .get(callback_index)
                            .cloned()
                    } {
                        callback_index += 1;
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
                    let mut callback_index = 0;
                    while let Some(handler) = {
                        let registration = extension.read();
                        registration.handlers.$field.get(callback_index).cloned()
                    } {
                        callback_index += 1;
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
            ExtensionEvent::BackgroundCompactionStart(event) => {
                dispatch!(event, background_compaction_start, "background_compaction_start")
            }
            ExtensionEvent::BackgroundCompactionEnd(event) => {
                dispatch!(event, background_compaction_end, "background_compaction_end")
            }
            ExtensionEvent::SessionCompact(event) => dispatch!(event, session_compact, "session_compact"),
            ExtensionEvent::SessionShutdown(event) => dispatch!(event, session_shutdown, "session_shutdown"),
            ExtensionEvent::SessionTree(event) => dispatch!(event, session_tree, "session_tree"),
            ExtensionEvent::AfterProviderResponse(event) => {
                dispatch!(event, after_provider_response, "after_provider_response")
            }
            ExtensionEvent::AgentStart(event) => dispatch!(event, agent_start, "agent_start"),
            ExtensionEvent::ModelRequestStart(event) => dispatch!(event, model_request_start, "model_request_start"),
            ExtensionEvent::ModelRequestEnd(event) => dispatch!(event, model_request_end, "model_request_end"),
            ExtensionEvent::AgentEnd(event) => dispatch!(event, agent_end, "agent_end"),
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
            let mut callback_index = 0;
            while let Some(handler) = {
                let registration = extension.read();
                registration.handlers.runtime_mailbox.get(callback_index).cloned()
            } {
                callback_index += 1;
                registrations.push((extension, handler.clone()));
            }
        }
        for (extension, handler) in registrations {
            match handler.call(event.clone(), ctx.clone()).await {
                Ok(Some(result)) if result.handled => return Ok(result),
                Ok(_) => {}
                Err(error) => {
                    let path = extension.read().path.clone();
                    self.report_runtime_mailbox_handler_error(&path, &event, &error);
                    return Err(error);
                }
            }
        }
        Ok(RuntimeMailboxEventResult { handled: false })
    }

    fn report_runtime_mailbox_handler_error(
        &self,
        extension_path: &JsString,
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
            let mut callback_index = 0;
            while let Some(handler) = {
                let registration = extension.read();
                registration.handlers.message_end.get(callback_index).cloned()
            } {
                callback_index += 1;
                let current_event = MessageEndEvent {
                    message: current_message.clone(),
                };
                match handler.call(current_event.clone(), ctx.clone()).await {
                    Ok(Some(handler_result)) => {
                        let Some(message) = handler_result.message else {
                            continue;
                        };
                        if message.role() != current_message.role() {
                            let extension_path = extension.read().path.clone();
                            self.emit_error(ExtensionError {
                                extension_path,
                                event: "message_end".to_owned(),
                                error: "message_end handlers must return a message with the same role".into(),
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
            let mut callback_index = 0;
            while let Some(handler) = {
                let registration = extension.read();
                registration.handlers.tool_result.get(callback_index).cloned()
            } {
                callback_index += 1;
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
            let mut callback_index = 0;
            while let Some(handler) = {
                let registration = extension.read();
                registration.handlers.tool_call.get(callback_index).cloned()
            } {
                callback_index += 1;
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
            let mut callback_index = 0;
            while let Some(gate) = {
                let registration = extension.read();
                registration.tool_gates.get(callback_index).cloned()
            } {
                callback_index += 1;
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
            let mut callback_index = 0;
            while let Some(reviewer) = {
                let registration = extension.read();
                registration.approval_reviewers.get(callback_index).cloned()
            } {
                callback_index += 1;
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
            let mut callback_index = 0;
            while let Some(handler) = {
                let registration = extension.read();
                registration.handlers.user_bash.get(callback_index).cloned()
            } {
                callback_index += 1;
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
        let signal = ctx.signal()?;
        let mut current_messages = messages;

        for extension in &self.extensions {
            let mut callback_index = 0;
            while let Some(handler) = {
                let registration = extension.read();
                registration.handlers.context.get(callback_index).cloned()
            } {
                callback_index += 1;
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
                        if signal.as_ref().is_some_and(AbortSignal::aborted) {
                            return Err(error);
                        }
                        self.emit_handler_error(extension, "context", error);
                    }
                }
            }
        }

        Ok(current_messages)
    }

    pub async fn emit_before_provider_request(&self, payload: JsValue) -> JsValue {
        let ctx = self.create_context();
        let mut current_payload = payload;
        for extension in &self.extensions {
            let mut callback_index = 0;
            while let Some(handler) = {
                let registration = extension.read();
                registration
                    .handlers
                    .before_provider_request
                    .get(callback_index)
                    .cloned()
            } {
                callback_index += 1;
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

    pub async fn emit_before_agent_start(
        &self,
        prompt: &JsString,
        images: Option<Vec<ImageContent>>,
        system_prompt: &JsString,
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
            let mut callback_index = 0;
            while let Some(handler) = {
                let registration = extension.read();
                registration.handlers.before_agent_start.get(callback_index).cloned()
            } {
                callback_index += 1;
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

    pub async fn emit_resources_discover(
        &self,
        cwd: &JsString,
        reason: ResourcesDiscoverReason,
    ) -> ResourcesDiscoverPaths {
        let ctx = self.create_context();
        let mut result = ResourcesDiscoverPaths::default();
        for extension in &self.extensions {
            let mut callback_index = 0;
            while let Some(handler) = {
                let registration = extension.read();
                registration.handlers.resources_discover.get(callback_index).cloned()
            } {
                callback_index += 1;
                let event = ResourcesDiscoverEvent {
                    cwd: cwd.to_owned(),
                    reason,
                };
                match handler.call(event, ctx.clone()).await {
                    Ok(Some(handler_result)) => {
                        if let Some(paths) = handler_result.skill_paths.filter(|paths| !paths.is_empty()) {
                            result.skill_paths.extend(paths.into_iter().map(|path| DiscoveredPath {
                                path,
                                extension_path: extension.read().path.clone(),
                            }));
                        }
                        if let Some(paths) = handler_result.prompt_paths.filter(|paths| !paths.is_empty()) {
                            result.prompt_paths.extend(paths.into_iter().map(|path| DiscoveredPath {
                                path,
                                extension_path: extension.read().path.clone(),
                            }));
                        }
                        if let Some(paths) = handler_result.theme_paths.filter(|paths| !paths.is_empty()) {
                            result.theme_paths.extend(paths.into_iter().map(|path| DiscoveredPath {
                                path,
                                extension_path: extension.read().path.clone(),
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
        text: &JsString,
        images: Option<InputImages>,
        source: InputSource,
        streaming_behavior: Option<StreamingBehavior>,
    ) -> InputEventResult {
        let ctx = self.create_context();
        let mut current_text = text.to_owned();
        let mut current_images = images.clone();

        for extension in &self.extensions {
            let mut callback_index = 0;
            while let Some(handler) = {
                let registration = extension.read();
                registration.handlers.input.get(callback_index).cloned()
            } {
                callback_index += 1;
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
                        if let Some(images) = images {
                            current_images = Some(InputImages::new(images.as_slice().to_vec()));
                        }
                    }
                    Ok(Some(InputEventResult::Continue)) | Ok(None) => {}
                    Err(error) => self.emit_handler_error(extension, "input", error),
                }
            }
        }

        let images_changed = match (&current_images, &images) {
            (Some(current), Some(original)) => !current.ptr_eq(original),
            (None, None) => false,
            _ => true,
        };
        if &current_text != text || images_changed {
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
    signal: &AbortSignal,
) -> Result<Option<ContextEventResult>, HandlerError> {
    if signal.aborted() {
        return Err(HandlerError::new("Agent run aborted"));
    }
    tokio::select! {
        result = future => result,
        _ = signal.wait() => Err(HandlerError::new("Agent run aborted")),
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

fn replace_tool_input(target: &mut JsObject, source: &JsObject) {
    target.clear();
    for (key, value) in source {
        target.insert(key.clone(), value.clone());
    }
}
