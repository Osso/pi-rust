//! Multi-agent extension options and runtime handles
//! (`extensions/agents-core/src/runtime.ts`).

use pi_js_compat::js_format;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use futures_util::future::Shared;
use pi_agent_core::AgentToolResult;
use pi_ai::request::BoxFuture;
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::{
    js_json as json,
    js_json::{self, JsObject as Map, JsString, JsValue as Value},
};

use crate::core::desktop_notification::{DesktopNotifier, send_desktop_notification};
use crate::core::extensions::types::{
    ExtensionApi, ExtensionCommandContext, ExtensionContext, HandlerError, NotifyType, ToolDefinition,
};
use crate::core::multi_agent_store::{AgentSnapshot, MultiAgentStore, MultiAgentStoreOptions, is_active_lifecycle};
use crate::core::session_manager::{KnownSessionEntry, SessionEntry};

use super::control_runtime::OwnedAgentRuntime;
use super::dispatch_runtime::WaitingDesktopNotificationHandles;
use super::mailbox_runtime::{OnSessionMessageSent, RuntimeLifecycleMirror};
use super::shared::{
    AttachedSessionFactory, CHILD_ORCHESTRATION_UNAVAILABLE_MESSAGE, ChildAgentSession, ChildAgentSessionFactory,
    WaitWakeListener, WaitWakeListeners,
};
use super::viewer::AgentViewerParams;

pub const MAIN_THREAD_AGENT_ID: &str = "main";
const MESSAGE_CONTENT_LIMIT: usize = 2000;

struct StoreArtifactRoot {
    store: std::sync::Weak<MultiAgentStore>,
    root: JsString,
}

static ARTIFACT_ROOTS_BY_STORE: Mutex<Vec<StoreArtifactRoot>> = Mutex::new(Vec::new());

pub(super) fn remember_store_artifact_root(store: Option<&Arc<MultiAgentStore>>, root: Option<&JsString>) {
    let (Some(store), Some(root)) = (store, root.filter(|root| !root.is_empty())) else {
        return;
    };
    let mut roots = ARTIFACT_ROOTS_BY_STORE.lock().expect("artifact roots");
    roots.retain(|entry| entry.store.strong_count() > 0);
    let identity = Arc::downgrade(store);
    if let Some(entry) = roots.iter_mut().find(|entry| entry.store.ptr_eq(&identity)) {
        entry.root = root.clone();
    } else {
        roots.push(StoreArtifactRoot {
            store: identity,
            root: root.clone(),
        });
    }
}

pub(super) fn artifact_root_for_store(store: &Arc<MultiAgentStore>) -> Option<JsString> {
    let identity = Arc::downgrade(store);
    ARTIFACT_ROOTS_BY_STORE
        .lock()
        .expect("artifact roots")
        .iter()
        .find(|entry| entry.store.ptr_eq(&identity))
        .map(|entry| entry.root.clone())
}

static DEFAULT_BROWSER_AGENT_CONFIG_ROOT: std::sync::OnceLock<
    Result<JsString, pi_js_compat::node_error::NativeNodeError>,
> = std::sync::OnceLock::new();

/// Frozen module-evaluation state: snapshot HOME once, before CLI dispatch.
/// Browser child read-tool wiring is tracked separately in agents-core traceability.
pub fn default_browser_agent_config_root() -> Result<&'static JsString, pi_js_compat::node_error::NativeNodeError> {
    DEFAULT_BROWSER_AGENT_CONFIG_ROOT
        .get_or_init(|| {
            pi_js_compat::node_os::homedir().map(|home| crate::utils::paths::node_join(&home, "AgentConfig"))
        })
        .as_ref()
        .map_err(Clone::clone)
}

pub(crate) struct NoopDesktopNotifier;

impl DesktopNotifier for NoopDesktopNotifier {
    fn notify(
        &self,
        notification: &crate::core::desktop_notification::DesktopNotification,
    ) -> Result<
        Option<Box<dyn crate::core::desktop_notification::DesktopNotificationHandle>>,
        pi_js_compat::callback_error::CallbackError,
    > {
        Ok(send_desktop_notification(notification))
    }
}

pub type SharedDispatch = Shared<BoxFuture<'static, Result<AgentSnapshot, pi_js_compat::js_json::JsString>>>;

