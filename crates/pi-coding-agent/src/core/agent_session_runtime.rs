//! Port of `packages/coding-agent/src/core/agent-session-runtime.ts`.

use std::sync::{Arc, Mutex};

use pi_ai::request::BoxFuture;
use pi_ai::types::Model;
use pi_js_compat::js_json::{JsObject, JsString, JsValue, encode_node_utf8};

use crate::core::agent_session::AgentSession;
use crate::core::agent_session_services::{
    AgentSessionRuntimeDiagnostic, AgentSessionServices, create_agent_session_from_services,
    create_agent_session_services,
};
use crate::core::extensions::emit_session_shutdown_event;
use crate::core::extensions::runner::EmitResult;
use crate::core::extensions::types::HandlerError;
use crate::core::extensions::types::{
    ExtensionEvent, ForkPosition as ExtensionForkPosition, SessionBeforeForkEvent, SessionBeforeSwitchEvent,
    SessionShutdownEvent, SessionShutdownReason, SessionSwitchReason,
};
use crate::core::sdk::CreateAgentSessionResult;
use crate::core::session_control_db::assert_main_session_runtime_available;
use crate::core::session_cwd::assert_session_cwd_exists;
use crate::core::session_errors::SessionImportFileNotFoundError;
use crate::core::session_manager::SessionManager;
use crate::utils::paths::{PathInputOptions, process_cwd, resolve_path};

/// Result returned by runtime creation.
pub struct CreateAgentSessionRuntimeResult {
    pub session: AgentSession,
    pub services: AgentSessionServices,
    pub diagnostics: Vec<AgentSessionRuntimeDiagnostic>,
    pub model_fallback_message: Option<JsString>,
}

/// Options passed to a runtime factory.
pub struct CreateAgentSessionRuntimeOptions {
    pub no_supervisor: Option<bool>,
    pub cwd: JsString,
    pub agent_dir: JsString,
    pub session_manager: SessionManager,
    pub session_start_reason: String,
    pub previous_session_file: Option<JsString>,
}

/// Runtime factory reused for `/new`, `/resume`, `/fork`, and imports.
pub type CreateAgentSessionRuntimeFactory = Arc<
    dyn Fn(CreateAgentSessionRuntimeOptions) -> BoxFuture<'static, Result<CreateAgentSessionRuntimeResult, JsString>>
        + Send
        + Sync,
>;

/// Callback receiving the replacement session.
pub type RebindSessionHandler = Arc<dyn Fn(AgentSession) -> BoxFuture<'static, Result<(), JsString>> + Send + Sync>;
/// Callback invoked before the outgoing session is invalidated.
pub type BeforeSessionInvalidateHandler = Arc<dyn Fn() + Send + Sync>;
/// Callback invoked before a process restart.
pub type BeforeProcessRestartHandler = Arc<dyn Fn() -> BoxFuture<'static, ()> + Send + Sync>;
/// Replacement for `restartCurrentProcess`.
pub type ProcessRestarter =
    Arc<dyn Fn(ProcessRestartOptions) -> BoxFuture<'static, Result<(), JsString>> + Send + Sync>;

/// Arguments for a process restart.
#[derive(Debug, Clone, Default)]
pub struct ProcessRestartOptions {
    pub session_file: JsString,
    pub prompt: Option<pi_js_compat::js_json::JsString>,
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
    pub selected_text: Option<pi_js_compat::js_json::JsString>,
}

/// Session setup callback used by `newSession`.
pub type SessionSetupCallback = Arc<dyn Fn(&mut SessionManager) -> BoxFuture<'static, ()> + Send + Sync>;
/// Callback receiving the replacement session context.
pub type ReplacedSessionCallback =
    Arc<dyn Fn(crate::core::extensions::types::ReplacedSessionContext) -> BoxFuture<'static, ()> + Send + Sync>;

/// Options for `newSession`.
#[derive(Default)]
pub struct NewSessionOptions {
    pub parent_session: Option<JsString>,
    pub setup: Option<SessionSetupCallback>,
    pub with_session: Option<ReplacedSessionCallback>,
}

