//! Multi-agent extension options and runtime handles
//! (`extensions/agents-core/src/runtime.ts`).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use futures_util::future::Shared;
use pi_agent_core::AgentToolResult;
use pi_ai::request::BoxFuture;
use serde_json::{Map, Value, json};
use tokio_util::sync::CancellationToken;

use crate::core::desktop_notification::{DesktopNotifier, send_desktop_notification};
use crate::core::extensions::types::{
    ExtensionApi, ExtensionCommandContext, ExtensionContext, HandlerError, NotifyType, ToolDefinition,
};
use crate::core::multi_agent_store::{AgentSnapshot, MultiAgentStore, MultiAgentStoreOptions, is_active_lifecycle};
use crate::core::session_manager::{KnownSessionEntry, SessionEntry, lock_session_manager};
use crate::core::sqlite::ControlDbError;

use super::control_runtime::OwnedAgentRuntime;
use super::dispatch_runtime::WaitingDesktopNotificationHandles;
use super::mailbox_runtime::{OnSessionMessageSent, RuntimeLifecycleMirror};
use super::parent_agent_journal::JournalWriter;
use super::shared::{
    AttachedSessionFactory, CHILD_ORCHESTRATION_UNAVAILABLE_MESSAGE, ChildAgentSession, ChildAgentSessionFactory,
    WaitWakeListener, WaitWakeListeners,
};
use super::viewer::AgentViewerParams;

pub const MAIN_THREAD_AGENT_ID: &str = "main";
const MESSAGE_CONTENT_LIMIT: usize = 2000;

pub(crate) struct NoopDesktopNotifier;

impl DesktopNotifier for NoopDesktopNotifier {
    fn notify(
        &self,
        notification: &crate::core::desktop_notification::DesktopNotification,
    ) -> Option<Box<dyn crate::core::desktop_notification::DesktopNotificationHandle>> {
        send_desktop_notification(notification)
    }
}

pub type SharedDispatch = Shared<BoxFuture<'static, Result<AgentSnapshot, String>>>;

#[derive(Clone, Default)]
pub struct MultiAgentRuntimeHandles {
    pub dispatches: Arc<Mutex<HashMap<String, SharedDispatch>>>,
    pub ownerships: Arc<Mutex<HashMap<String, Arc<OwnedAgentRuntime>>>>,
    pub sessions: Arc<Mutex<HashMap<String, Arc<dyn ChildAgentSession>>>>,
    pub wait_wake_listeners: WaitWakeListeners,
}

impl MultiAgentRuntimeHandles {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_wait_wake_listener(&self, listener: WaitWakeListener) -> super::shared::WaitWakeUnsubscribe {
        self.wait_wake_listeners.add(listener)
    }
}

pub type SelectAgentView = Arc<dyn Fn(&str) -> Option<bool> + Send + Sync>;

#[derive(Clone, Default)]
pub struct MultiAgentExtensionOptions {
    pub create_attached_session: Option<AttachedSessionFactory>,
    pub create_child_session: Option<ChildAgentSessionFactory>,
    pub desktop_notifier: Option<Arc<dyn DesktopNotifier>>,
    pub runtime_handles: Option<MultiAgentRuntimeHandles>,
    pub select_agent_view: Option<SelectAgentView>,
    pub on_session_message_sent: Option<OnSessionMessageSent>,
    pub store: Option<Arc<MultiAgentStore>>,
}

pub fn resolve_multi_agent_store(options: &MultiAgentExtensionOptions) -> Arc<MultiAgentStore> {
    match options.store.as_ref() {
        Some(store) => Arc::clone(store),
        None => Arc::new(MultiAgentStore::new(MultiAgentStoreOptions::default())),
    }
}

pub fn create_multi_agent_runtime_handles() -> MultiAgentRuntimeHandles {
    MultiAgentRuntimeHandles::new()
}

pub fn wake_wait_agents_after_coordination(runtime_handles: &MultiAgentRuntimeHandles, prompt: &str) {
    runtime_handles
        .wait_wake_listeners
        .emit(&super::shared::WaitAgentsWakeUp::coordination(prompt.to_owned()));
}

