//! Mailbox tools and runtime lifecycle mirroring
//! (`extensions/agents-core/src/mailbox-runtime.ts`).

use pi_js_compat::js_format;
use std::sync::{Arc, Mutex};

use pi_agent_core::AgentToolResult;
use pi_agent_core::agent::types::AgentToolUpdateCallback;
use pi_js_compat::{
    js_json as json,
    js_json::{self, JsString, JsValue as Value},
};
use serde::Deserialize;
use serde_json::json as schema_json;

use crate::core::extensions::types::{ExtensionApi, ExtensionContext, HandlerError, ToolDefinition};
use crate::core::multi_agent_store::{
    AgentLifecycle, AgentMailboxMessage, AgentSnapshot, ContactParentInput, MailboxMessageKindValue,
    MailboxMessageStatusValue, MultiAgentStore, RecordOutboundSessionMessageInput, SendMailboxMessageInput,
};
use crate::core::session_control_db::{
    EnqueueRuntimeMailboxMessageInput, RuntimeMailboxAddress, RuntimeMailboxMessage, RuntimeMailboxMessageKind,
    RuntimeMailboxStoreRef, enqueue_runtime_mailbox_message, list_runtime_mailbox_messages, read_session_metadata,
};
use crate::core::session_manager::lock_session_manager;
use pi_js_compat::abort::AbortSignal;

use super::runtime_utils::create_failed_mailbox_message;
use super::shared::empty_agent;
use super::tool_results::tool_result;

const MAIN_THREAD_AGENT_ID: &str = "main";

pub type OnSessionMessageSent = Arc<dyn Fn(&AgentMailboxMessage, &JsString) -> Result<(), HandlerError> + Send + Sync>;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendAgentMessageParams {
    pub file_refs: Option<Value>,
    pub message: JsString,
    pub thread_id: Option<JsString>,
    pub to_agent_id: JsString,
    pub to_session_id: Option<JsString>,
    pub to_host: Option<JsString>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContactParentParams {
    pub file_refs: Option<Value>,
    pub message: JsString,
    pub thread_id: Option<JsString>,
}

pub fn contact_parent_schema() -> serde_json::Value {
    schema_json!({
        "type": "object",
        "required": [
            "body"
        ],
        "properties": {
            "body": {
                "type": "string",
                "minLength": 1,
                "pattern": "\\S",
                "description": "Required nonblank message body, sent verbatim. Independent of assistant text or reasoning."
            },
            "fileRefs": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": [
                        "path"
                    ],
                    "properties": {
                        "path": {
                            "type": "string"
                        },
                        "label": {
                            "type": "string"
                        }
                    }
                },
                "description": "Optional file references to attach."
            },
            "threadId": {
                "type": "string",
                "description": "Optional thread identifier for conversation correlation."
            }
        },
        "additionalProperties": false
    })
}

pub fn send_agent_message_schema() -> serde_json::Value {
    schema_json!({
        "type": "object",
        "required": [
            "body",
            "toAgentId"
        ],
        "properties": {
            "body": {
                "type": "string",
                "minLength": 1,
                "pattern": "\\S",
                "description": "Required nonblank message body, sent verbatim. Independent of assistant text or reasoning."
            },
            "fileRefs": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": [
                        "path"
                    ],
                    "properties": {
                        "path": {
                            "type": "string"
                        },
                        "label": {
                            "type": "string"
                        }
                    }
                },
                "description": "Optional file references to attach."
            },
            "threadId": {
                "type": "string",
                "description": "Optional thread identifier for conversation correlation."
            },
            "toAgentId": {
                "type": "string",
                "description": "Target agent ID, or 'main' when sending to another session's main thread."
            },
            "toHost": {
                "type": "string",
                "description": "Target hostname for a session on another host; requires toSessionId. Omit for local delivery."
            },
            "toSessionId": {
                "type": "string",
                "description": "Optional target session ID for direct cross-session mailbox delivery."
            }
        },
        "additionalProperties": false
    })
}

fn mailbox_kind(kind: MailboxMessageKindValue) -> RuntimeMailboxMessageKind {
    match kind {
        MailboxMessageKindValue::Message => RuntimeMailboxMessageKind::Message,
        MailboxMessageKindValue::Ask => RuntimeMailboxMessageKind::Ask,
        MailboxMessageKindValue::Reply => RuntimeMailboxMessageKind::Reply,
        MailboxMessageKindValue::Steer => RuntimeMailboxMessageKind::Steer,
        MailboxMessageKindValue::ParentRequest => RuntimeMailboxMessageKind::ParentRequest,
        MailboxMessageKindValue::System => RuntimeMailboxMessageKind::System,
    }
}