/// Owns the current AgentSession plus its cwd-bound services.
pub struct AgentSessionRuntime {
    session: AgentSession,
    services: AgentSessionServices,
    create_runtime: CreateAgentSessionRuntimeFactory,
    diagnostics: Vec<AgentSessionRuntimeDiagnostic>,
    model_fallback_message: Option<JsString>,
    rebind_session: Option<RebindSessionHandler>,
    before_session_invalidate: Option<BeforeSessionInvalidateHandler>,
    before_process_restart: Option<BeforeProcessRestartHandler>,
    process_restarter: ProcessRestarter,
    prepared_tool_result_relocation: Mutex<Option<PreparedCwdRelocation>>,
    shared: Mutex<Option<std::sync::Weak<tokio::sync::Mutex<AgentSessionRuntime>>>>,
}

struct PreparedCwdRelocation {
    previous_session_file: Option<JsString>,
    session_manager: SessionManager,
}

impl AgentSessionRuntime {
    pub fn new(
        session: AgentSession,
        services: AgentSessionServices,
        create_runtime: CreateAgentSessionRuntimeFactory,
        diagnostics: Vec<AgentSessionRuntimeDiagnostic>,
        model_fallback_message: Option<JsString>,
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
            process_restarter: Arc::new(|options| {
                Box::pin(async move {
                    let target = crate::core::self_restart::RestartTarget {
                        argv: std::env::args().collect(),
                        ..Default::default()
                    };
                    let request = crate::core::self_restart::SelfRestartRequest {
                        session_file: options.session_file,
                        prompt: options.prompt,
                        old_pid: None,
                    };
                    match crate::core::self_restart::spawn_self_restart(&request, &target, false) {
                        Ok(code) => std::process::exit(code),
                        Err(error) => Err(error.to_string().into()),
                    }
                })
            }),
            prepared_tool_result_relocation: Mutex::new(None),
            shared: Mutex::new(None),
        }
    }

    pub fn services(&self) -> &AgentSessionServices {
        &self.services
    }

    pub fn session(&self) -> &AgentSession {
        &self.session
    }

    pub fn cwd(&self) -> &JsString {
        &self.services.cwd
    }

    pub fn diagnostics(&self) -> &[AgentSessionRuntimeDiagnostic] {
        &self.diagnostics
    }

    pub fn model_fallback_message(&self) -> Option<&JsString> {
        self.model_fallback_message.as_ref()
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

    /// Binds the runtime-owned relocation handlers on the session's extension runner.
    ///
    /// The TypeScript constructor calls `bindSessionRuntimeActions`; Rust needs the
    /// runtime's shared handle first, so [`share_agent_session_runtime`] wraps and binds
    /// a freshly created runtime. Rebindings after session replacement reuse the stored
    /// handle through [`Self::apply`].
    pub async fn bind_session_runtime_actions(shared: &Arc<tokio::sync::Mutex<AgentSessionRuntime>>) {
        let weak = Arc::downgrade(shared);
        let session = {
            let runtime = shared.lock().await;
            *runtime.shared.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(weak.clone());
            runtime.session.clone()
        };
        Self::bind_actions(&session, &weak);
    }

    fn bind_actions(session: &AgentSession, weak: &SharedRuntimeWeak) {
        session.bind_session_runtime_relocation(
            build_relocate_handler(weak),
            build_prepare_tool_result_relocation_handler(weak),
            build_activate_tool_result_relocation_handler(weak),
        );
    }

    fn apply(&mut self, result: CreateAgentSessionRuntimeResult) {
        self.session = result.session;
        self.services = result.services;
        self.diagnostics = result.diagnostics;
        self.model_fallback_message = result.model_fallback_message;
        let shared = match self.shared.get_mut() {
            Ok(slot) => slot.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        };
        if let Some(weak) = shared {
            Self::bind_actions(&self.session, &weak);
        }
    }

    async fn finish_session_replacement(&self, with_session: Option<ReplacedSessionCallback>) -> Result<(), JsString> {
        if let Some(rebind) = &self.rebind_session {
            rebind(self.session.clone()).await?;
        }
        if let Some(with_session) = with_session {
            with_session(self.session.create_replaced_session_context()).await;
        }
        Ok(())
    }

    async fn teardown_current(&self, reason: SessionShutdownReason, target_session_file: Option<JsString>) {
        self.teardown_current_with(reason, target_session_file, true).await;
    }

    async fn teardown_current_with(
        &self,
        reason: SessionShutdownReason,
        target_session_file: Option<JsString>,
        settle_active_response: bool,
    ) {
        if settle_active_response {
            let _ = self.session.abort().await;
        }
        let runner = self.session.extension_runner();
        emit_session_shutdown_event(
            &runner,
            SessionShutdownEvent {
                reason,
                target_session_file,
            },
        )
        .await;
        if let Some(handler) = &self.before_session_invalidate {
            handler();
        }
        self.session.dispose();
    }

    async fn emit_before_switch(&self, reason: SessionSwitchReason, target_session_file: Option<JsString>) -> bool {
        let runner = self.session.extension_runner();
        if !runner.has_handlers("session_before_switch") {
            return false;
        }
        let mut event = ExtensionEvent::SessionBeforeSwitch(SessionBeforeSwitchEvent {
            reason,
            target_session_file,
        });
        matches!(
            runner.emit(&mut event).await,
            EmitResult::SessionBeforeSwitch(result) if result.cancel == Some(true)
        )
    }

    async fn emit_before_fork(&self, entry_id: &JsString, position: ExtensionForkPosition) -> bool {
        let runner = self.session.extension_runner();
        if !runner.has_handlers("session_before_fork") {
            return false;
        }
        let mut event = ExtensionEvent::SessionBeforeFork(SessionBeforeForkEvent {
            entry_id: entry_id.to_owned(),
            position,
        });
        matches!(
            runner.emit(&mut event).await,
            EmitResult::SessionBeforeFork(result) if result.cancel == Some(true)
        )
    }

    async fn create_runtime(
        &self,
        cwd: &JsString,
        session_manager: SessionManager,
        reason: &str,
        previous_session_file: Option<JsString>,
    ) -> Result<CreateAgentSessionRuntimeResult, JsString> {
        (self.create_runtime)(CreateAgentSessionRuntimeOptions {
            no_supervisor: Some(self.session.no_supervisor()),
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
        session_path: &JsString,
        cwd_override: Option<&JsString>,
        with_session: Option<ReplacedSessionCallback>,
    ) -> Result<SessionTransitionResult, JsString> {
        if self.session.no_supervisor() {
            return Err("Session resume is unavailable in standalone worker mode".into());
        }
        if self
            .emit_before_switch(SessionSwitchReason::Resume, Some(session_path.to_owned()))
            .await
        {
            return Ok(SessionTransitionResult { cancelled: true });
        }
        let previous_session_file = self.session.session_file();
        let session_manager =
            SessionManager::open(session_path, None, cwd_override).map_err(|error| error.message().clone())?;
        assert_session_cwd_exists(&session_manager, self.cwd()).map_err(|error| error.message().clone())?;
        let control_db_path = self.services_control_db_path()?;
        assert_main_session_runtime_available(&control_db_path, session_manager.get_session_id())
            .map_err(|error| error.message().clone())?;
        let cwd = session_manager.get_cwd().to_owned();
        self.teardown_current(
            SessionShutdownReason::Resume,
            session_manager.get_session_file().cloned(),
        )
        .await;
        let result = self
            .create_runtime(&cwd, session_manager, "resume", previous_session_file)
            .await?;
        self.apply(result);
        self.finish_session_replacement(with_session).await?;
        Ok(SessionTransitionResult { cancelled: false })
    }

    fn services_control_db_path(&self) -> Result<JsString, JsString> {
        self.session
            .control_db_path()
            .ok_or_else(|| "Current session has no control database path".into())
    }

    fn resolve_existing_directory(&self, input_path: &JsString) -> Result<JsString, JsString> {
        let resolved = resolve_path(input_path, self.cwd(), &PathInputOptions::default())
            .map_err(|error| error.message().clone())?;
        let encoded = encode_node_utf8(&resolved);
        let path = std::path::Path::new(&encoded);
        if !path.exists() {
            return Err(JsString::from("Directory does not exist: ") + &resolved);
        }
        if !path.is_dir() {
            return Err(JsString::from("Not a directory: ") + &resolved);
        }
        Ok(resolved)
    }

    fn prepare_relocation(&self, target_cwd: &JsString) -> Result<PreparedCwdRelocation, JsString> {
        let resolved_target = self.resolve_existing_directory(target_cwd)?;
        let previous_cwd = self.cwd().to_owned();
        let previous_session_file = self.session.session_file();
        let mut session_manager = self.session.session_manager().clone();
        session_manager
            .relocate(&resolved_target, Some(&self.services.agent_dir))
            .map_err(|error| error.message().clone())?;
        let mut details = JsObject::new();
        details.insert("previousCwd".into(), JsValue::String(previous_cwd.clone()));
        details.insert("cwd".into(), JsValue::String(resolved_target.clone()));
        session_manager
            .append_custom_message_entry(
                &"cwd_changed".into(),
                pi_agent_core::harness::messages::CustomMessageContent::Text(
                    JsString::from("Working directory changed from ") + &previous_cwd + " to " + &resolved_target + ".",
                ),
                true,
                Some(JsValue::Object(details)),
            )
            .map_err(|error| error.message().clone())?;
        Ok(PreparedCwdRelocation {
            previous_session_file,
            session_manager,
        })
    }

    async fn activate_relocation(&mut self, prepared: PreparedCwdRelocation) -> Result<(), JsString> {
        let cwd = prepared.session_manager.get_cwd().to_owned();
        let session_file = prepared.session_manager.get_session_file().cloned();
        self.teardown_current_with(SessionShutdownReason::Resume, session_file, false)
            .await;
        let result = self
            .create_runtime(&cwd, prepared.session_manager, "resume", prepared.previous_session_file)
            .await?;
        self.apply(result);
        self.finish_session_replacement(None).await?;
        Ok(())
    }

    /// Prepares a tool-result working-directory relocation.
    pub async fn prepare_tool_result_relocation(&self, target_cwd: &JsString) -> Result<(), JsString> {
        let mut slot = self
            .prepared_tool_result_relocation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if slot.is_some() {
            return Err("A tool-result working-directory relocation is already prepared".into());
        }
        *slot = Some(self.prepare_relocation(target_cwd)?);
        Ok(())
    }

    /// Activates a prepared tool-result relocation and continues the session.
    pub async fn activate_tool_result_relocation(&mut self) -> Result<(), JsString> {
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
            .map_err(|error| error.message().clone())
    }

    /// Relocates the session to a new working directory.
    pub async fn relocate(&mut self, target_cwd: &JsString) -> Result<(), JsString> {
        let prepared = self.prepare_relocation(target_cwd)?;
        self.activate_relocation(prepared).await
    }

    /// Restarts the session, optionally as a process restart.
    pub async fn restart(
        &mut self,
        notice: Option<&pi_js_compat::js_json::JsString>,
        process: bool,
    ) -> Result<(), JsString> {
        let previous_session_file = self.session.session_file();
        let current_session_file = self.session.session_file();
        let is_persisted = self.session.session_manager().is_persisted();

        if let (true, true, Some(session_file)) = (process, is_persisted, current_session_file.clone()) {
            self.session
                .session_manager()
                .persist_for_recovery()
                .map_err(|error| error.message().clone())?;
            if let Some(handler) = &self.before_process_restart {
                handler().await;
            }
            self.teardown_current(SessionShutdownReason::Restart, Some(session_file.clone()))
                .await;
            return (self.process_restarter)(ProcessRestartOptions {
                session_file,
                prompt: notice.cloned(),
            })
            .await;
        }

        let session_manager = if is_persisted && current_session_file.is_some() {
            let file = current_session_file.as_ref().expect("checked session file");
            let (dir, cwd) = {
                let manager = self.session.session_manager();
                (manager.get_session_dir().to_owned(), manager.get_cwd().to_owned())
            };
            SessionManager::open(file, Some(&dir), Some(&cwd)).map_err(|error| error.message().clone())?
        } else {
            self.session.session_manager().clone()
        };
        let mut session_manager = session_manager;
        if let Some(notice) = notice {
            session_manager
                .append_custom_message_entry(
                    &"self_restart".into(),
                    pi_agent_core::harness::messages::CustomMessageContent::Text(notice.clone()),
                    true,
                    None,
                )
                .map_err(|error| error.message().clone())?;
        }
        let cwd = session_manager.get_cwd().to_owned();
        self.teardown_current(SessionShutdownReason::Restart, current_session_file)
            .await;
        let result = self
            .create_runtime(&cwd, session_manager, "restart", previous_session_file)
            .await?;
        self.apply(result);
        self.finish_session_replacement(None).await?;
        Ok(())
    }

    /// Starts a new session.
    pub async fn new_session(&mut self, options: NewSessionOptions) -> Result<SessionTransitionResult, JsString> {
        if self.emit_before_switch(SessionSwitchReason::New, None).await {
            return Ok(SessionTransitionResult { cancelled: true });
        }
        let previous_session_file = self.session.session_file();
        let (is_persisted, session_dir) = {
            let manager = self.session.session_manager();
            (manager.is_persisted(), manager.get_session_dir().to_owned())
        };
        let mut session_manager = if is_persisted {
            SessionManager::create(self.cwd(), Some(&session_dir), None).map_err(|error| error.message().clone())?
        } else {
            SessionManager::in_memory(Some(self.cwd()), None).map_err(|error| error.message().clone())?
        };
        if let Some(parent_session) = &options.parent_session {
            session_manager
                .new_session(Some(crate::core::session_manager::NewSessionOptions {
                    id: None,
                    parent_session: Some(parent_session.clone()),
                    is_subagent: false,
                    subagent_name: None,
                }))
                .map_err(|error| error.message().clone())?;
        }
        let cwd = self.cwd().to_owned();
        self.teardown_current(SessionShutdownReason::New, session_manager.get_session_file().cloned())
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
            let (context, identities) = manager.build_session_context_with_identities();
            *handle.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = manager;
            self.session
                .agent()
                .set_messages_with_identities(context.messages, identities)
                .map_err(|error| JsString::from(error.message()))?;
        }
        self.finish_session_replacement(options.with_session).await?;
        Ok(SessionTransitionResult { cancelled: false })
    }

    /// Swaps the runtime to `session_manager` and runs the replacement callback.
    async fn replace_with_fork(
        &mut self,
        cwd: &JsString,
        session_manager: SessionManager,
        previous_session_file: Option<JsString>,
        with_session: Option<ReplacedSessionCallback>,
    ) -> Result<(), JsString> {
        self.teardown_current(SessionShutdownReason::Fork, session_manager.get_session_file().cloned())
            .await;
        let result = self
            .create_runtime(cwd, session_manager, "fork", previous_session_file)
            .await?;
        self.apply(result);
        self.finish_session_replacement(with_session).await?;
        Ok(())
    }

    /// Forks the current session at an entry.
    pub async fn fork(
        &mut self,
        entry_id: &JsString,
        position: ForkPosition,
        with_session: Option<ReplacedSessionCallback>,
    ) -> Result<ForkResult, JsString> {
        let extension_position = match position {
            ForkPosition::Before => ExtensionForkPosition::Before,
            ForkPosition::At => ExtensionForkPosition::At,
        };
        if self.emit_before_fork(entry_id, extension_position).await {
            return Ok(ForkResult {
                cancelled: true,
                selected_text: None,
            });
        }
        let selected_entry = self
            .session
            .session_manager()
            .get_entry(entry_id)
            .cloned()
            .ok_or_else(|| JsString::from("Invalid entry ID for forking"))?;
        let mut selected_text = None;
        let target_leaf_id = if position == ForkPosition::At {
            selected_entry.id().cloned()
        } else {
            let is_user_message = selected_entry.as_message().is_some_and(|entry| {
                matches!(
                    entry.message.as_ref(),
                    pi_agent_core::agent::types::AgentMessage::User(_)
                )
            });
            if !is_user_message {
                return Err(JsString::from("Invalid entry ID for forking"));
            }
            if let Some(entry) = selected_entry.as_message()
                && let pi_agent_core::agent::types::AgentMessage::User(user) = entry.message.as_ref()
            {
                selected_text = Some(user_content_text(&user.content));
            }
            selected_entry.parent_id().cloned()
        };

        let previous_session_file = self.session.session_file();
        let is_persisted = self.session.session_manager().is_persisted();
        if is_persisted {
            let current_session_file = self
                .session
                .session_file()
                .ok_or_else(|| JsString::from("Persisted session is missing a session file"))?;
            let session_dir = self.session.session_manager().get_session_dir().to_owned();
            let Some(target_leaf_id) = target_leaf_id else {
                let mut session_manager = SessionManager::create(self.cwd(), Some(&session_dir), None)
                    .map_err(|error| error.message().clone())?;
                session_manager
                    .new_session(Some(crate::core::session_manager::NewSessionOptions {
                        id: None,
                        parent_session: Some(current_session_file),
                        is_subagent: false,
                        subagent_name: None,
                    }))
                    .map_err(|error| error.message().clone())?;
                let cwd = self.cwd().to_owned();
                self.replace_with_fork(&cwd, session_manager, previous_session_file, with_session)
                    .await?;
                return Ok(ForkResult {
                    cancelled: false,
                    selected_text,
                });
            };
            let mut session_manager = SessionManager::open(&current_session_file, Some(&session_dir), None)
                .map_err(|error| error.message().clone())?;
            let forked_session_path = session_manager
                .create_branched_session(&target_leaf_id)
                .map_err(|error| error.message().clone())?;
            if forked_session_path.is_none() {
                return Err("Failed to create forked session".into());
            }
            let cwd = session_manager.get_cwd().to_owned();
            self.replace_with_fork(&cwd, session_manager, previous_session_file, with_session)
                .await?;
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
                            parent_session: previous_session_file.clone(),
                            is_subagent: false,
                            subagent_name: None,
                        }))
                        .map_err(|error| error.message().clone())?;
                }
                Some(target_leaf_id) => {
                    manager
                        .create_branched_session(target_leaf_id)
                        .map_err(|error| error.message().clone())?;
                }
            }
        }
        let session_manager = self.session.session_manager().clone();
        let cwd = self.cwd().to_owned();
        self.replace_with_fork(&cwd, session_manager, previous_session_file, with_session)
            .await?;
        Ok(ForkResult {
            cancelled: false,
            selected_text,
        })
    }

    /// Imports a session JSONL file and switches to it.
    pub async fn import_from_jsonl(
        &mut self,
        input_path: &JsString,
        cwd_override: Option<&JsString>,
    ) -> Result<SessionTransitionResult, JsString> {
        if self.session.no_supervisor() {
            return Err("Session import is unavailable in standalone worker mode".into());
        }
        let resolved = resolve_path(
            input_path,
            &process_cwd().map_err(|error| error.message().clone())?,
            &PathInputOptions::default(),
        )
        .map_err(|error| error.message().clone())?;
        if !std::path::Path::new(&encode_node_utf8(&resolved)).exists() {
            return Err(SessionImportFileNotFoundError::new(resolved).message().clone());
        }
        let session_dir = self.session.session_manager().get_session_dir().to_owned();
        if !std::path::Path::new(&encode_node_utf8(&session_dir)).exists() {
            std::fs::create_dir_all(encode_node_utf8(&session_dir))
                .map_err(|error| JsString::from(error.to_string()))?;
        }
        let file_name = crate::utils::paths::node_basename(&resolved);
        let destination_path = crate::utils::paths::node_join(&session_dir, file_name);
        if self
            .emit_before_switch(SessionSwitchReason::Resume, Some(destination_path.clone()))
            .await
        {
            return Ok(SessionTransitionResult { cancelled: true });
        }
        let previous_session_file = self.session.session_file();
        let resolved_destination = resolve_path(
            &destination_path,
            &process_cwd().map_err(|error| error.message().clone())?,
            &PathInputOptions {
                expand_tilde: Some(false),
                ..PathInputOptions::default()
            },
        )
        .map_err(|error| error.message().clone())?;
        self.copy_imported_session(&resolved, &destination_path, &resolved_destination)?;
        let session_manager = SessionManager::open(&destination_path, Some(&session_dir), cwd_override)
            .map_err(|error| error.message().clone())?;
        assert_session_cwd_exists(&session_manager, self.cwd()).map_err(|error| error.message().clone())?;
        let cwd = session_manager.get_cwd().to_owned();
        self.teardown_current(
            SessionShutdownReason::Resume,
            session_manager.get_session_file().cloned(),
        )
        .await;
        let result = self
            .create_runtime(&cwd, session_manager, "resume", previous_session_file)
            .await?;
        self.apply(result);
        self.finish_session_replacement(None).await?;
        Ok(SessionTransitionResult { cancelled: false })
    }

    /// `copyImportedSession`: copy the imported file and drop any metadata
    /// that the destination path inherited from an earlier session.
    fn copy_imported_session(
        &self,
        source: &JsString,
        destination: &JsString,
        resolved_destination: &JsString,
    ) -> Result<(), JsString> {
        if resolved_destination == source {
            return Ok(());
        }
        copy_imported_session(source, destination)?;
        let control_db_path = self.session.session_manager().get_metadata_control_db_path().cloned();
        if let Some(control_db_path) = control_db_path {
            crate::core::session_control_db::remove_session_metadata(&control_db_path, destination)
                .map_err(|error| error.message().clone())?;
        }
        Ok(())
    }

    /// Disposes the runtime and its session.
    pub async fn dispose(&mut self) {
        let runner = self.session.extension_runner();
        emit_session_shutdown_event(
            &runner,
            SessionShutdownEvent {
                reason: SessionShutdownReason::Quit,
                target_session_file: None,
            },
        )
        .await;
        if let Some(handler) = &self.before_session_invalidate {
            handler();
        }
        self.session.dispose();
    }
}

