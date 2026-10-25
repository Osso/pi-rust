//! Port of `packages/coding-agent/src/core/agent-session-runtime.ts`.

use std::sync::{Arc, Mutex};

use pi_ai::request::{BoxFuture, CallbackError};
use pi_ai::types::Model;
use pi_js_compat::js_json::{JsObject, JsString, JsValue, encode_node_utf8};

use crate::core::agent_session::{AgentSession, WorkingDirectoryRecoveryHandlers};
use crate::core::agent_session_services::{
    AgentSessionRuntimeDiagnostic, AgentSessionServices, create_agent_session_from_services,
    create_agent_session_services,
};
use crate::core::extensions::emit_session_shutdown_event;
use crate::core::extensions::runner::EmitResult;
use crate::core::extensions::types::HandlerError;
use crate::core::extensions::types::{
    ExtensionEvent, ForkPosition as ExtensionForkPosition, ProjectTrustContext, SessionBeforeForkEvent,
    SessionBeforeSwitchEvent, SessionShutdownEvent, SessionShutdownReason, SessionSwitchReason,
};
use crate::core::sdk::CreateAgentSessionResult;
use crate::core::session_control_db::assert_main_session_runtime_available;
use crate::core::session_cwd::{assert_session_cwd_exists, find_nearest_existing_cwd};
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
    pub project_trust_context: Option<crate::core::extensions::types::ProjectTrustContext>,
    pub no_supervisor: Option<bool>,
    pub cwd: JsString,
    pub agent_dir: JsString,
    pub session_manager: SessionManager,
    pub session_start_reason: String,
    pub previous_session_file: Option<JsString>,
}

/// Runtime factory reused for `/new`, `/resume`, `/fork`, and imports.
pub type CreateAgentSessionRuntimeFactory = Arc<
    dyn Fn(
            CreateAgentSessionRuntimeOptions,
        ) -> BoxFuture<'static, Result<CreateAgentSessionRuntimeResult, CallbackError>>
        + Send
        + Sync,
>;