/// `resolveSelectedSessionMutationTarget`.
pub fn resolve_selected_session_mutation_target(
    store: &MultiAgentStore,
    runtime_handles: &MultiAgentRuntimeHandles,
) -> Result<Option<Arc<crate::core::extensions::types::ViewedSessionMutationTarget>>, HandlerError> {
    use crate::core::multi_agent_store::format_inactive_agent_selection_message;

    let Some(selected_agent_id) = store.get_selected_agent_id() else {
        return Ok(None);
    };
    if selected_agent_id == MAIN_THREAD_AGENT_ID {
        return Ok(None);
    }
    let Some(agent) = store.get_agent(&selected_agent_id) else {
        return Err(HandlerError::new(format!("Agent not found: {selected_agent_id}")));
    };
    if agent.agent_type == "background" {
        return Err(HandlerError::new(format!(
            "Agent {selected_agent_id} is detached and not a live child session"
        )));
    }
    if !is_active_lifecycle(agent.lifecycle) {
        return Err(HandlerError::new(format_inactive_agent_selection_message(&agent)));
    }
    let sessions = runtime_handles.sessions.lock().expect("sessions");
    let Some(session) = sessions.get(&selected_agent_id) else {
        return Err(HandlerError::new(format!(
            "Agent {selected_agent_id} is not a live child session"
        )));
    };
    let Some(target) = session.session_mutation_target() else {
        return Err(HandlerError::new(format!(
            "Agent {selected_agent_id} does not support live session mutation"
        )));
    };
    Ok(Some(target))
}

pub fn assert_no_wait_agents_params(params: &Value, api_name: &str) -> Result<(), HandlerError> {
    let empty = match params {
        Value::Null => true,
        Value::Object(object) => object.is_empty(),
        _ => false,
    };
    if empty {
        Ok(())
    } else {
        Err(HandlerError::new(format!("{api_name} does not accept parameters")))
    }
}

pub fn format_agent_status(agent: &AgentSnapshot) -> String {
    let summary = agent
        .result
        .as_ref()
        .and_then(|result| result.summary.as_deref())
        .map(pi_js_compat::string::js_trim)
        .unwrap_or_default();
    if !summary.is_empty() {
        return format!(
            "{} is {}: {summary}",
            agent.display_name,
            lifecycle_name(agent.lifecycle)
        );
    }
    let error_message = agent
        .error
        .as_ref()
        .map(|error| pi_js_compat::string::js_trim(&error.message))
        .unwrap_or_default();
    if !error_message.is_empty() {
        return format!(
            "{} is {}: {error_message}",
            agent.display_name,
            lifecycle_name(agent.lifecycle)
        );
    }
    format!("{} is {}.", agent.display_name, lifecycle_name(agent.lifecycle))
}

fn lifecycle_name(lifecycle: crate::core::multi_agent_store::AgentLifecycle) -> &'static str {
    use crate::core::multi_agent_store::AgentLifecycle;
    match lifecycle {
        AgentLifecycle::Running => "running",
        AgentLifecycle::WaitingForInput => "waiting_for_input",
        AgentLifecycle::SteeringPending => "steering_pending",
        AgentLifecycle::Cancelling => "cancelling",
        AgentLifecycle::Completed => "completed",
        AgentLifecycle::Failed => "failed",
        AgentLifecycle::Aborted => "aborted",
    }
}

pub async fn wait_agents(
    store: &Arc<MultiAgentStore>,
    runtime_handles: &MultiAgentRuntimeHandles,
    signal: Option<CancellationToken>,
    ctx: Option<&ExtensionContext>,
) -> Result<AgentToolResult, HandlerError> {
    let wake =
        super::wait_runtime::wait_notifications_runtime(store, &runtime_handles.wait_wake_listeners, signal, ctx)
            .await?;
    super::wait_runtime::consume_notifications_runtime(store, &wake, ctx, &format_agent_status)
}

pub async fn wait_notifications(
    store: &Arc<MultiAgentStore>,
    runtime_handles: &MultiAgentRuntimeHandles,
    signal: Option<CancellationToken>,
    ctx: Option<&ExtensionContext>,
) -> Result<super::wait_runtime::WaitNotificationsWake, HandlerError> {
    super::wait_runtime::wait_notifications_runtime(store, &runtime_handles.wait_wake_listeners, signal, ctx).await
}