/// Copies an imported session file. `copyFileSync` treats source and destination
/// that name the same file as a no-op, which is reachable once the caller
/// compares lexical `path.resolve` paths instead of canonical paths.
fn copy_imported_session(source: &JsString, destination: &JsString) -> Result<(), JsString> {
    if is_same_file(source, destination) {
        return Ok(());
    }
    std::fs::copy(encode_node_utf8(source), encode_node_utf8(destination))
        .map(|_| ())
        .map_err(|error| JsString::from(error.to_string()))
}

#[cfg(unix)]
fn is_same_file(left: &JsString, right: &JsString) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (
        std::fs::metadata(encode_node_utf8(left)),
        std::fs::metadata(encode_node_utf8(right)),
    ) {
        (Ok(left), Ok(right)) => left.dev() == right.dev() && left.ino() == right.ino(),
        _ => false,
    }
}

#[cfg(not(unix))]
fn is_same_file(left: &JsString, right: &JsString) -> bool {
    match (
        std::fs::canonicalize(encode_node_utf8(left)),
        std::fs::canonicalize(encode_node_utf8(right)),
    ) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

type SharedRuntimeWeak = std::sync::Weak<tokio::sync::Mutex<AgentSessionRuntime>>;

fn upgrade_runtime(weak: &SharedRuntimeWeak) -> Result<Arc<tokio::sync::Mutex<AgentSessionRuntime>>, HandlerError> {
    weak.upgrade()
        .ok_or_else(|| HandlerError::new("Agent session runtime is not available"))
}

fn build_relocate_handler(weak: &SharedRuntimeWeak) -> crate::core::extensions::types::RelocateCallback {
    let weak = weak.clone();
    Arc::new(move |target_cwd: &JsString| {
        let weak = weak.clone();
        let target_cwd = target_cwd.to_owned();
        Box::pin(async move {
            let shared = upgrade_runtime(&weak)?;
            shared
                .lock()
                .await
                .relocate(&target_cwd)
                .await
                .map_err(HandlerError::new)
        }) as BoxFuture<'static, Result<(), HandlerError>>
    })
}

fn build_prepare_tool_result_relocation_handler(
    weak: &SharedRuntimeWeak,
) -> crate::core::extensions::types::RelocateCallback {
    let weak = weak.clone();
    Arc::new(move |target_cwd: &JsString| {
        let weak = weak.clone();
        let target_cwd = target_cwd.to_owned();
        Box::pin(async move {
            let shared = upgrade_runtime(&weak)?;
            shared
                .lock()
                .await
                .prepare_tool_result_relocation(&target_cwd)
                .await
                .map_err(HandlerError::new)
        }) as BoxFuture<'static, Result<(), HandlerError>>
    })
}

fn build_activate_tool_result_relocation_handler(
    weak: &SharedRuntimeWeak,
) -> crate::core::extensions::types::ActivateToolResultRelocationHandler {
    let weak = weak.clone();
    Arc::new(move || {
        let weak = weak.clone();
        Box::pin(async move {
            let shared = upgrade_runtime(&weak)?;
            shared
                .lock()
                .await
                .activate_tool_result_relocation()
                .await
                .map_err(HandlerError::new)
        }) as BoxFuture<'static, Result<(), HandlerError>>
    })
}

/// Creates the initial runtime from a factory and initial session target.
pub async fn create_agent_session_runtime(
    create_runtime: CreateAgentSessionRuntimeFactory,
    cwd: &JsString,
    agent_dir: &JsString,
    session_manager: SessionManager,
    no_supervisor: Option<bool>,
) -> Result<AgentSessionRuntime, JsString> {
    if no_supervisor == Some(true) && session_manager.is_persisted() {
        return Err("Standalone worker requires an in-memory session".into());
    }
    assert_session_cwd_exists(&session_manager, cwd).map_err(|error| error.message().clone())?;
    if no_supervisor != Some(true)
        && let Some(control_db_path) = session_manager.get_metadata_control_db_path()
    {
        crate::core::detached_job_cleanup::run_detached_job_artifact_cleanup(control_db_path, Some(agent_dir), None)
            .map_err(|error| error.message().clone())?;
    }
    let result = create_runtime(CreateAgentSessionRuntimeOptions {
        no_supervisor,
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

/// Wraps a created runtime in its shared handle and binds the runtime-owned
/// extension actions, matching the TypeScript constructor call to
/// `bindSessionRuntimeActions`.
pub async fn share_agent_session_runtime(runtime: AgentSessionRuntime) -> Arc<tokio::sync::Mutex<AgentSessionRuntime>> {
    let shared = Arc::new(tokio::sync::Mutex::new(runtime));
    AgentSessionRuntime::bind_session_runtime_actions(&shared).await;
    shared
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
                    model_registry: None,
                    auth_storage: None,
                    extension_flag_values: None,
                    resource_loader_options: None,
                    resource_loader: Some(options.resource_loader),
                    resource_loader_reload_options: None,
                })
                .await?;
            let result: CreateAgentSessionResult = create_agent_session_from_services(
                crate::core::agent_session_services::CreateAgentSessionFromServicesOptions {
                    no_supervisor: request.no_supervisor,
                    services: AgentSessionServices {
                        cwd: services.cwd.clone(),
                        agent_dir: services.agent_dir.clone(),
                        model_registry: services.model_registry.clone(),
                        auth_storage: services.auth_storage.clone(),
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
                    multi_agent_store: None,
                    on_shared_channel_message_delivered: None,
                    session_start_event: None,
                },
            )
            .await?;
            let services = AgentSessionServices {
                cwd: services.cwd,
                agent_dir: services.agent_dir,
                model_registry: services.model_registry,
                auth_storage: services.auth_storage,
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

fn user_content_text(content: &pi_ai::types::UserContent) -> pi_js_compat::js_json::JsString {
    match content {
        pi_ai::types::UserContent::Text(text) => text.clone(),
        pi_ai::types::UserContent::Blocks(blocks) => pi_js_compat::js_json::JsString::join(
            blocks.iter().filter_map(|block| match block {
                pi_ai::types::UserContentBlock::Text(text) => Some(&text.text),
                pi_ai::types::UserContentBlock::Image(_) => None,
            }),
            "",
        ),
    }
}
