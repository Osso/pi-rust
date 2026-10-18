//! Port of `packages/coding-agent/src/core/agent-session-runtime.ts`.
//!
//! Extension `session_before_switch` / `session_before_fork` hooks and
//! `emitSessionShutdownEvent` are recorded as gaps until the AgentSession
//! extension wiring lands.

use std::sync::{Arc, Mutex};

use pi_ai::request::BoxFuture;
use pi_ai::types::Model;

use crate::core::agent_session::AgentSession;
use crate::core::agent_session_services::{
    AgentSessionRuntimeDiagnostic, AgentSessionServices, create_agent_session_from_services,
    create_agent_session_services,
};
use crate::core::sdk::CreateAgentSessionResult;
use crate::core::session_control_db::assert_main_session_runtime_available;
use crate::core::session_cwd::assert_session_cwd_exists;
use crate::core::session_errors::SessionImportFileNotFoundError;
use crate::core::session_manager::SessionManager;
use crate::utils::paths::{PathInputOptions, resolve_path};

/// Result returned by runtime creation.
pub struct CreateAgentSessionRuntimeResult {
    pub session: AgentSession,
    pub services: AgentSessionServices,
    pub diagnostics: Vec<AgentSessionRuntimeDiagnostic>,
    pub model_fallback_message: Option<String>,
}

/// Options passed to a runtime factory.
pub struct CreateAgentSessionRuntimeOptions {
    pub cwd: String,
    pub agent_dir: String,
    pub session_manager: SessionManager,
    pub session_start_reason: String,
    pub previous_session_file: Option<String>,
}

/// Runtime factory reused for `/new`, `/resume`, `/fork`, and imports.
pub type CreateAgentSessionRuntimeFactory = Arc<
    dyn Fn(CreateAgentSessionRuntimeOptions) -> BoxFuture<'static, Result<CreateAgentSessionRuntimeResult, String>>
        + Send
        + Sync,
>;

/// Callback receiving the replacement session.
pub type RebindSessionHandler = Arc<dyn Fn(AgentSession) -> BoxFuture<'static, ()> + Send + Sync>;
/// Callback invoked before the outgoing session is invalidated.
pub type BeforeSessionInvalidateHandler = Arc<dyn Fn() + Send + Sync>;
/// Callback invoked before a process restart.
pub type BeforeProcessRestartHandler = Arc<dyn Fn() -> BoxFuture<'static, ()> + Send + Sync>;
/// Replacement for `restartCurrentProcess`.
pub type ProcessRestarter = Arc<dyn Fn(ProcessRestartOptions) -> BoxFuture<'static, Result<(), String>> + Send + Sync>;

/// Arguments for a process restart.
#[derive(Debug, Clone, Default)]
pub struct ProcessRestartOptions {
    pub session_file: String,
    pub prompt: Option<String>,
}

/// `fork` position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForkPosition {
    Before,
    At,
}

/// Result of a session switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionTransitionResult {
    pub cancelled: bool,
}

/// Result of `fork`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForkResult {
    pub cancelled: bool,
    pub selected_text: Option<String>,
}

/// Session setup callback used by `newSession`.
pub type SessionSetupCallback = Arc<dyn Fn(&mut SessionManager) -> BoxFuture<'static, ()> + Send + Sync>;

/// Options for `newSession`.
pub struct NewSessionOptions {
    pub parent_session: Option<String>,
    pub setup: Option<SessionSetupCallback>,
    pub with_session: Option<Arc<dyn Fn(AgentSession) -> BoxFuture<'static, ()> + Send + Sync>>,
}

/// Owns the current AgentSession plus its cwd-bound services.
pub struct AgentSessionRuntime {
    session: AgentSession,
    services: AgentSessionServices,
    create_runtime: CreateAgentSessionRuntimeFactory,
    diagnostics: Vec<AgentSessionRuntimeDiagnostic>,
    model_fallback_message: Option<String>,
    rebind_session: Option<RebindSessionHandler>,
    before_session_invalidate: Option<BeforeSessionInvalidateHandler>,
    before_process_restart: Option<BeforeProcessRestartHandler>,
    process_restarter: ProcessRestarter,
    prepared_tool_result_relocation: Mutex<Option<PreparedCwdRelocation>>,
}

struct PreparedCwdRelocation {
    previous_session_file: Option<String>,
    session_manager: SessionManager,
}