pub fn send_agent_message(
    store: &Arc<MultiAgentStore>,
    params: &SendAgentMessageParams,
    ctx: Option<&ExtensionContext>,
    on_session_message_sent: Option<&OnSessionMessageSent>,
) -> Result<AgentToolResult, HandlerError> {
    use super::session_message_route::{is_remote_session_message, session_message_route_error};
    let sender = current_message_sender_id(ctx)?;
    let db = ctx.map(ExtensionContext::control_db_path).transpose()?.flatten();
    if sender.is_some()
        && let Some(reason) = session_message_route_error(db.as_ref(), params)?
    {
        let mut result = tool_result(
            JsString::from("Could not send agent message to ")
                + format_sent_message_target(
                    &empty_direct_message(
                        &sender.clone().unwrap_or_else(|| "unknown_subagent".into()),
                        &params.to_agent_id,
                        &params.message,
                    ),
                    params.to_session_id.as_ref(),
                )
                + ": "
                + reason,
            json!({"agent":current_message_sender_agent(store, ctx)?,"message":empty_direct_message(&sender.clone().unwrap_or_else(|| "unknown_subagent".into()),&params.to_agent_id,&params.message)}),
        );
        result.is_error = Some(true);
        return Ok(result);
    }
    if is_remote_session_message(params)
        || (params.to_session_id.is_some() && is_main_runtime_target(&params.to_agent_id))
    {
        return send_main_runtime_session_message(store, params, ctx, on_session_message_sent);
    }
    if let Some(target_error) = validate_explicit_runtime_target(store, params, ctx)? {
        return Ok(target_error);
    }
    send_agent_message_from_current_sender(store, params, ctx, on_session_message_sent)
}

fn send_agent_message_from_current_sender(
    store: &Arc<MultiAgentStore>,
    params: &SendAgentMessageParams,
    ctx: Option<&ExtensionContext>,
    on_session_message_sent: Option<&OnSessionMessageSent>,
) -> Result<AgentToolResult, HandlerError> {
    let Some(sender_id) = current_message_sender_id(ctx)? else {
        return Ok(tool_result(
            "Could not send agent message: subagent runtime identity is unavailable.",
            json!({
                "agent": empty_agent("unknown_subagent"),
                "message": empty_direct_message(&"unknown_subagent".into(), &params.to_agent_id, &params.message),
            }),
        ));
    };
    let sender = store.get_agent(&sender_id);
    let message_input = SendMailboxMessageInput {
        file_refs: params.file_refs.clone(),
        body: params.message.clone(),
        thread_id: params.thread_id.clone(),
        to_agent_id: params.to_agent_id.clone(),
    };
    let sent = match &sender {
        Some(sender) => store
            .send_mailbox_message(&sender.id, sender.revision, &message_input)
            .map_err(|error| HandlerError::new(error.message))?,
        None => store
            .send_main_thread_mailbox_message(&message_input)
            .map_err(|error| HandlerError::new(error.message))?,
    };
    match sent {
        crate::core::multi_agent_store::MailboxMessageCommandResult::Ok { agent, message } => {
            deliver_sent_agent_message(store, &agent, &message, params, ctx, on_session_message_sent)
        }
        other => {
            let error = mailbox_error_name(&other);
            let current = mailbox_error_current(&other).unwrap_or_else(|| empty_agent(&sender_id));
            Ok(tool_result(
                js_format!("Could not send agent message from {}: {}", sender_id, error),
                json!({
                    "agent": current,
                    "message": empty_direct_message(&sender_id, &params.to_agent_id, &params.message),
                }),
            ))
        }
    }
}

fn mailbox_error_name(result: &crate::core::multi_agent_store::MailboxMessageCommandResult) -> &'static str {
    use crate::core::multi_agent_store::MailboxMessageCommandResult as Result;
    match result {
        Result::Ok { .. } => "",
        Result::NotFound { .. } => "not_found",
        Result::TargetNotFound { .. } => "target_not_found",
        Result::StaleRevision { .. } => "stale_revision",
        Result::ForbiddenTarget { .. } => "forbidden_target",
    }
}

fn mailbox_error_current(
    result: &crate::core::multi_agent_store::MailboxMessageCommandResult,
) -> Option<AgentSnapshot> {
    use crate::core::multi_agent_store::MailboxMessageCommandResult as Result;
    match result {
        Result::Ok { .. } | Result::NotFound { .. } => None,
        Result::TargetNotFound { current, .. }
        | Result::StaleRevision { current }
        | Result::ForbiddenTarget { current, .. } => Some((**current).clone()),
    }
}

