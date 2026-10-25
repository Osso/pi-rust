//! Shared harness for first-party tool tests.
//!
//! Each integration test binary includes this module directly, so helpers used by
//! other binaries are dead code here.
#![allow(dead_code)]
//!
//! Tools receive the live [`ExtensionContext`], so tests build a real
//! `ExtensionRunner` over an in-memory or persisted `SessionManager`.

use std::sync::{Arc, Mutex};

use pi_agent_core::harness::ThinkingLevel;
use pi_coding_agent::core::extensions::{
    ExtensionActions, ExtensionCommandContext, ExtensionCommandContextActions, ExtensionContext,
    ExtensionContextActions, ExtensionMode, ExtensionRunner, ExtensionRuntime, ExtensionUiContext, ProviderActions,
    ReplacedSessionContext, SessionTransitionResult, SwitchSessionCallback, UserMessageContent,
    create_extension_runtime,
};
use pi_coding_agent::core::model_registry::ModelRegistry;
use pi_coding_agent::core::session_manager::{NewSessionOptions, SessionManager, SharedSessionManager};
use pi_js_compat::js_json::{JsString, JsValue as Value};

pub fn in_memory_session(cwd: impl Into<JsString>, id: impl Into<JsString>) -> SharedSessionManager {
    let cwd = cwd.into();
    let id = id.into();
    Arc::new(Mutex::new(
        SessionManager::in_memory(
            Some(&cwd),
            Some(NewSessionOptions {
                id: Some(id),
                ..Default::default()
            }),
        )
        .expect("in-memory session manager"),
    ))
}

pub fn persisted_session(
    cwd: impl Into<JsString>,
    session_dir: impl Into<JsString>,
    id: impl Into<JsString>,
) -> SharedSessionManager {
    let cwd = cwd.into();
    let session_dir = session_dir.into();
    let id = id.into();
    Arc::new(Mutex::new(
        SessionManager::create(
            &cwd,
            Some(&session_dir),
            Some(NewSessionOptions {
                id: Some(id),
                ..Default::default()
            }),
        )
        .expect("persisted session manager"),
    ))
}

pub fn command_context_actions(switch_session: SwitchSessionCallback) -> ExtensionCommandContextActions {
    ExtensionCommandContextActions {
        show_approval_selector: Arc::new(|| Ok(())),
        show_sandbox_selector: Arc::new(|| Ok(())),
        wait_for_idle: Arc::new(|| Box::pin(async {})),
        new_session: Arc::new(|_| Box::pin(async { Ok(SessionTransitionResult { cancelled: false }) })),
        fork: Arc::new(|_, _| Box::pin(async { Ok(SessionTransitionResult { cancelled: false }) })),
        navigate_tree: Arc::new(|_, _| Box::pin(async { Ok(SessionTransitionResult { cancelled: false }) })),
        switch_session,
        reload: Arc::new(|| Box::pin(async { Ok(()) })),
        restart: Arc::new(|_| Box::pin(async { Ok(()) })),
    }
}

/// `ctx.switchSession` recorder that runs `withSession` against a replaced context.
pub fn recording_switch_session(
    context: ExtensionContext,
    cancelled: bool,
    path: Arc<Mutex<Option<JsString>>>,
    starter_prompt: Arc<Mutex<Option<JsString>>>,
) -> SwitchSessionCallback {
    Arc::new(move |session_path, options| {
        *path.lock().expect("path lock") = Some(session_path);
        let with_session = options.and_then(|options| options.with_session);
        let context = context.clone();
        let starter_prompt = Arc::clone(&starter_prompt);
        Box::pin(async move {
            if let Some(with_session) = with_session {
                let replaced = ReplacedSessionContext {
                    command: ExtensionCommandContext { context },
                    send_message: Arc::new(|_, _| Box::pin(async { Ok(()) })),
                    send_user_message: Arc::new(move |content, _| {
                        if let UserMessageContent::Text(text) = content {
                            *starter_prompt.lock().expect("starter lock") = Some(text);
                        }
                        Box::pin(async { Ok(()) })
                    }),
                };
                with_session(replaced).await?;
            }
            Ok(SessionTransitionResult { cancelled })
        })
    })
}

pub struct TestContextBuilder {
    cwd: JsString,
    session: SharedSessionManager,
    control_db_path: Option<JsString>,
    mode: ExtensionMode,
    ui: Option<Arc<dyn ExtensionUiContext>>,
    agent_id: Option<JsString>,
    requires_agent_id: bool,
    no_supervisor: bool,
    model_registry: Option<Arc<ModelRegistry>>,
    provider_actions: Option<ProviderActions>,
    model: Option<pi_ai::types::Model>,
}

impl TestContextBuilder {
    pub fn new(cwd: impl Into<JsString>, session: SharedSessionManager) -> Self {
        Self {
            cwd: cwd.into(),
            session,
            control_db_path: None,
            mode: ExtensionMode::Print,
            ui: None,
            agent_id: None,
            requires_agent_id: false,
            no_supervisor: false,
            model_registry: None,
            provider_actions: None,
            model: None,
        }
    }

    pub fn control_db_path(mut self, path: impl Into<JsString>) -> Self {
        self.control_db_path = Some(path.into());
        self
    }