impl AgentSessionRuntime {
    pub fn new(
        session: AgentSession,
        services: AgentSessionServices,
        create_runtime: CreateAgentSessionRuntimeFactory,
        diagnostics: Vec<AgentSessionRuntimeDiagnostic>,
        model_fallback_message: Option<String>,
    ) -> Self {
        Self {
            session,
            services,
            create_runtime,
            diagnostics,
            model_fallback_message,
            rebind_session: None,
            before_session_invalidate: None,
            before_process_restart: None,
            process_restarter: Arc::new(|_| {
                Box::pin(async { Err("Process restart is not available in this session mode".to_owned()) })
            }),
            prepared_tool_result_relocation: Mutex::new(None),
        }
    }

    pub fn services(&self) -> &AgentSessionServices {
        &self.services
    }

    pub fn session(&self) -> &AgentSession {
        &self.session
    }

    pub fn cwd(&self) -> &str {
        &self.services.cwd
    }

    pub fn diagnostics(&self) -> &[AgentSessionRuntimeDiagnostic] {
        &self.diagnostics
    }

    pub fn model_fallback_message(&self) -> Option<&str> {
        self.model_fallback_message.as_deref()
    }

    pub fn set_rebind_session(&mut self, handler: Option<RebindSessionHandler>) {
        self.rebind_session = handler;
    }

    pub fn set_before_session_invalidate(&mut self, handler: Option<BeforeSessionInvalidateHandler>) {
        self.before_session_invalidate = handler;
    }

    pub fn set_before_process_restart(&mut self, handler: Option<BeforeProcessRestartHandler>) {
        self.before_process_restart = handler;
    }

    pub fn set_process_restarter(&mut self, restarter: ProcessRestarter) {
        self.process_restarter = restarter;
    }

    fn apply(&mut self, result: CreateAgentSessionRuntimeResult) {
        self.session = result.session;
        self.services = result.services;
        self.diagnostics = result.diagnostics;
        self.model_fallback_message = result.model_fallback_message;
    }

    async fn finish_session_replacement(
        &self,
        with_session: Option<Arc<dyn Fn(AgentSession) -> BoxFuture<'static, ()> + Send + Sync>>,
    ) {
        if let Some(rebind) = &self.rebind_session {
            rebind(self.session.clone()).await;
        }
        if let Some(with_session) = with_session {
            with_session(self.session.clone()).await;
        }
    }

    async fn teardown_current(&self, _reason: &str, _target_session_file: Option<String>) {
        let _ = self.session.abort().await;
        if let Some(handler) = &self.before_session_invalidate {
            handler();
        }
        self.session.dispose();
    }

    async fn create_runtime(
        &self,
        cwd: &str,
        session_manager: SessionManager,
        reason: &str,
        previous_session_file: Option<String>,
    ) -> Result<CreateAgentSessionRuntimeResult, String> {
        (self.create_runtime)(CreateAgentSessionRuntimeOptions {
            cwd: cwd.to_owned(),
            agent_dir: self.services.agent_dir.clone(),
            session_manager,
            session_start_reason: reason.to_owned(),
            previous_session_file,
        })
        .await
    }

    /// Switches to an existing session file.
    pub async fn switch_session(
        &mut self,
        session_path: &str,
        cwd_override: Option<&str>,
        with_session: Option<Arc<dyn Fn(AgentSession) -> BoxFuture<'static, ()> + Send + Sync>>,
    ) -> Result<SessionTransitionResult, String> {
        let previous_session_file = self.session.session_file();
        let session_manager =
            SessionManager::open(session_path, None, cwd_override).map_err(|error| error.to_string())?;
        assert_session_cwd_exists(&session_manager, self.cwd()).map_err(|error| error.to_string())?;
        let control_db_path = self.services_control_db_path()?;
        assert_main_session_runtime_available(&control_db_path, session_manager.get_session_id())
            .map_err(|error| error.to_string())?;
        let cwd = session_manager.get_cwd().to_owned();
        self.teardown_current("resume", session_manager.get_session_file().map(str::to_owned))
            .await;
        let result = self
            .create_runtime(&cwd, session_manager, "resume", previous_session_file)
            .await?;
        self.apply(result);
        self.finish_session_replacement(with_session).await;
        Ok(SessionTransitionResult { cancelled: false })
    }

    fn services_control_db_path(&self) -> Result<String, String> {
        self.session
            .control_db_path()
            .ok_or_else(|| "Current session has no control database path".to_owned())
    }