fn validate_explicit_runtime_target(
    store: &MultiAgentStore,
    params: &SendAgentMessageParams,
    ctx: Option<&ExtensionContext>,
) -> Result<Option<AgentToolResult>, HandlerError> {
    let Some(to_session_id) = params.to_session_id.as_ref() else {
        return Ok(None);
    };
    if is_main_runtime_target(&params.to_agent_id) {
        return Ok(None);
    }
    let target_session_id = resolve_agent_runtime_session_id(store, &params.to_agent_id);
    let target_matches = match target_session_id {
        None => true,
        Some(None) => false,
        Some(Some(target_session_id)) => &target_session_id == to_session_id,
    };
    if target_matches {
        return Ok(None);
    }
    let sender_id = current_message_sender_id(ctx)?.unwrap_or_else(|| "unknown_subagent".into());
    Ok(Some(tool_result(
        JsString::from("Could not send agent message to ")
            + &params.to_agent_id
            + ": target session does not match "
            + to_session_id
            + ".",
        json!({
            "agent": current_message_sender_agent(store, ctx)?,
            "message": empty_direct_message(&sender_id, &params.to_agent_id, &params.message),
        }),
    )))
}

fn deliver_sent_agent_message(
    store: &Arc<MultiAgentStore>,
    agent: &AgentSnapshot,
    message: &AgentMailboxMessage,
    params: &SendAgentMessageParams,
    ctx: Option<&ExtensionContext>,
    on_session_message_sent: Option<&OnSessionMessageSent>,
) -> Result<AgentToolResult, HandlerError> {
    let delivered = match params.to_session_id.as_ref() {
        Some(to_session_id) => enqueue_runtime_session_message(
            store,
            message,
            to_session_id,
            ctx,
            (!is_main_runtime_target(&params.to_agent_id)).then(|| params.to_agent_id.clone()),
            params.to_host.clone(),
        )?,
        None => enqueue_runtime_mailbox_store_message(store, message, ctx)?,
    };
    if !delivered {
        let failed_message = mark_failed_mailbox_transport_message(store, message)?;
        let error = if params.to_session_id.is_some() {
            "Could not send runtime session message: runtime mailbox transport is unavailable."
        } else {
            "Could not send agent message: runtime mailbox transport is unavailable."
        };
        return Ok(tool_result(error, json!({"agent": agent, "message": failed_message})));
    }
    if let (Some(to_session_id), Some(callback)) = (params.to_session_id.as_ref(), on_session_message_sent) {
        callback(message, to_session_id)?;
    }
    Ok(tool_result(
        JsString::from("Sent message to ") + format_sent_message_target(message, params.to_session_id.as_ref()) + ".",
        json!({"agent": agent, "message": message}),
    ))
}

fn send_main_runtime_session_message(
    store: &Arc<MultiAgentStore>,
    params: &SendAgentMessageParams,
    ctx: Option<&ExtensionContext>,
    on_session_message_sent: Option<&OnSessionMessageSent>,
) -> Result<AgentToolResult, HandlerError> {
    let sender = current_message_sender_agent(store, ctx)?;
    let sender_id = current_message_sender_id(ctx)?;
    let Some(to_session_id) = params.to_session_id.clone() else {
        return Ok(tool_result(
            "Could not send runtime session message: target session unavailable.",
            json!({
                "agent": sender,
                "message": empty_direct_message(
                    &sender_id.clone().unwrap_or_else(|| "unknown_subagent".into()),
                    &params.to_agent_id,
                    &params.message,
                ),
            }),
        ));
    };
    let Some(sender_id) = sender_id else {
        return Ok(tool_result(
            "Could not send runtime session message: subagent runtime identity is unavailable.",
            json!({
                "agent": sender,
                "message": empty_direct_message(&"unknown_subagent".into(), &params.to_agent_id, &params.message),
            }),
        ));
    };

    let message = store
        .record_outbound_session_message(&RecordOutboundSessionMessageInput {
            file_refs: params.file_refs.clone(),
            body: params.message.clone(),
            from_agent_id: sender_id,
            thread_id: params.thread_id.clone(),
            to_agent_id: params.to_agent_id.clone(),
        })
        .map_err(|error| HandlerError::new(error.message))?;
    if !enqueue_runtime_session_message(
        store,
        &message,
        &to_session_id,
        ctx,
        (!is_main_runtime_target(&params.to_agent_id)).then(|| params.to_agent_id.clone()),
        params.to_host.clone(),
    )? {
        let failed_message = mark_failed_mailbox_transport_message(store, &message)?;
        return Ok(tool_result(
            "Could not send runtime session message: runtime mailbox transport is unavailable.",
            json!({"agent": sender, "message": failed_message}),
        ));
    }
    let remote = super::session_message_route::is_remote_session_message(params);
    if !remote && let Some(callback) = on_session_message_sent {
        callback(&message, &to_session_id)?;
    }
    Ok(tool_result(
        if remote {
            JsString::from("Queued message to session ")
                + &to_session_id
                + " for relay to host "
                + params.to_host.clone().unwrap_or_default()
                + "."
        } else {
            JsString::from("Sent message to session ") + &to_session_id + "."
        },
        json!({"agent": sender, "message": message}),
    ))
}