#[derive(Clone, Default)]
pub struct MultiAgentRuntimeHandles {
    pub cancellation_settlement_timeout_ms: Option<u64>,
    pub latest_model_request_started_at: Arc<Mutex<Option<i64>>>,
    pub dispatches: Arc<Mutex<HashMap<JsString, SharedDispatch>>>,
    pub ownerships: Arc<Mutex<HashMap<JsString, Arc<OwnedAgentRuntime>>>>,
    pub sessions: Arc<Mutex<HashMap<JsString, Arc<dyn ChildAgentSession>>>>,
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

pub type SelectAgentView = Arc<dyn Fn(&JsString) -> BoxFuture<'static, Option<bool>> + Send + Sync>;

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

pub fn wake_wait_agents_after_coordination(runtime_handles: &MultiAgentRuntimeHandles, prompt: &JsString) {
    runtime_handles
        .wait_wake_listeners
        .emit(&super::shared::WaitAgentsWakeUp::coordination(prompt.to_owned()));
}

/// `resolveSelectedSessionMutationTarget`.
pub fn resolve_selected_session_mutation_target(
    store: &MultiAgentStore,
    runtime_handles: &MultiAgentRuntimeHandles,
) -> Result<Option<Arc<dyn crate::core::extensions::types::ViewedSessionMutationTarget>>, HandlerError> {
    use crate::core::multi_agent_store::format_inactive_agent_selection_message;

    let Some(selected_agent_id) = store.get_selected_agent_id() else {
        return Ok(None);
    };
    if selected_agent_id == MAIN_THREAD_AGENT_ID {
        return Ok(None);
    }
    let Some(agent) = store.get_agent(&selected_agent_id) else {
        return Err(HandlerError::new(js_format!("Agent not found: {}", selected_agent_id)));
    };
    if agent.agent_type == "background" {
        return Err(HandlerError::new(js_format!(
            "Agent {} is detached and not a live child session",
            selected_agent_id
        )));
    }
    if !is_active_lifecycle(agent.lifecycle) {
        return Err(HandlerError::new(format_inactive_agent_selection_message(&agent)));
    }
    let sessions = runtime_handles.sessions.lock().expect("sessions");
    let Some(session) = sessions.get(&selected_agent_id) else {
        return Err(HandlerError::new(js_format!(
            "Agent {} is not a live child session",
            selected_agent_id
        )));
    };
    let Some(target) = session.session_mutation_target() else {
        return Err(HandlerError::new(js_format!(
            "Agent {} does not support live session mutation",
            selected_agent_id
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

pub fn format_agent_status(agent: &AgentSnapshot) -> JsString {
    let status = JsString::from(&agent.display_name) + format!(" is {}", lifecycle_name(agent.lifecycle));
    let summary = agent
        .result
        .as_ref()
        .and_then(|result| result.summary.as_ref())
        .map(JsString::trim)
        .unwrap_or_default();
    if !summary.is_empty() {
        return status + ": " + summary;
    }
    let error = agent
        .error
        .as_ref()
        .map(|error| error.message.trim())
        .unwrap_or_default();
    if !error.is_empty() {
        return status + ": " + error;
    }
    status + "."
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
    signal: Option<AbortSignal>,
    ctx: Option<&ExtensionContext>,
) -> Result<AgentToolResult, HandlerError> {
    let wake = super::wait_runtime::wait_notifications_runtime(store, runtime_handles, signal, ctx).await?;
    super::wait_runtime::consume_notifications_runtime(store, &wake, ctx, &format_agent_status)
}

pub async fn wait_notifications(
    store: &Arc<MultiAgentStore>,
    runtime_handles: &MultiAgentRuntimeHandles,
    signal: Option<AbortSignal>,
    ctx: Option<&ExtensionContext>,
) -> Result<super::wait_runtime::WaitNotificationsWake, HandlerError> {
    super::wait_runtime::wait_notifications_runtime(store, runtime_handles, signal, ctx).await
}

pub fn consume_notifications(
    store: &Arc<MultiAgentStore>,
    wake: &super::wait_runtime::WaitNotificationsWake,
    ctx: Option<&ExtensionContext>,
) -> Result<AgentToolResult, HandlerError> {
    super::wait_runtime::consume_notifications_runtime(store, wake, ctx, &format_agent_status)
}

pub fn deliver_terminal_outbox_for_store(
    store: &Arc<MultiAgentStore>,
) -> Result<f64, pi_js_compat::callback_error::CallbackError> {
    let Some(persistence) = store.get_persistence_target() else {
        return Ok(0.0);
    };
    crate::core::terminal_outbox_delivery::deliver_terminal_outbox_projections(
        &crate::core::terminal_outbox_delivery::DeliverTerminalOutboxOptions {
            artifact_root: artifact_root_for_store(store),
            claim_id: pi_ai::utils::random_uuid::random_uuid()?.into(),
            control_db_path: persistence.control_db_path,
            now: Arc::new(|| Ok(crate::utils::iso_time::now_iso_string().into())),
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
        ui.notify(&"No background jobs.".into(), Some(NotifyType::Info))?;
        return Ok(());
    }
    ui.notify(
        &JsString::join(
            agents
                .iter()
                .map(|agent| agent.id.clone() + " " + format_agent_status(agent))
                .collect::<Vec<_>>(),
            "\n",
        ),
        Some(NotifyType::Info),
    )?;
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
    pub fn handles_method(&self, method: &JsString) -> bool {
        [
            "agents.spawn",
            "agents.wait",
            "agents.attachSession",
            "agents.list",
            "agents.current",
            "agents.select",
            "messages.last",
            "messages.send",
        ]
        .iter()
        .any(|owned| *method == **owned)
    }

    pub async fn handle(
        &self,
        request: &Value,
        ctx: &ExtensionContext,
        signal: Option<AbortSignal>,
        active_tool_call_id: Option<String>,
    ) -> Result<Value, HandlerError> {
        if self.disposed.load(Ordering::SeqCst) {
            return Err(HandlerError::new("Multi-agent Pi request handler is disposed"));
        }
        let method = request
            .get("method")
            .and_then(Value::as_string)
            .cloned()
            .unwrap_or_default();
        let is_child_runtime = super::shared::is_child_agent_runtime(ctx)?;
        if !is_child_runtime {
            self.mirror.bind(ctx)?;
        }
        if is_child_runtime && is_supervisor_only_agent_request(&method) {
            return Err(HandlerError::new(CHILD_ORCHESTRATION_UNAVAILABLE_MESSAGE));
        }
        let params = request.get("params").cloned().unwrap_or(Value::Null);
        self.dispatch(&method, params, ctx, signal, active_tool_call_id).await
    }

    async fn dispatch(
        &self,
        method: &JsString,
        params: Value,
        ctx: &ExtensionContext,
        signal: Option<AbortSignal>,
        active_tool_call_id: Option<String>,
    ) -> Result<Value, HandlerError> {
        // A method name with unpaired surrogates matches no route, as in JavaScript.
        match method.to_utf8().as_deref() {
            Ok("agents.spawn") => {
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
                        store: Arc::clone(&self.runtime.store),
                        waiting_desktop_notifications: Arc::clone(&self.runtime.waiting_desktop_notifications),
                        _marker: std::marker::PhantomData,
                    })
                    .await?;
                Ok(result.details.unwrap_or(Value::Null))
            }
            Ok("agents.wait") => {
                assert_no_wait_agents_params(&params, "pi.agents.wait")?;
                let waited = wait_agents(&self.runtime.store, &self.runtime.runtime_handles, signal, Some(ctx)).await?;
                if let Some(delivery) = waited
                    .details
                    .as_ref()
                    .and_then(|details| details.get("runtimeMailboxDelivery"))
                {
                    let delivery: crate::core::runtime_mailbox_acceptance::RuntimeMailboxToolDelivery =
                        js_json::from_value(delivery).map_err(|error| HandlerError::new(error.to_string()))?;
                    for message in delivery.messages {
                        crate::core::session_control_db::release_runtime_mailbox_message_claim(
                            &delivery.control_db_path,
                            message.id,
                        )
                        .map_err(|error| HandlerError::new(error.message()))?;
                    }
                }
                Ok(Value::Null)
            }
            Ok("agents.attachSession") => {
                let params: super::spawn_attach_runtime::AttachSessionAgentParams =
                    js_json::from_value(&params).map_err(|error| HandlerError::new(error.to_string()))?;
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
            Ok("agents.list") => {
                let params: super::viewer::ListAgentsParams = if params.is_null() {
                    Default::default()
                } else {
                    js_json::from_value(&params).map_err(|error| HandlerError::new(error.to_string()))?
                };
                let result = super::viewer::list_agents(&self.runtime.store, &params)?;
                Ok(result.details.unwrap_or(Value::Null))
            }
            Ok("agents.current") => select_current_agent(&self.runtime.store),
            Ok("agents.select") => {
                select_agent(
                    &self.runtime.store,
                    &params,
                    self.runtime.options.select_agent_view.as_ref(),
                )
                .await
            }
            Ok("messages.last") => {
                let session_manager = ctx.session_manager()?;
                let manager = session_manager;
                Ok(find_last_session_message(&manager.get_branch(None)))
            }
            Ok("messages.send") => {
                let params: super::mailbox_runtime::SendAgentMessageParams =
                    js_json::from_value(&params).map_err(|error| HandlerError::new(error.to_string()))?;
                let result = super::mailbox_runtime::send_agent_message(&self.runtime.store, &params, Some(ctx), None)?;
                Ok(result.details.unwrap_or(Value::Null))
            }
            _ => Ok(Value::Null),
        }
    }
}

pub fn create_multi_agent_pi_request_handler(options: &MultiAgentExtensionOptions) -> MultiAgentPiRequestHandler {
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
            runtime_handles,
            store,
            waiting_desktop_notifications: Arc::new(Mutex::new(HashMap::new())),
        }),
        mirror,
        disposed: AtomicBool::new(false),
    }
}

fn is_supervisor_only_agent_request(method: &JsString) -> bool {
    *method == "agents.spawn" || *method == "agents.attachSession" || *method == "agents.wait"
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

fn select_agent<'a>(
    store: &'a MultiAgentStore,
    params: &'a Value,
    select_agent_view: Option<&'a SelectAgentView>,
) -> BoxFuture<'a, Result<Value, HandlerError>> {
    Box::pin(async move {
        let agent_id = normalize_select_agent_id(params)?;
        let rendered = match select_agent_view {
            Some(callback) => callback(&agent_id).await,
            None => None,
        };
        if rendered == Some(true) {
            if agent_id == MAIN_THREAD_AGENT_ID {
                return Ok(json!({"agent": create_main_thread_snapshot()}));
            }
            if let Some(selected) = store.get_agent(&agent_id) {
                return Ok(json!({"agent": selected}));
            }
            return select_current_agent(store);
        }
        if rendered == Some(false) {
            return Err(HandlerError::new(js_format!(
                "Agent view selection failed: {}",
                agent_id
            )));
        }
        if agent_id == MAIN_THREAD_AGENT_ID {
            store.clear_selected_agent_view();
            return Ok(json!({"agent": create_main_thread_snapshot()}));
        }
        match store.select_agent_view_with_status(&agent_id) {
            crate::core::multi_agent_store::AgentViewSelectionResult::Ok(agent) => Ok(json!({"agent": agent})),
            crate::core::multi_agent_store::AgentViewSelectionResult::NotFound { .. } => {
                Err(HandlerError::new(js_format!("Agent not found: {}", agent_id)))
            }
        }
    })
}

fn normalize_select_agent_id(params: &Value) -> Result<JsString, HandlerError> {
    if let Some(agent_id) = params.as_string() {
        return Ok(agent_id.clone());
    }
    let Some(object) = params.as_object() else {
        return Err(HandlerError::new(
            "pi.agents.select requires an agent id string or { agentId } object",
        ));
    };
    let Some(agent_id) = object.get("agentId").and_then(Value::as_string) else {
        return Err(HandlerError::new("pi.agents.select requires a non-empty agentId"));
    };
    if agent_id.trim().is_empty() {
        return Err(HandlerError::new("pi.agents.select requires a non-empty agentId"));
    }
    Ok(agent_id.clone())
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
        if let Some(KnownSessionEntry::Message(message)) = entry.known().as_deref() {
            return summarize_session_message(message);
        }
    }
    Value::Null
}

fn summarize_session_message(entry: &crate::core::session_manager::SessionMessageEntry) -> Value {
    let message = &entry.message;
    let role = message.role();
    let mut details = Map::new();
    details.insert("entryId".into(), json!(entry.id));
    details.insert("role".into(), json!(role));
    if let Some(text) = summarize_message_content(message) {
        details.insert("content".into(), json!(text.value));
        details.insert("text".into(), json!(text.value));
        if text.truncated {
            details.insert("truncated".into(), json!(true));
        }
    }
    Value::Object(details)
}

struct SummarizedText {
    truncated: bool,
    value: JsString,
}

fn summarize_message_content(message: &pi_agent_core::AgentMessage) -> Option<SummarizedText> {
    let content = match message {
        pi_agent_core::AgentMessage::Assistant(assistant) => {
            let text = JsString::join(
                assistant
                    .content
                    .iter()
                    .filter_map(|part| match part {
                        pi_ai::types::AssistantContent::Text(text) => Some(text.text.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>(),
                "\n",
            );
            return (!text.is_empty()).then(|| truncate_text(&text));
        }
        pi_agent_core::AgentMessage::User(user) => match &user.content {
            pi_ai::types::UserContent::Text(text) => return Some(truncate_text(text)),
            pi_ai::types::UserContent::Blocks(blocks) => blocks,
        },
        pi_agent_core::AgentMessage::ToolResult(result) => &result.content,
        pi_agent_core::AgentMessage::Custom(object) => {
            let content = object.get("content")?;
            if let Some(text) = content.as_string() {
                return Some(truncate_text(text));
            }
            let parts: Vec<JsString> = content
                .as_array()?
                .iter()
                .filter_map(|part| {
                    (part
                        .get("type")
                        .and_then(Value::as_string)
                        .is_some_and(|kind| kind == "text"))
                    .then(|| part.get("text").and_then(Value::as_string).cloned())
                    .flatten()
                })
                .collect();
            let text = JsString::join(parts, "\n");
            return (!text.is_empty()).then(|| truncate_text(&text));
        }
    };
    summarize_content_parts(content)
}

fn summarize_content_parts(content: &[pi_ai::types::TextOrImageContent]) -> Option<SummarizedText> {
    let text = JsString::join(
        content
            .iter()
            .filter_map(|part| match part {
                pi_ai::types::TextOrImageContent::Text(text) => Some(text.text.clone()),
                pi_ai::types::TextOrImageContent::Image(_) => None,
            })
            .collect::<Vec<_>>(),
        "\n",
    );
    (!text.is_empty()).then(|| truncate_text(&text))
}

fn truncate_text(text: &JsString) -> SummarizedText {
    if text.len() <= MESSAGE_CONTENT_LIMIT {
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

fn slice_utf16(text: &JsString, limit: usize) -> JsString {
    text.slice(0, limit)
}

pub fn register_agent_viewer_tools(
    pi: &ExtensionApi,
    options: &MultiAgentExtensionOptions,
) -> Result<(), HandlerError> {
    let store = resolve_multi_agent_store(options);
    pi.register_tool(ToolDefinition {
        name: "agent_viewer".to_owned(),
        label: "Agent Viewer".into(),
        description: "Inspect one agent by ID, with status, transcript, child IDs, command descriptors, and optional historical lifecycle and descendant trace.".into(),
        prompt_snippet: None,
        prompt_guidelines: None,
        approval_required: Some(false),
        approval_kind: None,
        parameters: agent_viewer_schema(),
        render_shell: None,
        prepare_arguments: None,
        prepare_execution: None,
        execution_mode: None,
        execute: Arc::new(move |_tool_call_id, params, _signal, _on_update, ctx| {
            let store = Arc::clone(&store);
            Box::pin(async move {
                let params: AgentViewerParams =
                    js_json::from_value(&params).map_err(|error| HandlerError::new(error.to_string()))?;
                super::viewer::agent_viewer(&store, &params, ctx.as_ref())
            })
        }),
        render_call: None,
        render_result: None,
    })
}

fn agent_viewer_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "required": ["agentId"],
        "properties": {
            "agentId": {"type": "string"},
            "sessionId": {"type": "string"},
            "storeSessionId": {"type": "string"},
            "trace": {"type": "boolean"}
        }
    })
}

pub fn register_agents_mailbox_tools(
    pi: &ExtensionApi,
    options: &MultiAgentExtensionOptions,
) -> Result<(), HandlerError> {
    let store = resolve_multi_agent_store(options);
    super::mailbox_runtime::register_mailbox_tools(pi, store.clone(), options.on_session_message_sent.clone())?;
    super::question_relay_tools::register_question_relay_tools(pi, store)
}

pub fn multi_agent_extension(pi: &ExtensionApi, options: &MultiAgentExtensionOptions) -> Result<(), HandlerError> {
    let store = resolve_multi_agent_store(options);
    let mut shared_options = options.clone();
    shared_options.store = Some(store);
    super::registration::register_agents_core_tools(pi, &shared_options)?;
    register_agent_viewer_tools(pi, &shared_options)?;
    register_agents_mailbox_tools(pi, &shared_options)
}