    fn resolve_existing_directory(&self, input_path: &str) -> Result<String, String> {
        let resolved =
            resolve_path(input_path, self.cwd(), &PathInputOptions::default()).map_err(|error| error.to_string())?;
        let path = std::path::Path::new(&resolved);
        if !path.exists() {
            return Err(format!("Directory does not exist: {resolved}"));
        }
        if !path.is_dir() {
            return Err(format!("Not a directory: {resolved}"));
        }
        Ok(resolved)
    }

    fn prepare_relocation(&self, target_cwd: &str) -> Result<PreparedCwdRelocation, String> {
        let resolved_target = self.resolve_existing_directory(target_cwd)?;
        let previous_cwd = self.cwd().to_owned();
        let previous_session_file = self.session.session_file();
        let mut session_manager = self.session.session_manager().clone();
        session_manager
            .relocate(&resolved_target, Some(&self.services.agent_dir))
            .map_err(|error| error.to_string())?;
        let mut details = serde_json::Map::new();
        details.insert(
            "previousCwd".to_owned(),
            serde_json::Value::String(previous_cwd.clone()),
        );
        details.insert("cwd".to_owned(), serde_json::Value::String(resolved_target.clone()));
        session_manager
            .append_custom_message_entry(
                "cwd_changed",
                pi_agent_core::harness::messages::CustomMessageContent::Text(format!(
                    "Working directory changed from {previous_cwd} to {resolved_target}."
                )),
                true,
                Some(serde_json::Value::Object(details)),
            )
            .map_err(|error| error.to_string())?;
        Ok(PreparedCwdRelocation {
            previous_session_file,
            session_manager,
        })
    }

    async fn activate_relocation(&mut self, prepared: PreparedCwdRelocation) -> Result<(), String> {
        let cwd = prepared.session_manager.get_cwd().to_owned();
        let session_file = prepared.session_manager.get_session_file().map(str::to_owned);
        self.teardown_current("resume", session_file).await;
        let result = self
            .create_runtime(&cwd, prepared.session_manager, "resume", prepared.previous_session_file)
            .await?;
        self.apply(result);
        self.finish_session_replacement(None).await;
        Ok(())
    }

    /// Prepares a tool-result working-directory relocation.
    pub async fn prepare_tool_result_relocation(&self, target_cwd: &str) -> Result<(), String> {
        let mut slot = self
            .prepared_tool_result_relocation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if slot.is_some() {
            return Err("A tool-result working-directory relocation is already prepared".to_owned());
        }
        *slot = Some(self.prepare_relocation(target_cwd)?);
        Ok(())
    }

    /// Activates a prepared tool-result relocation and continues the session.
    pub async fn activate_tool_result_relocation(&mut self) -> Result<(), String> {
        let prepared = self
            .prepared_tool_result_relocation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        let Some(prepared) = prepared else {
            return Ok(());
        };
        self.activate_relocation(prepared).await?;
        self.session
            .continue_run()
            .await
            .map_err(|error| error.message().to_owned())
    }

    /// Relocates the session to a new working directory.
    pub async fn relocate(&mut self, target_cwd: &str) -> Result<(), String> {
        let prepared = self.prepare_relocation(target_cwd)?;
        self.activate_relocation(prepared).await
    }

    /// Restarts the session, optionally as a process restart.
    pub async fn restart(&mut self, notice: Option<&str>, process: bool) -> Result<(), String> {
        let previous_session_file = self.session.session_file();
        let current_session_file = self.session.session_file();
        let is_persisted = self.session.session_manager().is_persisted();

        if let (true, true, Some(session_file)) = (process, is_persisted, current_session_file.clone()) {
            self.session
                .session_manager()
                .persist_for_recovery()
                .map_err(|error| error.to_string())?;
            if let Some(handler) = &self.before_process_restart {
                handler().await;
            }
            self.teardown_current("restart", Some(session_file.clone())).await;
            return (self.process_restarter)(ProcessRestartOptions {
                session_file,
                prompt: notice.map(str::to_owned),
            })
            .await;
        }

        let session_manager = if is_persisted && current_session_file.is_some() {
            let file = current_session_file.as_deref().expect("checked session file");
            let (dir, cwd) = {
                let manager = self.session.session_manager();
                (manager.get_session_dir().to_owned(), manager.get_cwd().to_owned())
            };
            SessionManager::open(file, Some(&dir), Some(&cwd)).map_err(|error| error.to_string())?
        } else {
            self.session.session_manager().clone()
        };
        let mut session_manager = session_manager;
        if let Some(notice) = notice {
            session_manager
                .append_custom_message_entry(
                    "self_restart",
                    pi_agent_core::harness::messages::CustomMessageContent::Text(notice.to_owned()),
                    true,
                    None,
                )
                .map_err(|error| error.to_string())?;
        }
        let cwd = session_manager.get_cwd().to_owned();
        self.teardown_current("restart", current_session_file).await;
        let result = self
            .create_runtime(&cwd, session_manager, "restart", previous_session_file)
            .await?;
        self.apply(result);
        self.finish_session_replacement(None).await;
        Ok(())
    }