pub fn contact_parent(
    store: &Arc<MultiAgentStore>,
    params: &ContactParentParams,
    ctx: Option<&ExtensionContext>,
) -> Result<AgentToolResult, HandlerError> {
    let current_agent_id = match ctx {
        Some(ctx) => ctx.multi_agent_agent_id()?,
        None => None,
    };
    let Some(current_agent_id) = current_agent_id else {
        return Ok(tool_result(
            "Could not contact parent: agent runtime identity is unavailable.",
            json!({
                "agent": empty_agent(""),
                "message": empty_parent_request(&JsString::default(), &params.message),
            }),
        ));
    };

    let contacted = store
        .contact_parent(
            &current_agent_id,
            &ContactParentInput {
                file_refs: params.file_refs.clone(),
                body: params.message.clone(),
                thread_id: params.thread_id.clone(),
            },
        )
        .map_err(|error| HandlerError::new(error.message))?;
    match contacted {
        crate::core::multi_agent_store::ParentContactResult::Ok { agent, message } => {
            deliver_parent_contact(store, &agent, &message, ctx)
        }
        other => {
            let (error, current) = parent_contact_error(&other, &current_agent_id);
            Ok(tool_result(
                js_format!("Could not contact parent for {}: {}", current_agent_id, error),
                json!({
                    "agent": current,
                    "message": empty_parent_request(&current_agent_id, &params.message),
                }),
            ))
        }
    }
}

fn parent_contact_error(
    result: &crate::core::multi_agent_store::ParentContactResult,
    agent_id: &JsString,
) -> (&'static str, AgentSnapshot) {
    use crate::core::multi_agent_store::ParentContactResult as Result;
    match result {
        Result::Ok { .. } => ("", empty_agent(agent_id)),
        Result::NotFound { .. } => ("not_found", empty_agent(agent_id)),
        Result::ParentNotFound { current } => ("parent_not_found", (**current).clone()),
        Result::StaleRevision { current } => ("stale_revision", (**current).clone()),
    }
}

fn deliver_parent_contact(
    store: &Arc<MultiAgentStore>,
    agent: &AgentSnapshot,
    message: &AgentMailboxMessage,
    ctx: Option<&ExtensionContext>,
) -> Result<AgentToolResult, HandlerError> {
    if !enqueue_runtime_mailbox_store_message(store, message, ctx)? {
        let failed_message = mark_failed_mailbox_transport_message(store, message)?;
        return Ok(tool_result(
            "Could not contact parent: runtime mailbox transport is unavailable.",
            json!({"agent": agent, "message": failed_message}),
        ));
    }
    Ok(tool_result(
        JsString::from("Contacted parent for ") + &agent.display_name + ".",
        json!({"agent": agent, "message": message}),
    ))
}

struct MirrorState {
    bound_ctx: Option<ExtensionContext>,
    bound_session_id: Option<JsString>,
    unsubscribe: Option<crate::core::multi_agent_store::AgentSubscription>,
}

/// `createRuntimeLifecycleMirror`.
#[derive(Clone)]
pub struct RuntimeLifecycleMirror {
    store: Arc<MultiAgentStore>,
    state: Arc<Mutex<MirrorState>>,
}

impl RuntimeLifecycleMirror {
    pub fn new(store: Arc<MultiAgentStore>) -> Self {
        Self {
            store,
            state: Arc::new(Mutex::new(MirrorState {
                bound_ctx: None,
                bound_session_id: None,
                unsubscribe: None,
            })),
        }
    }