pub fn consume_notifications(
    store: &Arc<MultiAgentStore>,
    wake: &super::wait_runtime::WaitNotificationsWake,
    ctx: Option<&ExtensionContext>,
) -> Result<AgentToolResult, HandlerError> {
    super::wait_runtime::consume_notifications_runtime(store, wake, ctx, &format_agent_status)
}

pub fn deliver_terminal_outbox_for_store(store: &Arc<MultiAgentStore>) -> Result<u32, ControlDbError> {
    let Some(persistence) = store.get_persistence_target() else {
        return Ok(0);
    };
    crate::core::terminal_outbox_delivery::deliver_terminal_outbox_projections(
        &crate::core::terminal_outbox_delivery::DeliverTerminalOutboxOptions {
            artifact_root: None,
            claim_id: pi_ai::utils::uuid::random_uuid(),
            control_db_path: persistence.control_db_path,
            now: Arc::new(crate::utils::iso_time::now_iso_string),
            store: Arc::clone(store),
        },
    )
}

pub fn jobs_command(store: &MultiAgentStore, ctx: &ExtensionCommandContext) -> Result<(), HandlerError> {
    let agents: Vec<AgentSnapshot> = store
        .list_agents()
        .into_iter()
        .filter(|agent| agent.agent_type == "background")
        .collect();
    let ui = ctx.ui()?;
    if agents.is_empty() {
        ui.notify("No background jobs.", Some(NotifyType::Info));
        return Ok(());
    }
    ui.notify(
        &agents
            .iter()
            .map(|agent| format!("{} {}", agent.id, format_agent_status(agent)))
            .collect::<Vec<_>>()
            .join("\n"),
        Some(NotifyType::Info),
    );
    Ok(())
}

pub struct MultiAgentPiRequestHandler {
    runtime: Arc<PiRequestRuntime>,
    mirror: RuntimeLifecycleMirror,
    disposed: AtomicBool,
}

struct PiRequestRuntime {
    desktop_notifier: Arc<dyn DesktopNotifier>,
    options: MultiAgentExtensionOptions,
    pi: Option<JournalWriter>,
    runtime_handles: MultiAgentRuntimeHandles,
    store: Arc<MultiAgentStore>,
    waiting_desktop_notifications: WaitingDesktopNotificationHandles,
}

impl MultiAgentPiRequestHandler {
    pub fn dispose(&self) {
        self.disposed.store(true, Ordering::SeqCst);
        self.mirror.dispose();
    }

    /// Whether this handler owns `method`; unowned methods fall through to the
    /// next Pi request dispatcher.
    pub fn handles_method(&self, method: &str) -> bool {
        matches!(
            method,
            "agents.spawn"
                | "agents.wait"
                | "agents.attachSession"
                | "agents.list"
                | "agents.current"
                | "agents.select"
                | "messages.last"
                | "messages.send"
        )
    }

    pub async fn handle(
        &self,
        request: &Value,
        ctx: &ExtensionContext,
        signal: Option<CancellationToken>,
        active_tool_call_id: Option<String>,
    ) -> Result<Value, HandlerError> {
        if self.disposed.load(Ordering::SeqCst) {
            return Err(HandlerError::new("Multi-agent Pi request handler is disposed"));
        }
        let method = request.get("method").and_then(Value::as_str).unwrap_or_default();
        let is_child_runtime = super::shared::is_child_agent_runtime(ctx)?;
        if !is_child_runtime {
            self.mirror.bind(ctx)?;
        }
        if is_child_runtime && is_supervisor_only_agent_request(method) {
            return Err(HandlerError::new(CHILD_ORCHESTRATION_UNAVAILABLE_MESSAGE));
        }
        let params = request.get("params").cloned().unwrap_or(Value::Null);
        self.dispatch(method, params, ctx, signal, active_tool_call_id).await
    }