/// Host scheduler for lifecycle work; the runtime retains sole ownership of the FIFO gate.
pub type HostTransitionExecutor = Arc<dyn Fn(BoxFuture<'static, ()>) -> Result<(), CallbackError> + Send + Sync>;

/// Callback receiving the replacement session.
pub type RebindSessionHandler =
    Arc<dyn Fn(AgentSession) -> BoxFuture<'static, Result<(), CallbackError>> + Send + Sync>;
/// Callback invoked before the outgoing session is invalidated.
pub type BeforeSessionInvalidateHandler = Arc<dyn Fn() -> Result<(), CallbackError> + Send + Sync>;
/// Callback invoked before a process restart.
pub type BeforeProcessRestartHandler = Arc<dyn Fn() -> BoxFuture<'static, Result<(), CallbackError>> + Send + Sync>;
/// Replacement for `restartCurrentProcess`.
pub type ProcessRestarter =
    Arc<dyn Fn(ProcessRestartOptions) -> BoxFuture<'static, Result<(), CallbackError>> + Send + Sync>;

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
pub type SessionSetupCallback =
    Arc<dyn Fn(&mut SessionManager) -> BoxFuture<'static, Result<(), CallbackError>> + Send + Sync>;
/// Callback receiving the replacement session context.
pub type ReplacedSessionCallback = Arc<
    dyn Fn(crate::core::extensions::types::ReplacedSessionContext) -> BoxFuture<'static, Result<(), CallbackError>>
        + Send
        + Sync,
>;

/// Options for `newSession`.
#[derive(Default)]
pub struct NewSessionOptions {
    pub parent_session: Option<JsString>,
    pub setup: Option<SessionSetupCallback>,
    pub with_session: Option<ReplacedSessionCallback>,
}

pub type ProjectTrustContextFactory =
    Arc<dyn Fn(&JsString) -> Result<crate::core::extensions::types::ProjectTrustContext, CallbackError> + Send + Sync>;

#[derive(Default)]
pub struct SwitchSessionOptions {
    pub cwd_override: Option<JsString>,
    pub with_session: Option<ReplacedSessionCallback>,
    pub project_trust_context_factory: Option<ProjectTrustContextFactory>,
}

/// Cloneable owner of the current session and its serialized lifecycle.
#[derive(Clone)]
pub struct AgentSessionRuntime {
    inner: Arc<RuntimeInner>,
}

struct RuntimeInner {
    state: Mutex<RuntimeState>,
    create_runtime: CreateAgentSessionRuntimeFactory,
    lifecycle: Arc<tokio::sync::Mutex<()>>,
    prepared_tool_result_relocation: Mutex<Option<PreparedToolResultRelocation>>,
}

struct RuntimeState {
    session: AgentSession,
    services: AgentSessionServices,
    diagnostics: Vec<AgentSessionRuntimeDiagnostic>,
    model_fallback_message: Option<JsString>,
    rebind_session: Option<RebindSessionHandler>,
    before_session_invalidate: Option<BeforeSessionInvalidateHandler>,
    before_process_restart: Option<BeforeProcessRestartHandler>,
    process_restarter: ProcessRestarter,
    host_transition_executor: Option<HostTransitionExecutor>,
}

struct PreparedCwdRelocation {
    project_trust_context: Option<ProjectTrustContext>,
    previous_session_file: Option<JsString>,
    session_manager: SessionManager,
}

struct PreparedToolResultRelocation {
    relocation: PreparedCwdRelocation,
    permit: tokio::sync::OwnedMutexGuard<()>,
}

fn create_import_session_directory(path: &JsString) -> Result<(), CallbackError> {
    let encoded = encode_node_utf8(path);
    // Rust's recursive mkdir accepts an empty path as a no-op; Node's mkdirSync
    // rejects it. Keep the source's in-memory import failure, not a recovery path.
    let result = if path.is_empty() {
        std::fs::create_dir(&encoded)
    } else {
        std::fs::create_dir_all(&encoded)
    };
    result.map_err(|error| {
        let record = pi_js_compat::node_fs::node_fs_error_record(&error, "mkdir", path);
        CallbackError::from(pi_js_compat::node_error::NativeNodeError::from(record)).with_original(Arc::new(error))
    })
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn default_process_restarter() -> ProcessRestarter {
    Arc::new(|options| {
        Box::pin(async move {
            let request = crate::core::self_restart::SelfRestartRequest {
                session_file: options.session_file,
                prompt: options.prompt,
                old_pid: None,
            };
            crate::core::self_restart::restart_current_process(
                request,
                &crate::core::self_restart::SelfRestartDependencies::default(),
            )
            .await
        })
    })
}

impl AgentSessionRuntime {
    pub fn new(
        session: AgentSession,
        services: AgentSessionServices,
        create_runtime: CreateAgentSessionRuntimeFactory,
        diagnostics: Vec<AgentSessionRuntimeDiagnostic>,
        model_fallback_message: Option<JsString>,
    ) -> Self {
        let runtime = Self {
            inner: Arc::new(RuntimeInner {
                state: Mutex::new(RuntimeState {
                    session,
                    services,
                    diagnostics,
                    model_fallback_message,
                    rebind_session: None,
                    before_session_invalidate: None,
                    before_process_restart: None,
                    process_restarter: default_process_restarter(),
                    host_transition_executor: None,
                }),
                create_runtime,
                lifecycle: Arc::new(tokio::sync::Mutex::new(())),
                prepared_tool_result_relocation: Mutex::new(None),
            }),
        };
        runtime.bind_session_runtime_actions();
        runtime
    }

    pub fn services(&self) -> AgentSessionServices {
        lock(&self.inner.state).services.clone()
    }

    pub fn session(&self) -> AgentSession {
        lock(&self.inner.state).session.clone()
    }

    pub fn cwd(&self) -> JsString {
        lock(&self.inner.state).services.cwd.clone()
    }

    pub fn diagnostics(&self) -> Vec<AgentSessionRuntimeDiagnostic> {
        lock(&self.inner.state).diagnostics.clone()
    }

    pub fn model_fallback_message(&self) -> Option<JsString> {
        lock(&self.inner.state).model_fallback_message.clone()
    }

    pub fn set_rebind_session(&self, handler: Option<RebindSessionHandler>) {
        lock(&self.inner.state).rebind_session = handler;
    }

    pub fn set_before_session_invalidate(&self, handler: Option<BeforeSessionInvalidateHandler>) {
        lock(&self.inner.state).before_session_invalidate = handler;
    }

    pub fn set_before_process_restart(&self, handler: Option<BeforeProcessRestartHandler>) {
        lock(&self.inner.state).before_process_restart = handler;
    }

    pub fn set_process_restarter(&self, restarter: ProcessRestarter) {
        lock(&self.inner.state).process_restarter = restarter;
    }

    pub fn set_host_transition_executor(&self, executor: Option<HostTransitionExecutor>) {
        lock(&self.inner.state).host_transition_executor = executor;
    }

    async fn dispatch_transition<T: Send + 'static>(
        &self,
        transition: BoxFuture<'static, Result<T, CallbackError>>,
    ) -> Result<T, CallbackError> {
        let executor = lock(&self.inner.state).host_transition_executor.clone();
        let Some(executor) = executor else {
            return transition.await;
        };
        let (respond, receiver) = tokio::sync::oneshot::channel();
        executor(Box::pin(async move {
            let _ = respond.send(transition.await);
        }))?;
        receiver
            .await
            .map_err(|_| CallbackError::new("Agent session runtime is not available"))?
    }

    async fn run_lifecycle_transition<T>(
        &self,
        transition: impl Future<Output = Result<T, CallbackError>>,
    ) -> Result<T, CallbackError> {
        let _permit = self.inner.lifecycle.clone().lock_owned().await;
        transition.await
    }

    pub async fn switch_session(
        &self,
        session_path: &JsString,
        options: SwitchSessionOptions,
    ) -> Result<SessionTransitionResult, CallbackError> {
        if self.session().no_supervisor() {
            return Err("Session resume is unavailable in standalone worker mode".into());
        }
        let runtime = self.clone();
        let session_path = session_path.clone();
        self.dispatch_transition(Box::pin(async move {
            runtime
                .run_lifecycle_transition(runtime.switch_session_unlocked(&session_path, options))
                .await
        }))
        .await
    }

    pub async fn new_session(&self, options: NewSessionOptions) -> Result<SessionTransitionResult, CallbackError> {
        let runtime = self.clone();
        self.dispatch_transition(Box::pin(async move {
            runtime
                .run_lifecycle_transition(runtime.new_session_unlocked(options))
                .await
        }))
        .await
    }

    pub async fn fork(
        &self,
        entry_id: &JsString,
        position: ForkPosition,
        with_session: Option<ReplacedSessionCallback>,
    ) -> Result<ForkResult, CallbackError> {
        let runtime = self.clone();
        let entry_id = entry_id.clone();
        self.dispatch_transition(Box::pin(async move {
            runtime
                .run_lifecycle_transition(runtime.fork_unlocked(&entry_id, position, with_session))
                .await
        }))
        .await
    }

    pub async fn import_from_jsonl(
        &self,
        input_path: &JsString,
        cwd_override: Option<&JsString>,
    ) -> Result<SessionTransitionResult, CallbackError> {
        if self.session().no_supervisor() {
            return Err("Session import is unavailable in standalone worker mode".into());
        }
        let runtime = self.clone();
        let input_path = input_path.clone();
        let cwd_override = cwd_override.cloned();
        self.dispatch_transition(Box::pin(async move {
            runtime
                .run_lifecycle_transition(runtime.import_from_jsonl_unlocked(&input_path, cwd_override.as_ref()))
                .await
        }))
        .await
    }

    pub async fn relocate(
        &self,
        target_cwd: &JsString,
        project_trust_context: Option<crate::core::extensions::types::ProjectTrustContext>,
    ) -> Result<(), CallbackError> {
        let runtime = self.clone();
        let target_cwd = target_cwd.clone();
        self.dispatch_transition(Box::pin(async move {
            runtime
                .run_lifecycle_transition(runtime.relocate_unlocked(&target_cwd, project_trust_context))
                .await
        }))
        .await
    }

    pub async fn prepare_tool_result_relocation(&self, target_cwd: &JsString) -> Result<(), CallbackError> {
        let runtime = self.clone();
        let target_cwd = target_cwd.clone();
        self.dispatch_transition(Box::pin(async move {
            runtime.prepare_tool_result_relocation_unlocked(&target_cwd).await
        }))
        .await
    }

    pub async fn activate_tool_result_relocation(&self) -> Result<(), CallbackError> {
        let runtime = self.clone();
        self.dispatch_transition(Box::pin(async move {
            runtime.activate_tool_result_relocation_unlocked().await
        }))
        .await
    }

    pub async fn reload(
        &self,
        before_session_start: Option<Arc<dyn Fn() -> BoxFuture<'static, ()> + Send + Sync>>,
    ) -> Result<(), CallbackError> {
        let runtime = self.clone();
        self.dispatch_transition(Box::pin(async move {
            runtime
                .run_lifecycle_transition(runtime.reload_unlocked(before_session_start))
                .await
        }))
        .await
    }

    pub async fn restart(&self, notice: Option<&JsString>, process: bool) -> Result<(), CallbackError> {
        let runtime = self.clone();
        let notice = notice.cloned();
        self.dispatch_transition(Box::pin(async move {
            runtime
                .run_lifecycle_transition(runtime.restart_unlocked(notice.as_ref(), process))
                .await
        }))
        .await
    }

    /// Install runtime relocation actions without a second operation lock.
    pub fn bind_session_runtime_actions(&self) {
        let weak = Arc::downgrade(&self.inner);
        self.session()
            .set_working_directory_recovery_handlers(WorkingDirectoryRecoveryHandlers {
                before_run: build_before_working_directory_recovery_handler(&weak),
                after_turn: build_after_working_directory_recovery_handler(&weak),
            });
        self.session().bind_session_runtime_relocation(
            build_relocate_handler(&weak),
            build_prepare_tool_result_relocation_handler(&weak),
            build_activate_tool_result_relocation_handler(&weak),
        );
    }

    async fn recover_working_directory_before_run(&self) -> Result<Option<AgentSession>, CallbackError> {
        let target = find_nearest_existing_cwd(&self.cwd())?;
        self.repair_process_working_directory(&target).await?;
        if target == self.cwd() {
            return Ok(None);
        }
        self.relocate(&target, None).await?;
        Ok(Some(self.session()))
    }

    async fn prepare_working_directory_recovery(&self) -> Result<(), CallbackError> {
        let runner = self.session().extension_runner();
        if runner.has_prepared_tool_result_relocation() {
            return Ok(());
        }
        let target = find_nearest_existing_cwd(&self.cwd())?;
        self.repair_process_working_directory(&target).await?;
        if target != self.cwd() {
            runner
                .prepare_working_directory_relocation(&target)
                .await
                .map_err(CallbackError::from)?;
        }
        Ok(())
    }

    async fn repair_process_working_directory(&self, target: &JsString) -> Result<(), CallbackError> {
        if let Some(notice) = repair_deleted_process_cwd(target)? {
            self.session()
                .send_custom_message(
                    &"process_cwd_recovered".into(),
                    pi_agent_core::harness::messages::CustomMessageContent::Text(notice),
                    true,
                    None,
                    false,
                    None,
                )
                .await
                .map_err(CallbackError::from)?;
        }
        Ok(())
    }

    fn apply(&self, result: CreateAgentSessionRuntimeResult) {
        {
            let mut state = lock(&self.inner.state);
            state.session = result.session;
            state.services = result.services;
            state.diagnostics = result.diagnostics;
            state.model_fallback_message = result.model_fallback_message;
        }
        self.bind_session_runtime_actions();
    }

    async fn finish_session_replacement(
        &self,
        with_session: Option<ReplacedSessionCallback>,
    ) -> Result<(), CallbackError> {
        let rebind = lock(&self.inner.state).rebind_session.clone();
        if let Some(rebind) = rebind {
            rebind(self.session().clone()).await?;
        }
        if let Some(with_session) = with_session {
            with_session(self.session().create_replaced_session_context()).await?;
        }
        Ok(())
    }

    async fn teardown_current(
        &self,
        reason: SessionShutdownReason,
        target_session_file: Option<JsString>,
    ) -> Result<(), CallbackError> {
        let session = self.session();
        let runner = session.extension_runner();
        emit_session_shutdown_event(
            &runner,
            SessionShutdownEvent {
                reason,
                target_session_file: target_session_file.clone(),
            },
        )
        .await;
        let handler = lock(&self.inner.state).before_session_invalidate.clone();
        if let Some(handler) = handler {
            handler()?;
        }
        crate::core::empty_session_cleanup::remove_abandoned_empty_session(
            &session.session_manager(),
            target_session_file.as_ref(),
        )?;
        session.dispose();
        Ok(())
    }

    async fn emit_before_switch(&self, reason: SessionSwitchReason, target_session_file: Option<JsString>) -> bool {
        let runner = self.session().extension_runner();
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
        let runner = self.session().extension_runner();
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
        project_trust_context: Option<ProjectTrustContext>,
    ) -> Result<CreateAgentSessionRuntimeResult, CallbackError> {
        (self.inner.create_runtime)(CreateAgentSessionRuntimeOptions {
            project_trust_context,
            no_supervisor: Some(self.session().no_supervisor()),
            cwd: cwd.to_owned(),
            agent_dir: self.services().agent_dir.clone(),
            session_manager,
            session_start_reason: reason.to_owned(),
            previous_session_file,
        })
        .await
    }

    /// Switches to an existing session file.
    async fn switch_session_unlocked(
        &self,
        session_path: &JsString,
        options: SwitchSessionOptions,
    ) -> Result<SessionTransitionResult, CallbackError> {
        if self
            .emit_before_switch(SessionSwitchReason::Resume, Some(session_path.to_owned()))
            .await
        {
            return Ok(SessionTransitionResult { cancelled: true });
        }
        let previous_session_file = self.session().session_file();
        let control_db_path = self.current_session_control_db_path()?;
        let restored_path =
            crate::core::session_archive_storage::restore_archived_session(&control_db_path, session_path)
                .map_err(CallbackError::from)?;
        let mut session_manager =
            SessionManager::open(&restored_path, None, options.cwd_override.as_ref()).map_err(CallbackError::from)?;
        session_manager
            .set_metadata_control_db_path(Some(&control_db_path), Default::default())
            .map_err(CallbackError::from)?;
        assert_session_cwd_exists(&session_manager, &self.cwd())?;
        assert_main_session_runtime_available(&control_db_path, session_manager.get_session_id())
            .map_err(CallbackError::from)?;
        let cwd = session_manager.get_cwd().to_owned();
        self.teardown_current(
            SessionShutdownReason::Resume,
            session_manager.get_session_file().cloned(),
        )
        .await?;
        let context = options
            .project_trust_context_factory
            .as_ref()
            .map(|factory| factory(&cwd))
            .transpose()?;
        let result = self
            .create_runtime(&cwd, session_manager, "resume", previous_session_file, context)
            .await?;
        self.apply(result);
        self.finish_session_replacement(options.with_session).await?;
        Ok(SessionTransitionResult { cancelled: false })
    }

    fn current_session_control_db_path(&self) -> Result<JsString, CallbackError> {
        self.session()
            .session_manager()
            .get_metadata_control_db_path()
            .filter(|path| !path.is_empty())
            .cloned()
            .ok_or_else(|| "Current session has no control database path".into())
    }

    fn resolve_existing_directory(&self, input_path: &JsString) -> Result<JsString, CallbackError> {
        let resolved =
            resolve_path(input_path, &self.cwd(), &PathInputOptions::default()).map_err(CallbackError::from)?;
        let encoded = encode_node_utf8(&resolved);
        let path = std::path::Path::new(&encoded);
        if !path.exists() {
            return Err((JsString::from("Directory does not exist: ") + &resolved).into());
        }
        if !path.is_dir() {
            return Err((JsString::from("Not a directory: ") + &resolved).into());
        }
        Ok(resolved)
    }

    fn prepare_relocation(
        &self,
        target_cwd: &JsString,
        project_trust_context: Option<ProjectTrustContext>,
    ) -> Result<PreparedCwdRelocation, CallbackError> {
        let resolved_target = self.resolve_existing_directory(target_cwd)?;
        let previous_cwd = self.cwd().to_owned();
        let previous_session_file = self.session().session_file();
        let session = self.session();
        let mut session_manager = session.session_manager();
        session_manager
            .relocate(&resolved_target, Some(&self.services().agent_dir))
            .map_err(CallbackError::from)?;
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
            .map_err(CallbackError::from)?;
        Ok(PreparedCwdRelocation {
            previous_session_file,
            project_trust_context,
            session_manager: session_manager.clone(),
        })
    }

    async fn activate_relocation(&self, prepared: PreparedCwdRelocation) -> Result<(), CallbackError> {
        let cwd = prepared.session_manager.get_cwd().to_owned();
        self.repair_process_working_directory(&cwd).await?;
        let session_file = prepared.session_manager.get_session_file().cloned();
        self.teardown_current(SessionShutdownReason::Resume, session_file)
            .await?;
        let result = self
            .create_runtime(
                &cwd,
                prepared.session_manager,
                "resume",
                prepared.previous_session_file,
                prepared.project_trust_context,
            )
            .await?;
        self.apply(result);
        self.finish_session_replacement(None).await?;
        Ok(())
    }

    /// Holds the lifecycle permit across the tool-result prepare/activate boundary.
    async fn prepare_tool_result_relocation_unlocked(&self, target_cwd: &JsString) -> Result<(), CallbackError> {
        if lock(&self.inner.prepared_tool_result_relocation).is_some() {
            return Err("A tool-result working-directory relocation is already prepared".into());
        }
        let permit = self.inner.lifecycle.clone().lock_owned().await;
        if lock(&self.inner.prepared_tool_result_relocation).is_some() {
            return Err("A tool-result working-directory relocation is already prepared".into());
        }
        let relocation = self.prepare_relocation(target_cwd, None)?;
        *lock(&self.inner.prepared_tool_result_relocation) = Some(PreparedToolResultRelocation { relocation, permit });
        Ok(())
    }

    async fn activate_tool_result_relocation_unlocked(&self) -> Result<(), CallbackError> {
        let prepared = lock(&self.inner.prepared_tool_result_relocation).take();
        let Some(prepared) = prepared else {
            return Ok(());
        };
        let result = self.activate_relocation(prepared.relocation).await;
        drop(prepared.permit);
        result?;
        self.session().continue_run().await.map_err(CallbackError::from)
    }

    /// Relocates the session to a new working directory.
    async fn relocate_unlocked(
        &self,
        target_cwd: &JsString,
        project_trust_context: Option<ProjectTrustContext>,
    ) -> Result<(), CallbackError> {
        let prepared = self.prepare_relocation(target_cwd, project_trust_context)?;
        self.activate_relocation(prepared).await
    }

    async fn reload_unlocked(
        &self,
        before_session_start: Option<Arc<dyn Fn() -> BoxFuture<'static, ()> + Send + Sync>>,
    ) -> Result<(), CallbackError> {
        self.session()
            .reload(before_session_start)
            .await
            .map_err(CallbackError::from)
    }

    fn persist_session_for_process_restart(&self) -> Result<(), CallbackError> {
        let session = self.session();
        let model = session.model();
        let thinking = session.thinking_level();
        let mut manager = session.session_manager();
        manager
            .set_session_model(&model.provider.as_str().into(), &model.id.as_str().into())
            .map_err(CallbackError::from)?;
        manager
            .set_session_thinking_level(&thinking.as_str().into())
            .map_err(CallbackError::from)?;
        manager.persist_for_recovery().map_err(CallbackError::from)
    }

    /// Restarts the session, optionally as a process restart.
    async fn restart_unlocked(
        &self,
        notice: Option<&pi_js_compat::js_json::JsString>,
        process: bool,
    ) -> Result<(), CallbackError> {
        let previous_session_file = self.session().session_file();
        let current_session_file = self.session().session_file();
        let is_persisted = self.session().session_manager().is_persisted();
        // Frozen agent-session-runtime.ts:439 propagates before either restart branch.
        self.session().detach_all_running_tools()?;

        if let (true, true, Some(session_file)) = (process, is_persisted, current_session_file.clone()) {
            self.persist_session_for_process_restart()?;
            let handler = lock(&self.inner.state).before_process_restart.clone();
            if let Some(handler) = handler {
                handler().await?;
            }
            self.teardown_current(SessionShutdownReason::Restart, Some(session_file.clone()))
                .await?;
            let restarter = lock(&self.inner.state).process_restarter.clone();
            return restarter(ProcessRestartOptions {
                session_file,
                prompt: notice.cloned(),
            })
            .await;
        }

        let session_manager = if is_persisted && current_session_file.is_some() {
            let file = current_session_file.as_ref().expect("checked session file");
            let (dir, cwd) = {
                let session = self.session();
                let manager = session.session_manager();
                (manager.get_session_dir().to_owned(), manager.get_cwd().to_owned())
            };
            SessionManager::open(file, Some(&dir), Some(&cwd)).map_err(CallbackError::from)?
        } else {
            self.session().session_manager().clone()
        };
        let mut session_manager = session_manager;
        crate::core::self_restart::resolve_restart_interrupted_tool_calls(&mut session_manager)?;
        if let Some(notice) = notice.filter(|notice| !notice.is_empty()) {
            session_manager
                .append_custom_message_entry(
                    &"self_restart".into(),
                    pi_agent_core::harness::messages::CustomMessageContent::Text(notice.clone()),
                    true,
                    None,
                )
                .map_err(CallbackError::from)?;
        }
        let cwd = session_manager.get_cwd().to_owned();
        self.teardown_current(SessionShutdownReason::Restart, current_session_file)
            .await?;
        let result = self
            .create_runtime(&cwd, session_manager, "restart", previous_session_file, None)
            .await?;
        self.apply(result);
        self.finish_session_replacement(None).await?;
        Ok(())
    }

    /// Starts a new session.
    async fn new_session_unlocked(&self, options: NewSessionOptions) -> Result<SessionTransitionResult, CallbackError> {
        if self.emit_before_switch(SessionSwitchReason::New, None).await {
            return Ok(SessionTransitionResult { cancelled: true });
        }
        let previous_session_file = self.session().session_file();
        let (is_persisted, session_dir, control_db) = {
            let session = self.session();
            let manager = session.session_manager();
            (
                manager.is_persisted(),
                manager.get_session_dir().to_owned(),
                manager.get_metadata_control_db_path().cloned(),
            )
        };
        let mut session_manager = if is_persisted {
            SessionManager::create(&self.cwd(), Some(&session_dir), None).map_err(CallbackError::from)?
        } else {
            SessionManager::in_memory(Some(&self.cwd()), None).map_err(CallbackError::from)?
        };
        session_manager
            .set_metadata_control_db_path(control_db.as_ref(), Default::default())
            .map_err(CallbackError::from)?;
        if let Some(parent_session) = options.parent_session.as_ref().filter(|parent| !parent.is_empty()) {
            session_manager
                .new_session(Some(crate::core::session_manager::NewSessionOptions {
                    id: None,
                    parent_session: Some(parent_session.clone()),
                    is_subagent: false,
                    subagent_name: None,
                }))
                .map_err(CallbackError::from)?;
        }
        let cwd = self.cwd().to_owned();
        self.teardown_current(SessionShutdownReason::New, session_manager.get_session_file().cloned())
            .await?;
        let result = self
            .create_runtime(&cwd, session_manager, "new", previous_session_file, None)
            .await?;
        self.apply(result);
        if let Some(setup) = options.setup {
            let handle = self.session().session_manager_handle();
            let mut manager = {
                let guard = handle.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                guard.clone()
            };
            let future = setup(&mut manager);
            *handle.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = manager;
            future.await?;
            let (context, identities) = handle
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .build_session_context_with_identities();
            self.session()
                .agent()
                .set_messages_with_identities(context.messages, identities)
                .map_err(|error| JsString::from(error.message()))?;
        }
        self.finish_session_replacement(options.with_session).await?;
        Ok(SessionTransitionResult { cancelled: false })
    }

    /// Swaps the runtime to `session_manager` and runs the replacement callback.
    async fn replace_with_fork(
        &self,
        cwd: &JsString,
        session_manager: SessionManager,
        previous_session_file: Option<JsString>,
        with_session: Option<ReplacedSessionCallback>,
    ) -> Result<(), CallbackError> {
        self.teardown_current(SessionShutdownReason::Fork, session_manager.get_session_file().cloned())
            .await?;
        let result = self
            .create_runtime(cwd, session_manager, "fork", previous_session_file, None)
            .await?;
        self.apply(result);
        self.finish_session_replacement(with_session).await?;
        Ok(())
    }

    /// Forks the current session at an entry.
    async fn fork_unlocked(
        &self,
        entry_id: &JsString,
        position: ForkPosition,
        with_session: Option<ReplacedSessionCallback>,
    ) -> Result<ForkResult, CallbackError> {
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
            .session()
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
                return Err("Invalid entry ID for forking".into());
            }
            if let Some(entry) = selected_entry.as_message()
                && let pi_agent_core::agent::types::AgentMessage::User(user) = entry.message.as_ref()
            {
                selected_text = Some(user_content_text(&user.content));
            }
            selected_entry.parent_id().cloned()
        };

        let previous_session_file = self.session().session_file();
        let is_persisted = self.session().session_manager().is_persisted();
        if is_persisted {
            let current_session_file = self
                .session()
                .session_file()
                .ok_or_else(|| JsString::from("Persisted session is missing a session file"))?;
            let session_dir = self.session().session_manager().get_session_dir().to_owned();
            let Some(target_leaf_id) = target_leaf_id else {
                let mut session_manager =
                    SessionManager::create(&self.cwd(), Some(&session_dir), None).map_err(CallbackError::from)?;
                session_manager
                    .new_session(Some(crate::core::session_manager::NewSessionOptions {
                        id: None,
                        parent_session: Some(current_session_file),
                        is_subagent: false,
                        subagent_name: None,
                    }))
                    .map_err(CallbackError::from)?;
                let cwd = self.cwd().to_owned();
                self.replace_with_fork(&cwd, session_manager, previous_session_file, with_session)
                    .await?;
                return Ok(ForkResult {
                    cancelled: false,
                    selected_text,
                });
            };
            let mut session_manager =
                SessionManager::open(&current_session_file, Some(&session_dir), None).map_err(CallbackError::from)?;
            let forked_session_path = session_manager
                .create_branched_session(&target_leaf_id)
                .map_err(CallbackError::from)?;
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
            let session = self.session();
            let mut manager = session.session_manager();
            match &target_leaf_id {
                None => {
                    manager
                        .new_session(Some(crate::core::session_manager::NewSessionOptions {
                            id: None,
                            parent_session: previous_session_file.clone(),
                            is_subagent: false,
                            subagent_name: None,
                        }))
                        .map_err(CallbackError::from)?;
                }
                Some(target_leaf_id) => {
                    manager
                        .create_branched_session(target_leaf_id)
                        .map_err(CallbackError::from)?;
                }
            }
        }
        let session_manager = self.session().session_manager().clone();
        let cwd = self.cwd().to_owned();
        self.replace_with_fork(&cwd, session_manager, previous_session_file, with_session)
            .await?;
        Ok(ForkResult {
            cancelled: false,
            selected_text,
        })
    }

    /// Imports a session JSONL file and switches to it.
    async fn import_from_jsonl_unlocked(
        &self,
        input_path: &JsString,
        cwd_override: Option<&JsString>,
    ) -> Result<SessionTransitionResult, CallbackError> {
        let resolved = resolve_path(
            input_path,
            &process_cwd().map_err(CallbackError::from)?,
            &PathInputOptions::default(),
        )
        .map_err(CallbackError::from)?;
        if !std::path::Path::new(&encode_node_utf8(&resolved)).exists() {
            return Err(SessionImportFileNotFoundError::new(resolved).into_callback());
        }
        let session_dir = self.session().session_manager().get_session_dir().to_owned();
        if !std::path::Path::new(&encode_node_utf8(&session_dir)).exists() {
            create_import_session_directory(&session_dir)?;
        }
        let file_name = crate::utils::paths::node_basename(&resolved);
        let destination_path = crate::utils::paths::node_join(&session_dir, file_name);
        if self
            .emit_before_switch(SessionSwitchReason::Resume, Some(destination_path.clone()))
            .await
        {
            return Ok(SessionTransitionResult { cancelled: true });
        }
        let previous_session_file = self.session().session_file();
        let resolved_destination = resolve_path(
            &destination_path,
            &process_cwd().map_err(CallbackError::from)?,
            &PathInputOptions {
                expand_tilde: Some(false),
                ..PathInputOptions::default()
            },
        )
        .map_err(CallbackError::from)?;
        self.copy_imported_session(&resolved, &destination_path, &resolved_destination)?;
        let session_manager =
            SessionManager::open(&destination_path, Some(&session_dir), cwd_override).map_err(CallbackError::from)?;
        assert_session_cwd_exists(&session_manager, &self.cwd())?;
        let cwd = session_manager.get_cwd().to_owned();
        self.teardown_current(
            SessionShutdownReason::Resume,
            session_manager.get_session_file().cloned(),
        )
        .await?;
        let result = self
            .create_runtime(&cwd, session_manager, "resume", previous_session_file, None)
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
    ) -> Result<(), CallbackError> {
        if resolved_destination == source {
            return Ok(());
        }
        copy_imported_session(source, destination)?;
        let control_db_path = self.session().session_manager().get_metadata_control_db_path().cloned();
        if let Some(control_db_path) = control_db_path {
            crate::core::session_control_db::remove_session_metadata(&control_db_path, destination)
                .map_err(CallbackError::from)?;
        }
        Ok(())
    }

    /// Disposes the runtime and its session on the host owner, under the same lifecycle permit.
    pub async fn dispose(&self) -> Result<(), CallbackError> {
        let runtime = self.clone();
        self.dispatch_transition(Box::pin(async move {
            runtime
                .run_lifecycle_transition(runtime.teardown_current(SessionShutdownReason::Quit, None))
                .await
        }))
        .await
    }
}