    pub fn bind(&self, ctx: &ExtensionContext) -> Result<(), HandlerError> {
        let session_id = lock_session_manager(&ctx.session_manager()?).get_session_id().clone();
        let mut state = self.state.lock().expect("mirror state");
        state.bound_ctx = Some(ctx.clone());
        if state.bound_session_id.as_ref() == Some(&session_id) && state.unsubscribe.is_some() {
            return Ok(());
        }
        state.unsubscribe = None;
        state.bound_session_id = Some(session_id);
        let store = Arc::clone(&self.store);
        let listener_store = Arc::clone(&store);
        let bound = Arc::clone(&self.state);
        let subscription = store.subscribe_lifecycle_notifications(move |message| {
            let ctx = bound.lock().expect("mirror state").bound_ctx.clone();
            let Some(ctx) = ctx else {
                return;
            };
            if let Err(error) = enqueue_lifecycle_runtime_mailbox_message(&listener_store, message, &ctx) {
                eprintln!(
                    "Failed to mirror lifecycle notification {}: {error}",
                    js_json::encode_node_utf8(&message.id)
                );
            }
        });
        state.unsubscribe = Some(subscription);
        drop(state);
        mirror_pending_lifecycle_runtime_mailbox_messages(&self.store, ctx)
    }

    pub fn dispose(&self) {
        let mut state = self.state.lock().expect("mirror state");
        state.unsubscribe = None;
        state.bound_ctx = None;
        state.bound_session_id = None;
    }
}

pub fn mirror_pending_lifecycle_runtime_mailbox_messages(
    store: &MultiAgentStore,
    ctx: &ExtensionContext,
) -> Result<(), HandlerError> {
    for agent in store.list_agents() {
        if !is_runtime_mirrored_lifecycle(agent.lifecycle) {
            continue;
        }
        let lifecycle = match agent.lifecycle {
            AgentLifecycle::Completed => crate::core::multi_agent_store::LifecycleNotificationLifecycle::Completed,
            AgentLifecycle::Failed => crate::core::multi_agent_store::LifecycleNotificationLifecycle::Failed,
            AgentLifecycle::WaitingForInput => {
                crate::core::multi_agent_store::LifecycleNotificationLifecycle::WaitingForInput
            }
            _ => continue,
        };
        for message in store
            .list_pending_lifecycle_notifications_for_agent(&agent.id, lifecycle)
            .map_err(|error| HandlerError::new(error.message))?
        {
            enqueue_lifecycle_runtime_mailbox_message(store, &message, ctx)?;
        }
    }
    Ok(())
}

fn mark_failed_mailbox_transport_message(
    store: &MultiAgentStore,
    message: &AgentMailboxMessage,
) -> Result<AgentMailboxMessage, HandlerError> {
    let error = "Runtime mailbox transport is unavailable.";
    Ok(
        match store
            .mark_mailbox_message_failed(&message.id, error)
            .map_err(|error| HandlerError::new(error.message))?
        {
            Some(failed) => failed,
            None => {
                let mut failed = message.clone();
                failed.error = Some(error.into());
                failed.status = MailboxMessageStatusValue::Failed;
                failed
            }
        },
    )
}

fn enqueue_runtime_session_message(
    store: &MultiAgentStore,
    message: &AgentMailboxMessage,
    to_session_id: &JsString,
    ctx: Option<&ExtensionContext>,
    recipient_agent_id: Option<JsString>,
    recipient_host: Option<JsString>,
) -> Result<bool, HandlerError> {
    let Some(ctx) = ctx else {
        return Ok(false);
    };
    let Some(control_db_path) = ctx.control_db_path()? else {
        return Ok(false);
    };
    let Some(store_ref) = read_runtime_mailbox_store_ref(store, message, Some(ctx))? else {
        eprintln!(
            "Runtime mailbox requires a store persisted to the control DB; dropped session message {}.",
            js_json::encode_node_utf8(&message.id)
        );
        return Ok(false);
    };
    let session_id = lock_session_manager(&ctx.session_manager()?)
        .get_session_id()
        .to_owned();
    let input = EnqueueRuntimeMailboxMessageInput {
        recipient_host,
        kind: mailbox_kind(message.kind),
        recipient: RuntimeMailboxAddress {
            agent_id: recipient_agent_id,
            session_id: to_session_id.to_owned().into(),
        },
        sender: RuntimeMailboxAddress {
            agent_id: (message.from_agent_id != MAIN_THREAD_AGENT_ID).then(|| message.from_agent_id.clone()),
            session_id: session_id.into(),
        },
        store_ref,
    };
    match enqueue_runtime_mailbox_message(&control_db_path, &input) {
        Ok(_) => Ok(true),
        Err(error) => {
            eprintln!(
                "Failed to enqueue runtime session message {}: {error}",
                js_json::encode_node_utf8(&message.id)
            );
            Ok(false)
        }
    }
}