    async fn dispatch(
        &self,
        method: &str,
        params: Value,
        ctx: &ExtensionContext,
        signal: Option<CancellationToken>,
        active_tool_call_id: Option<String>,
    ) -> Result<Value, HandlerError> {
        match method {
            "agents.spawn" => {
                let result =
                    super::spawn_attach_runtime::spawn_agent(super::spawn_attach_runtime::SpawnAgentRuntimeInput {
                        active_tool_call_id,
                        create_child_session: self.runtime.options.create_child_session.clone(),
                        ctx: ctx.clone(),
                        desktop_notifier: Arc::clone(&self.runtime.desktop_notifier),
                        dispatches: Arc::clone(&self.runtime.runtime_handles.dispatches),
                        handles: Some(Arc::clone(&self.runtime.runtime_handles.sessions)),
                        ownerships: Arc::clone(&self.runtime.runtime_handles.ownerships),
                        params,
                        pi: self.runtime.pi.clone(),
                        store: Arc::clone(&self.runtime.store),
                        waiting_desktop_notifications: Arc::clone(&self.runtime.waiting_desktop_notifications),
                        _marker: std::marker::PhantomData,
                    })
                    .await?;
                Ok(result.details.unwrap_or(Value::Null))
            }
            "agents.wait" => {
                assert_no_wait_agents_params(&params, "pi.agents.wait")?;
                wait_agents(&self.runtime.store, &self.runtime.runtime_handles, signal, Some(ctx)).await?;
                Ok(Value::Null)
            }
            "agents.attachSession" => {
                let params: super::spawn_attach_runtime::AttachSessionAgentParams =
                    serde_json::from_value(params).map_err(|error| HandlerError::new(error.to_string()))?;
                let result = super::spawn_attach_runtime::attach_session_agent(
                    &super::spawn_attach_runtime::AttachSessionAgentRuntimeInput {
                        create_attached_session: self.runtime.options.create_attached_session.clone(),
                        ctx: ctx.clone(),
                        desktop_notifier: Arc::clone(&self.runtime.desktop_notifier),
                        dispatches: Arc::clone(&self.runtime.runtime_handles.dispatches),
                        handles: Some(Arc::clone(&self.runtime.runtime_handles.sessions)),
                        params,
                        ownerships: Arc::clone(&self.runtime.runtime_handles.ownerships),
                        store: Arc::clone(&self.runtime.store),
                        waiting_desktop_notifications: Arc::clone(&self.runtime.waiting_desktop_notifications),
                    },
                )
                .await?;
                Ok(result.details.unwrap_or(Value::Null))
            }
            "agents.list" => {
                let params: super::viewer::ListAgentsParams = if params.is_null() {
                    Default::default()
                } else {
                    serde_json::from_value(params).map_err(|error| HandlerError::new(error.to_string()))?
                };
                let result = super::viewer::list_agents(&self.runtime.store, &params)?;
                Ok(result.details.unwrap_or(Value::Null))
            }
            "agents.current" => select_current_agent(&self.runtime.store),
            "agents.select" => select_agent(
                &self.runtime.store,
                &params,
                self.runtime.options.select_agent_view.as_ref(),
            ),
            "messages.last" => {
                let session_manager = ctx.session_manager()?;
                let manager = lock_session_manager(&session_manager);
                Ok(find_last_session_message(&manager.get_branch(None)))
            }
            "messages.send" => {
                let params: super::mailbox_runtime::SendAgentMessageParams =
                    serde_json::from_value(params).map_err(|error| HandlerError::new(error.to_string()))?;
                let result = super::mailbox_runtime::send_agent_message(
                    &self.runtime.store,
                    &params,
                    Some(ctx),
                    self.runtime.options.on_session_message_sent.as_ref(),
                )?;
                Ok(result.details.unwrap_or(Value::Null))
            }
            _ => Ok(Value::Null),
        }
    }
}

pub fn create_multi_agent_pi_request_handler(
    options: &MultiAgentExtensionOptions,
    pi: Option<JournalWriter>,
) -> MultiAgentPiRequestHandler {
    let store = resolve_multi_agent_store(options);
    let runtime_handles = options
        .runtime_handles
        .clone()
        .unwrap_or_else(create_multi_agent_runtime_handles);
    let mirror = RuntimeLifecycleMirror::new(Arc::clone(&store));
    MultiAgentPiRequestHandler {
        runtime: Arc::new(PiRequestRuntime {
            desktop_notifier: options
                .desktop_notifier
                .clone()
                .unwrap_or_else(|| Arc::new(NoopDesktopNotifier)),
            options: options.clone(),
            pi,
            runtime_handles,
            store,
            waiting_desktop_notifications: Arc::new(Mutex::new(HashMap::new())),
        }),
        mirror,
        disposed: AtomicBool::new(false),
    }
}