    pub fn mode(mut self, mode: ExtensionMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn ui(mut self, ui: Arc<dyn ExtensionUiContext>) -> Self {
        self.ui = Some(ui);
        self
    }

    pub fn agent_id(mut self, agent_id: impl Into<JsString>) -> Self {
        self.agent_id = Some(agent_id.into());
        self
    }

    pub fn requires_agent_id(mut self, requires_agent_id: bool) -> Self {
        self.requires_agent_id = requires_agent_id;
        self
    }

    pub fn no_supervisor(mut self, no_supervisor: bool) -> Self {
        self.no_supervisor = no_supervisor;
        self
    }

    pub fn model_registry(mut self, registry: Arc<ModelRegistry>) -> Self {
        self.model_registry = Some(registry);
        self
    }

    pub fn model(mut self, model: pi_ai::types::Model) -> Self {
        self.model = Some(model);
        self
    }

    pub fn provider_actions(mut self, actions: ProviderActions) -> Self {
        self.provider_actions = Some(actions);
        self
    }

    pub async fn build(self) -> (ExtensionRunner, ExtensionContext) {
        let (runner, context, _) = self.build_with_runtime().await;
        (runner, context)
    }

    pub async fn build_with_runtime(self) -> (ExtensionRunner, ExtensionContext, ExtensionRuntime) {
        let runtime = create_extension_runtime();
        let registry = match self.model_registry {
            Some(registry) => registry,
            None => model_registry().await,
        };
        let mut runner = ExtensionRunner::new(
            Vec::new(),
            runtime.clone(),
            self.cwd,
            Arc::clone(&self.session),
            registry,
            None,
        );
        runner
            .bind_core(
                actions(),
                context_actions(
                    self.control_db_path,
                    self.agent_id,
                    self.requires_agent_id,
                    self.no_supervisor,
                    self.model,
                ),
                self.provider_actions,
            )
            .unwrap();
        runner.set_ui_context(self.ui, self.mode);
        runner.bind_command_context(None);
        let context = runner.create_context();
        (runner, context, runtime)
    }
}

async fn model_registry() -> Arc<ModelRegistry> {
    Arc::new(
        ModelRegistry::in_memory(pi_coding_agent::core::auth_storage::AuthStorage::in_memory(
            serde_json::Map::new(),
        ))
        .unwrap(),
    )
}

fn actions() -> ExtensionActions {
    ExtensionActions {
        send_message: Arc::new(|_, _| Ok(())),
        send_user_message: Arc::new(|_, _| Ok(())),
        append_entry: Arc::new(|_, _| Ok(())),
        request_resume_continuation: Arc::new(|| {}),
        set_session_name: Arc::new(|_| Ok(())),
        get_session_name: Arc::new(|| Ok(None)),
        set_label: Arc::new(|_, _| Ok(())),
        get_active_tools: Arc::new(|| Ok(Vec::new())),
        get_all_tools: Arc::new(|| Ok(Vec::new())),
        set_active_tools: Arc::new(|_| Ok(())),
        call_tool: Arc::new(|_, _, _, _| {
            Box::pin(async {
                Ok(pi_agent_core::AgentToolResult {
                    content: Vec::new(),
                    details: None,
                    is_error: None,
                    terminate: None,
                })
            })
        }),
        call_command: Arc::new(|_, _| Box::pin(async { Ok(Value::Null) })),
        refresh_tools: Arc::new(|| Ok(())),
        get_commands: Arc::new(|| Ok(Vec::new())),
        set_model: Arc::new(|_| Box::pin(async { Ok(false) })),
        get_thinking_level: Arc::new(|| Ok(ThinkingLevel::Off)),
        set_thinking_level: Arc::new(|_| Ok(())),
    }
}

fn context_actions(
    control_db_path: Option<JsString>,
    agent_id: Option<JsString>,
    requires_agent_id: bool,
    no_supervisor: bool,
    model: Option<pi_ai::types::Model>,
) -> ExtensionContextActions {
    ExtensionContextActions {
        get_model: Arc::new(move || model.clone()),
        get_thinking_level: None,
        set_thinking_level: None,
        get_scoped_models: None,
        get_footer_data: None,
        is_idle: Arc::new(|| true),
        has_active_retry: Arc::new(|| false),
        is_project_trusted: Arc::new(|| true),
        get_signal: Arc::new(|| None),
        abort: Arc::new(|| {}),
        has_pending_messages: Arc::new(|| false),
        shutdown: Arc::new(|| {}),
        restart: Arc::new(|_| Box::pin(async { Ok(()) })),
        get_control_db_path: Some(Arc::new(move || control_db_path.clone())),
        get_context_usage: Arc::new(|| None),
        get_multi_agent_agent_id: Some(Arc::new(move || agent_id.clone())),
        get_multi_agent_parent_session_id: None,
        register_supervisor_review_cancellation: None,
        get_no_supervisor: Some(Arc::new(move || no_supervisor)),
        get_multi_agent_requires_agent_id: Some(Arc::new(move || requires_agent_id)),
        get_multi_agent_store: None,
        get_tool_detach_registry: None,
        compact: Arc::new(|_| {}),
        get_system_prompt: Arc::new(JsString::default),
        get_system_prompt_options: None,
    }
}