fn read_runtime_mailbox_store_ref(
    store: &MultiAgentStore,
    message: &AgentMailboxMessage,
    ctx: Option<&ExtensionContext>,
) -> Result<Option<RuntimeMailboxStoreRef>, HandlerError> {
    let Some(persistence) = store.get_persistence_target() else {
        return Ok(None);
    };
    if message.id.is_empty() {
        return Ok(None);
    }
    let ctx_control_db_path = match ctx {
        Some(ctx) => ctx.control_db_path()?,
        None => None,
    };
    if Some(&persistence.control_db_path) != ctx_control_db_path.as_ref() {
        return Ok(None);
    }
    Ok(Some(RuntimeMailboxStoreRef {
        message_id: message.id.clone(),
        session_path: persistence.session_path.into(),
    }))
}

fn enqueue_lifecycle_runtime_mailbox_message(
    store: &MultiAgentStore,
    notification: &AgentMailboxMessage,
    ctx: &ExtensionContext,
) -> Result<(), HandlerError> {
    let Some(control_db_path) = ctx.control_db_path()? else {
        return Ok(());
    };
    let Some(store_ref) = read_runtime_mailbox_store_ref(store, notification, Some(ctx))? else {
        eprintln!(
            "Runtime mailbox requires a store persisted to the control DB; dropped lifecycle notification {}.",
            js_json::encode_node_utf8(&notification.id)
        );
        return Ok(());
    };
    let agent = store.get_agent(&notification.from_agent_id);
    let already_enqueued = match agent.as_ref() {
        Some(agent) => list_runtime_mailbox_messages(&control_db_path)
            .map_err(|error| HandlerError::new(error.message()))?
            .iter()
            .any(|message| {
                message.store_ref.session_path == store_ref.session_path
                    && is_detached_terminal_transport(message, agent)
            }),
        None => false,
    };
    if already_enqueued {
        return Ok(());
    }
    let Some(recipient) = resolve_runtime_recipient(store, notification, ctx)? else {
        return Ok(());
    };
    let current_session_id = lock_session_manager(&ctx.session_manager()?)
        .get_session_id()
        .to_owned();
    let sender_session_id = agent
        .as_ref()
        .and_then(|agent| agent.transcript.as_ref())
        .map(|transcript| transcript.session_id.clone().into())
        .unwrap_or(current_session_id);
    enqueue_runtime_mailbox_message(
        &control_db_path,
        &EnqueueRuntimeMailboxMessageInput {
            recipient_host: None,
            kind: mailbox_kind(notification.kind),
            recipient,
            sender: RuntimeMailboxAddress {
                agent_id: Some(notification.from_agent_id.clone()),
                session_id: sender_session_id.into(),
            },
            store_ref,
        },
    )
    .map_err(|error| HandlerError::new(error.message()))?;
    Ok(())
}

fn is_detached_terminal_transport(message: &RuntimeMailboxMessage, agent: &AgentSnapshot) -> bool {
    if !message.sender.agent_id.as_ref().is_some_and(|id| id == &agent.id)
        || message.kind != RuntimeMailboxMessageKind::System
    {
        return false;
    }
    let Ok(body) = js_json::parse_lossless(&message.body) else {
        return false;
    };
    body.get("type")
        .and_then(Value::as_string)
        .is_some_and(|kind| kind == "multi_agent_terminal")
        && body
            .get("agentId")
            .and_then(Value::as_string)
            .is_some_and(|id| id == &agent.id)
        && body.get("terminalRevision").and_then(Value::as_f64) == Some(agent.revision as f64)
}

fn is_runtime_mirrored_lifecycle(lifecycle: AgentLifecycle) -> bool {
    matches!(
        lifecycle,
        AgentLifecycle::Completed | AgentLifecycle::Failed | AgentLifecycle::WaitingForInput
    )
}