fn is_supervisor_only_agent_request(method: &str) -> bool {
    matches!(method, "agents.spawn" | "agents.attachSession" | "agents.wait")
}

fn select_current_agent(store: &MultiAgentStore) -> Result<Value, HandlerError> {
    let selected = store
        .get_selected_agent_id()
        .and_then(|agent_id| store.get_agent(&agent_id))
        .filter(|agent| is_active_lifecycle(agent.lifecycle));
    Ok(match selected {
        Some(agent) => json!({"agent": agent}),
        None => json!({"agent": create_main_thread_snapshot()}),
    })
}

fn select_agent(
    store: &MultiAgentStore,
    params: &Value,
    select_agent_view: Option<&SelectAgentView>,
) -> Result<Value, HandlerError> {
    let agent_id = normalize_select_agent_id(params)?;
    let rendered = select_agent_view.map(|callback| callback(&agent_id));
    if rendered == Some(Some(true)) {
        if agent_id == MAIN_THREAD_AGENT_ID {
            return Ok(json!({"agent": create_main_thread_snapshot()}));
        }
        if let Some(selected) = store.get_agent(&agent_id) {
            return Ok(json!({"agent": selected}));
        }
        return select_current_agent(store);
    }
    if rendered == Some(Some(false)) {
        return Err(HandlerError::new(format!("Agent view selection failed: {agent_id}")));
    }
    if agent_id == MAIN_THREAD_AGENT_ID {
        store.clear_selected_agent_view();
        return Ok(json!({"agent": create_main_thread_snapshot()}));
    }
    match store.select_agent_view_with_status(&agent_id) {
        crate::core::multi_agent_store::AgentViewSelectionResult::Ok(agent) => Ok(json!({"agent": agent})),
        crate::core::multi_agent_store::AgentViewSelectionResult::NotFound { .. } => {
            Err(HandlerError::new(format!("Agent not found: {agent_id}")))
        }
    }
}

fn normalize_select_agent_id(params: &Value) -> Result<String, HandlerError> {
    if let Some(agent_id) = params.as_str() {
        return Ok(agent_id.to_owned());
    }
    let Some(object) = params.as_object() else {
        return Err(HandlerError::new(
            "pi.agents.select requires an agent id string or { agentId } object",
        ));
    };
    let Some(agent_id) = object.get("agentId").and_then(Value::as_str) else {
        return Err(HandlerError::new("pi.agents.select requires a non-empty agentId"));
    };
    if pi_js_compat::string::js_trim(agent_id).is_empty() {
        return Err(HandlerError::new("pi.agents.select requires a non-empty agentId"));
    }
    Ok(agent_id.to_owned())
}

fn create_main_thread_snapshot() -> Value {
    json!({
        "displayName": "Main thread",
        "id": MAIN_THREAD_AGENT_ID,
        "lifecycle": "current",
        "selected": true,
    })
}

fn find_last_session_message(entries: &[SessionEntry]) -> Value {
    for entry in entries.iter().rev() {
        if let SessionEntry::Known(KnownSessionEntry::Message(message)) = entry {
            return summarize_session_message(message);
        }
    }
    Value::Null
}

fn summarize_session_message(entry: &crate::core::session_manager::SessionMessageEntry) -> Value {
    let message = &entry.message;
    let role = message.role();
    let mut details = Map::new();
    details.insert("entryId".to_owned(), json!(entry.id));
    details.insert("role".to_owned(), json!(role));
    if let Some(text) = summarize_message_content(message) {
        details.insert("content".to_owned(), json!(text.value));
        details.insert("text".to_owned(), json!(text.value));
        if text.truncated {
            details.insert("truncated".to_owned(), json!(true));
        }
    }
    Value::Object(details)
}

struct SummarizedText {
    truncated: bool,
    value: String,
}

