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
use pi_ai::auth::types::CredentialStore;
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::extensions::{
    ExtensionActions, ExtensionCommandContext, ExtensionCommandContextActions, ExtensionContext,
    ExtensionContextActions, ExtensionMode, ExtensionRunner, ExtensionUiContext, ReplacedSessionContext,
    SessionTransitionResult, SwitchSessionCallback, UserMessageContent, create_extension_runtime,
};
use pi_coding_agent::core::model_registry::ModelRegistry;
use pi_coding_agent::core::model_runtime::{CreateModelRuntimeOptions, ModelRuntime};
use pi_coding_agent::core::session_manager::{NewSessionOptions, SessionManager};
use serde_json::{Map, Value};

pub fn in_memory_session(cwd: &str, id: &str) -> Arc<SessionManager> {
    Arc::new(
        SessionManager::in_memory(
            Some(cwd),
            Some(NewSessionOptions {
                id: Some(id.to_owned()),
                ..Default::default()
            }),
        )
        .expect("in-memory session manager"),
    )
}

pub fn persisted_session(cwd: &str, session_dir: &str, id: &str) -> Arc<SessionManager> {
    Arc::new(
        SessionManager::create(
            cwd,
            Some(session_dir),
            Some(NewSessionOptions {
                id: Some(id.to_owned()),
                ..Default::default()
            }),
        )
        .expect("persisted session manager"),
    )
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
        reload: Arc::new(|| Box::pin(async {})),
        restart: Arc::new(|_| Box::pin(async {})),
    }
}

/// `ctx.switchSession` recorder that runs `withSession` against a replaced context.
pub fn recording_switch_session(
    context: ExtensionContext,
    cancelled: bool,
    path: Arc<Mutex<Option<String>>>,
    starter_prompt: Arc<Mutex<Option<String>>>,
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
    cwd: String,
    session: Arc<SessionManager>,
    control_db_path: Option<String>,
    mode: ExtensionMode,
    ui: Option<Arc<dyn ExtensionUiContext>>,
    agent_id: Option<String>,
    requires_agent_id: bool,
}

impl TestContextBuilder {
    pub fn new(cwd: &str, session: Arc<SessionManager>) -> Self {
        Self {
            cwd: cwd.to_owned(),
            session,
            control_db_path: None,
            mode: ExtensionMode::Print,
            ui: None,
            agent_id: None,
            requires_agent_id: false,
        }
    }

    pub fn control_db_path(mut self, path: &str) -> Self {
        self.control_db_path = Some(path.to_owned());
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

    pub fn agent_id(mut self, agent_id: &str) -> Self {
        self.agent_id = Some(agent_id.to_owned());
        self
    }

    pub fn requires_agent_id(mut self, requires_agent_id: bool) -> Self {
        self.requires_agent_id = requires_agent_id;
        self
    }

    pub async fn build(self) -> (ExtensionRunner, ExtensionContext) {
        let mut runner = ExtensionRunner::new(
            Vec::new(),
            create_extension_runtime(),
            self.cwd,
            Arc::clone(&self.session),
            model_registry().await,
            None,
        );
        runner.bind_core(
            actions(),
            context_actions(self.control_db_path, self.agent_id, self.requires_agent_id),
            None,
        );
        runner.set_ui_context(self.ui, self.mode);
        runner.bind_command_context(None);
        let context = runner.create_context();
        (runner, context)
    }
}

async fn model_registry() -> Arc<ModelRegistry> {
    let storage = Arc::new(AuthStorage::in_memory(Map::new()));
    let runtime = ModelRuntime::create(CreateModelRuntimeOptions {
        credentials: Some(storage as Arc<dyn CredentialStore>),
        models_path: None,
        ..Default::default()
    })
    .await
    .expect("in-memory model runtime");
    Arc::new(ModelRegistry::new(runtime))
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
                    usage: None,
                    added_tool_names: None,
                    is_error: None,
                    terminate: None,
                })
            })
        }),
        call_command: Arc::new(|_, _| Box::pin(async { Ok(Value::Null) })),
        refresh_tools: Arc::new(|| {}),
        get_commands: Arc::new(|| Ok(Vec::new())),
        set_model: Arc::new(|_| Box::pin(async { Ok(false) })),
        get_thinking_level: Arc::new(|| Ok(ThinkingLevel::Off)),
        set_thinking_level: Arc::new(|_| Ok(())),
    }
}

fn context_actions(
    control_db_path: Option<String>,
    agent_id: Option<String>,
    requires_agent_id: bool,
) -> ExtensionContextActions {
    ExtensionContextActions {
        get_model: Arc::new(|| None),
        get_thinking_level: None,
        get_scoped_models: None,
        get_footer_data: None,
        is_idle: Arc::new(|| true),
        has_active_retry: Arc::new(|| false),
        is_project_trusted: Arc::new(|| true),
        get_signal: Arc::new(|| None),
        abort: Arc::new(|| {}),
        has_pending_messages: Arc::new(|| false),
        shutdown: Arc::new(|| {}),
        restart: Arc::new(|_| Box::pin(async {})),
        get_control_db_path: Some(Arc::new(move || control_db_path.clone())),
        get_context_usage: Arc::new(|| None),
        get_multi_agent_agent_id: Some(Arc::new(move || agent_id.clone())),
        get_multi_agent_parent_session_id: None,
        get_multi_agent_requires_agent_id: Some(Arc::new(move || requires_agent_id)),
        get_multi_agent_store: None,
        get_tool_detach_registry: None,
        compact: Arc::new(|_| {}),
        get_system_prompt: Arc::new(String::new),
        get_system_prompt_options: None,
    }
}