fn enqueue_runtime_mailbox_store_message(
    store: &MultiAgentStore,
    message: &AgentMailboxMessage,
    ctx: Option<&ExtensionContext>,
) -> Result<bool, HandlerError> {
    let Some(ctx) = ctx else {
        return Ok(false);
    };
    let Some(control_db_path) = ctx.control_db_path()? else {
        return Ok(false);
    };
    let Some(recipient) = resolve_runtime_recipient(store, message, ctx)? else {
        return Ok(false);
    };
    let Some(store_ref) = read_runtime_mailbox_store_ref(store, message, Some(ctx))? else {
        eprintln!(
            "Runtime mailbox requires a store persisted to the control DB; dropped mailbox message {}.",
            js_json::encode_node_utf8(&message.id)
        );
        return Ok(false);
    };
    let session_id = lock_session_manager(&ctx.session_manager()?)
        .get_session_id()
        .to_owned();
    let input = EnqueueRuntimeMailboxMessageInput {
        recipient_host: None,
        kind: mailbox_kind(message.kind),
        recipient,
        sender: RuntimeMailboxAddress {
            agent_id: Some(message.from_agent_id.clone()),
            session_id: session_id.into(),
        },
        store_ref,
    };
    match enqueue_runtime_mailbox_message(&control_db_path, &input) {
        Ok(_) => Ok(true),
        Err(error) => {
            eprintln!(
                "Failed to enqueue runtime mailbox message {}: {error}",
                js_json::encode_node_utf8(&message.id)
            );
            Ok(false)
        }
    }
}

fn resolve_runtime_recipient(
    store: &MultiAgentStore,
    message: &AgentMailboxMessage,
    ctx: &ExtensionContext,
) -> Result<Option<RuntimeMailboxAddress>, HandlerError> {
    if let Some(target) = store.get_agent(&message.to_agent_id)
        && let Some(transcript) = target.transcript.as_ref()
    {
        return Ok(Some(RuntimeMailboxAddress {
            agent_id: Some(target.id.into()),
            session_id: transcript.session_id.clone().into(),
        }));
    }
    if !is_main_runtime_target(&message.to_agent_id) {
        return Ok(Some(RuntimeMailboxAddress {
            agent_id: Some(message.to_agent_id.clone()),
            session_id: lock_session_manager(&ctx.session_manager()?).get_session_id().clone(),
        }));
    }
    let current_session_id = lock_session_manager(&ctx.session_manager()?).get_session_id().clone();
    let session_id = read_parent_runtime_session_id(ctx)?.unwrap_or(current_session_id);
    Ok(Some(RuntimeMailboxAddress {
        agent_id: None,
        session_id,
    }))
}

fn read_parent_runtime_session_id(ctx: &ExtensionContext) -> Result<Option<JsString>, HandlerError> {
    if let Some(parent_session_id) = ctx.multi_agent_parent_session_id()? {
        return Ok(Some(parent_session_id));
    }
    let parent_session = {
        let session_manager = ctx.session_manager()?;
        let manager = lock_session_manager(&session_manager);
        if !manager.is_subagent_session() {
            return Ok(None);
        }
        let Some(parent_session) = manager.get_header().and_then(|header| header.parent_session().cloned()) else {
            return Ok(None);
        };
        parent_session
    };
    let Some(control_db_path) = ctx.control_db_path()? else {
        return Ok(Some(parent_session));
    };
    Ok(Some(
        read_session_metadata(&control_db_path, &parent_session)
            .map_err(|error| HandlerError::new(error.message()))?
            .map(|metadata| metadata.id)
            .unwrap_or(parent_session),
    ))
}

pub fn current_message_sender_id(ctx: Option<&ExtensionContext>) -> Result<Option<JsString>, HandlerError> {
    let Some(ctx) = ctx else {
        return Ok(Some(MAIN_THREAD_AGENT_ID.into()));
    };
    if let Some(agent_id) = ctx.multi_agent_agent_id()? {
        return Ok(Some(agent_id));
    }
    if ctx.multi_agent_requires_agent_id()? {
        return Ok(None);
    }
    Ok(Some(MAIN_THREAD_AGENT_ID.into()))
}

fn current_message_sender_agent(
    store: &MultiAgentStore,
    ctx: Option<&ExtensionContext>,
) -> Result<AgentSnapshot, HandlerError> {
    let sender_id = current_message_sender_id(ctx)?.unwrap_or_else(|| "unknown_subagent".into());
    Ok(store.get_agent(&sender_id).unwrap_or_else(|| empty_agent(&sender_id)))
}

fn resolve_agent_runtime_session_id(store: &MultiAgentStore, to_agent_id: &JsString) -> Option<Option<JsString>> {
    store.get_agent(to_agent_id).map(|target| {
        target
            .transcript
            .as_ref()
            .map(|transcript| transcript.session_id.clone().into())
    })
}

fn is_main_runtime_target(to_agent_id: &JsString) -> bool {
    to_agent_id == MAIN_THREAD_AGENT_ID || to_agent_id == "supervisor"
}