fn summarize_message_content(message: &pi_agent_core::AgentMessage) -> Option<SummarizedText> {
    let content = match message {
        pi_agent_core::AgentMessage::Assistant(assistant) => {
            let text = assistant
                .content
                .iter()
                .filter_map(|part| match part {
                    pi_ai::types::AssistantContent::Text(text) => Some(text.text.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            return (!text.is_empty()).then(|| truncate_text(&text));
        }
        pi_agent_core::AgentMessage::User(user) => match &user.content {
            pi_ai::types::UserContent::Text(text) => return Some(truncate_text(text)),
            pi_ai::types::UserContent::Blocks(blocks) => blocks,
        },
        pi_agent_core::AgentMessage::ToolResult(result) => &result.content,
        pi_agent_core::AgentMessage::Custom(object) => {
            let content = object.get("content")?;
            if let Some(text) = content.as_str() {
                return Some(truncate_text(text));
            }
            let parts: Vec<String> = content
                .as_array()?
                .iter()
                .filter_map(|part| {
                    (part.get("type").and_then(Value::as_str) == Some("text"))
                        .then(|| part.get("text").and_then(Value::as_str).map(str::to_owned))
                        .flatten()
                })
                .collect();
            let text = parts.join("\n");
            return (!text.is_empty()).then(|| truncate_text(&text));
        }
    };
    summarize_content_parts(content)
}

fn summarize_content_parts(content: &[pi_ai::types::TextOrImageContent]) -> Option<SummarizedText> {
    let text = content
        .iter()
        .filter_map(|part| match part {
            pi_ai::types::TextOrImageContent::Text(text) => Some(text.text.clone()),
            pi_ai::types::TextOrImageContent::Image(_) => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    (!text.is_empty()).then(|| truncate_text(&text))
}

fn truncate_text(text: &str) -> SummarizedText {
    if pi_js_compat::string::utf16_len(text) <= MESSAGE_CONTENT_LIMIT {
        return SummarizedText {
            truncated: false,
            value: text.to_owned(),
        };
    }
    SummarizedText {
        truncated: true,
        value: slice_utf16(text, MESSAGE_CONTENT_LIMIT),
    }
}

fn slice_utf16(text: &str, limit: usize) -> String {
    let mut units = 0;
    let mut result = String::new();
    for character in text.chars() {
        units += character.len_utf16();
        if units > limit {
            break;
        }
        result.push(character);
    }
    result
}

pub fn register_agent_viewer_tools(
    pi: &ExtensionApi,
    options: &MultiAgentExtensionOptions,
) -> Result<(), HandlerError> {
    let store = resolve_multi_agent_store(options);
    pi.register_tool(ToolDefinition {
        name: "agent_viewer".to_owned(),
        label: "Agent Viewer".to_owned(),
        description: "Inspect one agent by ID, with status, transcript, child IDs, and command descriptors.".to_owned(),
        prompt_snippet: None,
        prompt_guidelines: None,
        approval_required: Some(false),
        approval_kind: None,
        parameters: agent_viewer_schema(),
        constrained_sampling: None,
        render_shell: None,
        prepare_arguments: None,
        prepare_execution: None,
        execution_mode: None,
        execute: Arc::new(move |_tool_call_id, params, _signal, _on_update, ctx| {
            let store = Arc::clone(&store);
            Box::pin(async move {
                let params: AgentViewerParams =
                    serde_json::from_value(params).map_err(|error| HandlerError::new(error.to_string()))?;
                super::viewer::agent_viewer(&store, &params, Some(&ctx))
            })
        }),
        render_call: None,
        render_result: None,
    })
    .map_err(HandlerError::new)
}

fn agent_viewer_schema() -> Value {
    json!({
        "type": "object",
        "required": ["agentId"],
        "properties": {
            "agentId": {"type": "string"},
            "sessionId": {"type": "string"},
            "storeSessionId": {"type": "string"}
        }
    })
}

pub fn register_agents_mailbox_tools(
    pi: &ExtensionApi,
    options: &MultiAgentExtensionOptions,
) -> Result<(), HandlerError> {
    let store = resolve_multi_agent_store(options);
    super::mailbox_runtime::register_mailbox_tools(pi, store, options.on_session_message_sent.clone())
        .map_err(HandlerError::new)
}

pub fn multi_agent_extension(pi: &ExtensionApi, options: &MultiAgentExtensionOptions) -> Result<(), HandlerError> {
    let store = resolve_multi_agent_store(options);
    let mut shared_options = options.clone();
    shared_options.store = Some(store);
    super::registration::register_agents_core_tools(pi, &shared_options)?;
    register_agent_viewer_tools(pi, &shared_options)?;
    register_agents_mailbox_tools(pi, &shared_options)
}