    /// Starts a new session.
    pub async fn new_session(&mut self, options: NewSessionOptions) -> Result<SessionTransitionResult, String> {
        let previous_session_file = self.session.session_file();
        let (is_persisted, session_dir) = {
            let manager = self.session.session_manager();
            (manager.is_persisted(), manager.get_session_dir().to_owned())
        };
        let mut session_manager = if is_persisted {
            SessionManager::create(self.cwd(), Some(&session_dir), None).map_err(|error| error.to_string())?
        } else {
            SessionManager::in_memory(Some(self.cwd()), None).map_err(|error| error.to_string())?
        };
        if let Some(parent_session) = &options.parent_session {
            session_manager
                .new_session(Some(crate::core::session_manager::NewSessionOptions {
                    id: None,
                    parent_session: Some(parent_session.clone()),
                    is_subagent: false,
                    subagent_name: None,
                }))
                .map_err(|error| error.to_string())?;
        }
        let cwd = self.cwd().to_owned();
        self.teardown_current("new", session_manager.get_session_file().map(str::to_owned))
            .await;
        let result = self
            .create_runtime(&cwd, session_manager, "new", previous_session_file)
            .await?;
        self.apply(result);
        if let Some(setup) = options.setup {
            let handle = self.session.session_manager_handle();
            let mut manager = {
                let guard = handle.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                guard.clone()
            };
            setup(&mut manager).await;
            let messages = manager.build_session_context().messages;
            *handle.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = manager;
            self.session.agent().set_messages(messages);
        }
        self.finish_session_replacement(options.with_session).await;
        Ok(SessionTransitionResult { cancelled: false })
    }