/// Copies an imported session file. `copyFileSync` treats source and destination
/// that name the same file as a no-op, which is reachable once the caller
/// compares lexical `path.resolve` paths instead of canonical paths.
fn copy_imported_session(source: &JsString, destination: &JsString) -> Result<(), CallbackError> {
    if is_same_file(source, destination) {
        return Ok(());
    }
    std::fs::copy(encode_node_utf8(source), encode_node_utf8(destination))
        .map(|_| ())
        .map_err(|error| CallbackError::new(error.to_string()))
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

fn repair_deleted_process_cwd(target: &JsString) -> Result<Option<JsString>, CallbackError> {
    let previous = match pi_js_compat::process_cwd::cwd() {
        Ok(previous) => {
            if crate::core::session_cwd::stat_cwd_directory(&previous)? {
                return Ok(None);
            }
            Some(previous)
        }
        Err(error) => {
            let value = error.value();
            if !matches!(value.get("code").and_then(JsValue::as_string), Some(code) if code == "ENOENT" || code == "ENOTDIR")
            {
                return Err(error.into());
            }
            None
        }
    };
    pi_js_compat::process_cwd::chdir(target).map_err(CallbackError::from)?;
    let prefix = previous
        .filter(|cwd| !cwd.is_empty())
        .map(|cwd| JsString::from(" ") + cwd)
        .unwrap_or_default();
    Ok(Some(pi_js_compat::js_format!(
        "Deleted process working directory{}; recovered in {}.",
        prefix,
        target
    )))
}

type SharedRuntimeWeak = std::sync::Weak<RuntimeInner>;

fn upgrade_runtime(weak: &SharedRuntimeWeak) -> Result<AgentSessionRuntime, HandlerError> {
    weak.upgrade()
        .map(|inner| AgentSessionRuntime { inner })
        .ok_or_else(|| HandlerError::new("Agent session runtime is not available"))
}

fn build_before_working_directory_recovery_handler(
    weak: &SharedRuntimeWeak,
) -> crate::core::agent_session::BeforeWorkingDirectoryRecoveryRun {
    let weak = weak.clone();
    Arc::new(move || {
        let weak = weak.clone();
        Box::pin(async move {
            let runtime = upgrade_runtime(&weak).map_err(CallbackError::from)?;
            runtime.recover_working_directory_before_run().await
        })
    })
}

fn build_after_working_directory_recovery_handler(
    weak: &SharedRuntimeWeak,
) -> crate::core::agent_session::AfterWorkingDirectoryRecoveryTurn {
    let weak = weak.clone();
    Arc::new(move || {
        let weak = weak.clone();
        Box::pin(async move {
            let runtime = upgrade_runtime(&weak).map_err(CallbackError::from)?;
            runtime.prepare_working_directory_recovery().await
        })
    })
}

fn build_relocate_handler(weak: &SharedRuntimeWeak) -> crate::core::extensions::types::RelocateCallback {
    let weak = weak.clone();
    Arc::new(move |target_cwd: &JsString| {
        let weak = weak.clone();
        let target_cwd = target_cwd.to_owned();
        Box::pin(async move {
            let shared = upgrade_runtime(&weak)?;
            shared.relocate(&target_cwd, None).await.map_err(HandlerError::from)
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
                .prepare_tool_result_relocation(&target_cwd)
                .await
                .map_err(HandlerError::from)
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
                .activate_tool_result_relocation()
                .await
                .map_err(HandlerError::from)
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
) -> Result<AgentSessionRuntime, CallbackError> {
    if no_supervisor == Some(true) && session_manager.is_persisted() {
        return Err("Standalone worker requires an in-memory session".into());
    }
    assert_session_cwd_exists(&session_manager, cwd)?;
    if no_supervisor != Some(true)
        && let Some(control_db_path) = session_manager.get_metadata_control_db_path()
    {
        crate::core::detached_job_cleanup::run_detached_job_artifact_cleanup(control_db_path, Some(agent_dir), None)?;
        crate::core::session_archive_storage::run_idle_session_archive(
            control_db_path,
            agent_dir,
            session_manager.get_session_file(),
        );
    }
    let result = create_runtime(CreateAgentSessionRuntimeOptions {
        project_trust_context: None,
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