fn format_sent_message_target(message: &AgentMailboxMessage, to_session_id: Option<&JsString>) -> JsString {
    match to_session_id {
        Some(to_session_id) => message.to_agent_id.clone() + " in session " + to_session_id,
        None => message.to_agent_id.clone(),
    }
}

fn empty_parent_request(agent_id: &JsString, body: &JsString) -> AgentMailboxMessage {
    create_failed_mailbox_message(body, agent_id, MailboxMessageKindValue::ParentRequest, "")
}

fn empty_direct_message(from_agent_id: &JsString, to_agent_id: &JsString, body: &JsString) -> AgentMailboxMessage {
    create_failed_mailbox_message(body, from_agent_id, MailboxMessageKindValue::Message, to_agent_id)
}

fn model_mailbox_params(mut params: Value) -> Result<Value, HandlerError> {
    let object = params
        .as_object_mut()
        .ok_or_else(|| HandlerError::new("Expected mailbox arguments"))?;
    let body = object
        .shift_remove(&JsString::from("body"))
        .ok_or_else(|| HandlerError::new("Missing required body"))?;
    object.insert("message".into(), body);
    Ok(params)
}

pub fn register_mailbox_tools(
    pi: &ExtensionApi,
    store: Arc<MultiAgentStore>,
    on_session_message_sent: Option<OnSessionMessageSent>,
) -> Result<(), crate::core::extensions::types::HandlerError> {
    let send_store = Arc::clone(&store);
    let send_callback = on_session_message_sent.clone();
    pi.register_tool(ToolDefinition {
        name: "send_agent_message".to_owned(),
        label: "Send Agent Message".into(),
        description: "Send the required body verbatim across a direct parent-child edge, or to another session via toSessionId (with toAgentId 'main'). For another host, also pass toHost; the message queues locally until relay delivery. Unknown local sessions are rejected. Supply the complete message in body; assistant narration and reasoning are never read.".into(),
        prompt_snippet: None,
        prompt_guidelines: Some(vec!["For send_agent_message and contact_parent, put the complete outgoing message in the required body argument. It is sent verbatim; assistant narration and reasoning are never read. No assistant text is required, and multiple calls in one turn can send distinct bodies.".into(), "For a cross-host reply, use the sender's hostname as toHost plus its session ID as toSessionId. Never infer host ownership from a session ID.".into()]), 
        approval_required: Some(false),
        approval_kind: None,
        parameters: send_agent_message_schema(),
        render_shell: None,
        prepare_arguments: None,
        prepare_execution: None,
        execution_mode: None,
        execute: Arc::new(
            move |_tool_call_id: String,
                  params: Value,
                  _signal: Option<AbortSignal>,
                  _on_update: Option<AgentToolUpdateCallback>,
                  ctx: Option<ExtensionContext>| {
                let store = Arc::clone(&send_store);
                let callback = send_callback.clone();
                Box::pin(async move {
                    let params: SendAgentMessageParams =
                        js_json::from_value(&model_mailbox_params(params)?).map_err(|error| HandlerError::new(error.to_string()))?;
                    send_agent_message(&store, &params, ctx.as_ref(), callback.as_ref())
                })
            },
        ),
        render_call: None,
        render_result: None,
    })?;

    pi.register_tool(ToolDefinition {
        name: "contact_parent".to_owned(),
        label: "Contact Parent".into(),
        description: "Send the required body verbatim as a child-agent mailbox request to its direct parent. Supply the complete message in body; assistant narration and reasoning are never read.".into(),
        prompt_snippet: None,
        prompt_guidelines: Some(vec!["For send_agent_message and contact_parent, put the complete outgoing message in the required body argument. It is sent verbatim; assistant narration and reasoning are never read. No assistant text is required, and multiple calls in one turn can send distinct bodies.".into()]), 
        approval_required: Some(false),
        approval_kind: None,
        parameters: contact_parent_schema(),
        render_shell: None,
        prepare_arguments: None,
        prepare_execution: None,
        execution_mode: None,
        execute: Arc::new(
            move |_tool_call_id: String,
                  params: Value,
                  _signal: Option<AbortSignal>,
                  _on_update: Option<AgentToolUpdateCallback>,
                  ctx: Option<ExtensionContext>| {
                let store = Arc::clone(&store);
                Box::pin(async move {
                    let params: ContactParentParams =
                        js_json::from_value(&model_mailbox_params(params)?).map_err(|error| HandlerError::new(error.to_string()))?;
                    contact_parent(&store, &params, ctx.as_ref())
                })
            },
        ),
        render_call: None,
        render_result: None,
    })?;
    Ok(())
}