    /// Forks the current session at an entry.
    pub async fn fork(
        &mut self,
        entry_id: &str,
        position: ForkPosition,
        with_session: Option<Arc<dyn Fn(AgentSession) -> BoxFuture<'static, ()> + Send + Sync>>,
    ) -> Result<ForkResult, String> {
        let selected_entry = self
            .session
            .session_manager()
            .get_entry(entry_id)
            .cloned()
            .ok_or_else(|| "Invalid entry ID for forking".to_owned())?;
        let mut selected_text = None;
        let target_leaf_id = if position == ForkPosition::At {
            selected_entry.id().map(str::to_owned)
        } else {
            let is_user_message = selected_entry.as_message().is_some_and(|entry| {
                matches!(
                    entry.message.as_ref(),
                    pi_agent_core::agent::types::AgentMessage::User(_)
                )
            });
            if !is_user_message {
                return Err("Invalid entry ID for forking".to_owned());
            }
            if let Some(entry) = selected_entry.as_message()
                && let pi_agent_core::agent::types::AgentMessage::User(user) = entry.message.as_ref()
            {
                selected_text = Some(user_content_text(&user.content));
            }
            selected_entry.parent_id().map(str::to_owned)
        };

        let previous_session_file = self.session.session_file();
        let is_persisted = self.session.session_manager().is_persisted();
        if is_persisted {
            let current_session_file = self
                .session
                .session_file()
                .ok_or_else(|| "Persisted session is missing a session file".to_owned())?;
            let session_dir = self.session.session_manager().get_session_dir().to_owned();
            let Some(target_leaf_id) = target_leaf_id else {
                let mut session_manager =
                    SessionManager::create(self.cwd(), Some(&session_dir), None).map_err(|error| error.to_string())?;
                session_manager
                    .new_session(Some(crate::core::session_manager::NewSessionOptions {
                        id: None,
                        parent_session: Some(current_session_file),
                        is_subagent: false,
                        subagent_name: None,
                    }))
                    .map_err(|error| error.to_string())?;
                let cwd = self.cwd().to_owned();
                self.teardown_current("fork", session_manager.get_session_file().map(str::to_owned))
                    .await;
                let result = self
                    .create_runtime(&cwd, session_manager, "fork", previous_session_file)
                    .await?;
                self.apply(result);
                self.finish_session_replacement(with_session).await;
                return Ok(ForkResult {
                    cancelled: false,
                    selected_text,
                });
            };
            if !std::path::Path::new(&current_session_file).exists() {
                return Err("This session has not been saved yet. Wait for the first assistant response before cloning or forking it.".to_owned());
            }
            let mut session_manager = SessionManager::open(&current_session_file, Some(&session_dir), None)
                .map_err(|error| error.to_string())?;
            let forked_session_path = session_manager
                .create_branched_session(&target_leaf_id)
                .map_err(|error| error.to_string())?;
            if forked_session_path.is_none() {
                return Err("Failed to create forked session".to_owned());
            }
            let cwd = session_manager.get_cwd().to_owned();
            self.teardown_current("fork", session_manager.get_session_file().map(str::to_owned))
                .await;
            let result = self
                .create_runtime(&cwd, session_manager, "fork", previous_session_file)
                .await?;
            self.apply(result);
            self.finish_session_replacement(with_session).await;
            return Ok(ForkResult {
                cancelled: false,
                selected_text,
            });
        }

        {
            let mut manager = self.session.session_manager();
            match &target_leaf_id {
                None => {
                    manager
                        .new_session(Some(crate::core::session_manager::NewSessionOptions {
                            id: None,
                            parent_session: self.session.session_file(),
                            is_subagent: false,
                            subagent_name: None,
                        }))
                        .map_err(|error| error.to_string())?;
                }
                Some(target_leaf_id) => {
                    manager
                        .create_branched_session(target_leaf_id)
                        .map_err(|error| error.to_string())?;
                }
            }
        }
        let session_manager = self.session.session_manager().clone();
        let cwd = self.cwd().to_owned();
        self.teardown_current("fork", session_manager.get_session_file().map(str::to_owned))
            .await;
        let result = self
            .create_runtime(&cwd, session_manager, "fork", previous_session_file)
            .await?;
        self.apply(result);
        self.finish_session_replacement(with_session).await;
        Ok(ForkResult {
            cancelled: false,
            selected_text,
        })
    }

    /// Imports a session JSONL file and switches to it.
    pub async fn import_from_jsonl(
        &mut self,
        input_path: &str,
        cwd_override: Option<&str>,
    ) -> Result<SessionTransitionResult, String> {
        let resolved = resolve_path(input_path, &process_cwd(), &PathInputOptions::default())
            .map_err(|error| error.to_string())?;
        if !std::path::Path::new(&resolved).exists() {
            return Err(SessionImportFileNotFoundError::new(resolved).to_string());
        }
        let session_dir = self.session.session_manager().get_session_dir().to_owned();
        if !std::path::Path::new(&session_dir).exists() {
            std::fs::create_dir_all(&session_dir).map_err(|error| error.to_string())?;
        }
        let file_name = std::path::Path::new(&resolved)
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        let destination = std::path::Path::new(&session_dir).join(file_name);
        let destination_path = destination.display().to_string();
        let previous_session_file = self.session.session_file();
        if std::fs::canonicalize(&destination_path).ok().as_deref() != std::fs::canonicalize(&resolved).ok().as_deref()
        {
            std::fs::copy(&resolved, &destination_path).map_err(|error| error.to_string())?;
        }
        let session_manager = SessionManager::open(&destination_path, Some(&session_dir), cwd_override)
            .map_err(|error| error.to_string())?;
        assert_session_cwd_exists(&session_manager, self.cwd()).map_err(|error| error.to_string())?;
        let cwd = session_manager.get_cwd().to_owned();
        self.teardown_current("resume", session_manager.get_session_file().map(str::to_owned))
            .await;
        let result = self
            .create_runtime(&cwd, session_manager, "resume", previous_session_file)
            .await?;
        self.apply(result);
        self.finish_session_replacement(None).await;
        Ok(SessionTransitionResult { cancelled: false })
    }

    /// Disposes the runtime and its session.
    pub async fn dispose(&mut self) {
        if let Some(handler) = &self.before_session_invalidate {
            handler();
        }
        self.session.dispose();
    }
}

/// Creates the initial runtime from a factory and initial session target.
pub async fn create_agent_session_runtime(
    create_runtime: CreateAgentSessionRuntimeFactory,
    cwd: &str,
    agent_dir: &str,
    session_manager: SessionManager,
) -> Result<AgentSessionRuntime, String> {
    assert_session_cwd_exists(&session_manager, cwd).map_err(|error| error.to_string())?;
    let result = create_runtime(CreateAgentSessionRuntimeOptions {
        cwd: cwd.to_owned(),
        agent_dir: agent_dir.to_owned(),
        session_manager,
        session_start_reason: "startup".to_owned(),
        previous_session_file: None,
    })
    .await?;
    Ok(AgentSessionRuntime::new(
        result.session,
        result.services,
        create_runtime,
        result.diagnostics,
        result.model_fallback_message,
    ))
}

/// Convenience factory that creates services and a session for one cwd.
pub fn default_runtime_factory(options: DefaultRuntimeFactoryOptions) -> CreateAgentSessionRuntimeFactory {
    Arc::new(move |request| {
        let options = options.clone();
        Box::pin(async move {
            let services =
                create_agent_session_services(crate::core::agent_session_services::CreateAgentSessionServicesOptions {
                    cwd: request.cwd.clone(),
                    agent_dir: Some(request.agent_dir.clone()),
                    settings_manager: None,
                    model_runtime: None,
                    model_runtime_signal: None,
                    extension_flag_values: None,
                    resource_loader_options: None,
                    resource_loader: Some(options.resource_loader),
                    resource_loader_reload_options: None,
                })
                .await?;
            let result: CreateAgentSessionResult = create_agent_session_from_services(
                crate::core::agent_session_services::CreateAgentSessionFromServicesOptions {
                    services: AgentSessionServices {
                        cwd: services.cwd.clone(),
                        agent_dir: services.agent_dir.clone(),
                        model_runtime: services.model_runtime.clone(),
                        settings_manager: Arc::clone(&services.settings_manager),
                        resource_loader: Arc::clone(&services.resource_loader),
                        diagnostics: services.diagnostics.clone(),
                    },
                    session_manager: request.session_manager,
                    model: options.model.clone(),
                    thinking_level: options.thinking_level,
                    scoped_models: options.scoped_models.clone(),
                    tools: options.tools.clone(),
                    exclude_tools: options.exclude_tools.clone(),
                    no_tools: options.no_tools,
                    permission_prompt_tool: options.permission_prompt_tool.clone(),
                    multi_agent_runtime_role: None,
                    multi_agent_execution_capability: None,
                    custom_tools: options.custom_tools.clone(),
                    base_tools: options.base_tools.clone(),
                },
            )
            .await?;
            let services = AgentSessionServices {
                cwd: services.cwd,
                agent_dir: services.agent_dir,
                model_runtime: services.model_runtime,
                settings_manager: services.settings_manager,
                resource_loader: services.resource_loader,
                diagnostics: services.diagnostics,
            };
            Ok(CreateAgentSessionRuntimeResult {
                session: result.session,
                services,
                diagnostics: Vec::new(),
                model_fallback_message: result.model_fallback_message,
            })
        })
    })
}

/// Options for [`default_runtime_factory`].
#[derive(Clone)]
pub struct DefaultRuntimeFactoryOptions {
    pub resource_loader: Arc<dyn crate::core::resource_loader::ResourceLoader>,
    pub model: Option<Model>,
    pub thinking_level: Option<pi_agent_core::harness::types::ThinkingLevel>,
    pub scoped_models: Vec<crate::core::model_resolver::ScopedModel>,
    pub tools: Option<Vec<String>>,
    pub exclude_tools: Option<Vec<String>>,
    pub no_tools: Option<crate::core::sdk::NoTools>,
    pub permission_prompt_tool: Option<String>,
    pub custom_tools: Vec<crate::core::agent_session::ToolDefinition>,
    pub base_tools: Vec<Arc<dyn pi_agent_core::agent::types::AgentTool>>,
}

fn user_content_text(content: &pi_ai::types::UserContent) -> String {
    match content {
        pi_ai::types::UserContent::Text(text) => text.clone(),
        pi_ai::types::UserContent::Blocks(blocks) => blocks
            .iter()
            .filter_map(|block| match block {
                pi_ai::types::UserContentBlock::Text(text) => Some(text.text.as_str()),
                pi_ai::types::UserContentBlock::Image(_) => None,
            })
            .collect(),
    }
}

fn process_cwd() -> String {
    std::env::current_dir()
        .map(|path| path.display().to_string())
        .unwrap_or_default()
}
