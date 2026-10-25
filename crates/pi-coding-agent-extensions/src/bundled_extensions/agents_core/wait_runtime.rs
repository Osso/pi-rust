//! Wait tool runtime (`extensions/agents-core/src/wait-runtime.ts`).

use indexmap::IndexSet;
use pi_js_compat::js_format;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pi_agent_core::AgentToolResult;
use pi_js_compat::{js_json as json, js_json::JsString};

use crate::core::extensions::types::{ExtensionContext, HandlerError};
use crate::core::lifecycle_coordinator::LifecycleCoordinator;
use crate::core::multi_agent_store::{AgentMailboxMessage, AgentSnapshot, MultiAgentStore, is_active_lifecycle};
use crate::core::runtime_coordination_format::{format_runtime_mailbox_prompt, format_shared_channel_prompt};
use crate::core::session_control_db::{
    RuntimeMailboxAddress, SharedChannelMessage, advance_shared_channel_cursor,
    has_pending_runtime_coordination_message, is_runtime_coordination_mailbox_message,
    list_shared_channel_messages_after, read_multi_agent_agents, read_multi_agent_state, read_shared_channel_cursor,
    read_shared_channel_tail, release_runtime_mailbox_message_claim, resolve_own_main_runtime_coordination_recipient,
    take_runtime_mailbox_messages_for_delivery,
};
use crate::core::terminal_outbox_delivery::{DeliverTerminalOutboxOptions, deliver_terminal_outbox_projections};

use super::mailbox_runtime::mirror_pending_lifecycle_runtime_mailbox_messages;
use super::shared::{
    CHILD_ORCHESTRATION_UNAVAILABLE_MESSAGE, WaitAgentsWakeUp, WaitWakeListeners, is_child_agent_runtime,
};
use super::tool_results::{empty_tool_result, tool_result};

const RUNTIME_COORDINATION_POLL_INTERVAL_MS: u64 = 3_000;
const SHARED_CHANNEL_PAGE_LIMIT: i64 = 20;

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock")
        .as_millis() as i64
}

#[derive(Debug, Clone)]
pub enum WaitNotificationsWake {
    Agent { agent: Box<AgentSnapshot> },
    Cancelled,
    Coordination { prompt: Option<JsString> },
    Error { message: JsString },
    None,
    Timeout,
    Unavailable { message: JsString },
    WakeUp { wake_up: WaitAgentsWakeUp },
}

#[derive(Debug, Clone)]
struct RuntimeCoordinationRecipient {
    address: RuntimeMailboxAddress,
    control_db_path: JsString,
}

struct WaitAgentsWakeWatcherOptions {
    deadline: Option<i64>,
    active_agents: Vec<AgentSnapshot>,
    control_db_path: JsString,
    recipient: Option<RuntimeCoordinationRecipient>,
    session_path: JsString,
    signal: Option<pi_js_compat::abort::AbortSignal>,
    store: Arc<MultiAgentStore>,
    wait_wake_listeners: WaitWakeListeners,
}

struct WaitAgentsWakeWatcher {
    deadline: Option<i64>,
    control_db_path: JsString,
    recipient: Option<RuntimeCoordinationRecipient>,
    session_path: JsString,
    signal: Option<pi_js_compat::abort::AbortSignal>,
    store: Arc<MultiAgentStore>,
    tracked_agent_ids: IndexSet<JsString>,
    wait_wake_listeners: WaitWakeListeners,
}

impl WaitAgentsWakeWatcher {
    fn new(options: WaitAgentsWakeWatcherOptions) -> Self {
        Self {
            deadline: options.deadline,
            control_db_path: options.control_db_path,
            recipient: options.recipient,
            session_path: options.session_path,
            signal: options.signal,
            store: options.store,
            tracked_agent_ids: options.active_agents.into_iter().map(|agent| agent.id).collect(),
            wait_wake_listeners: options.wait_wake_listeners,
        }
    }

    async fn wait(self) -> WaitNotificationsWake {
        if self.signal.as_ref().is_some_and(|signal| signal.aborted()) {
            return WaitNotificationsWake::Cancelled;
        }
        if self.tracked_agent_ids.is_empty() {
            return match self.check_coordination() {
                Some(wake) => wake,
                None => WaitNotificationsWake::None,
            };
        }

        let notify = Arc::new(tokio::sync::Notify::new());
        let transition_notify = Arc::clone(&notify);
        let _transition_subscription = self.store.subscribe_agent_transitions(move |_previous, _current| {
            transition_notify.notify_waiters();
            Ok(())
        });
        let wake_up: Arc<Mutex<Option<WaitAgentsWakeUp>>> = Arc::new(Mutex::new(None));
        let listener_notify = Arc::clone(&notify);
        let listener_wake_up = Arc::clone(&wake_up);
        let _wake_subscription = self.wait_wake_listeners.add(Arc::new(move |wake| {
            *listener_wake_up.lock().expect("wait wake up") = Some(wake.clone());
            listener_notify.notify_waiters();
        }));
        if let Some(wake) = self.reconcile_dead_detached_runtimes() {
            return wake;
        }
        if let Some(wake) = self.settle_for_terminal_agent_or_check_coordination() {
            return wake;
        }

        if self.deadline.is_some_and(|deadline| now_ms() >= deadline) {
            return WaitNotificationsWake::Timeout;
        }
        let deadline_timer = async {
            match self.deadline {
                Some(deadline) => {
                    tokio::time::sleep(Duration::from_millis(deadline.saturating_sub(now_ms()).max(0) as u64)).await
                }
                None => std::future::pending().await,
            }
        };
        tokio::pin!(deadline_timer);
        loop {
            let sleep = tokio::time::sleep(Duration::from_millis(RUNTIME_COORDINATION_POLL_INTERVAL_MS));
            let notified = notify.notified();
            tokio::pin!(sleep);
            tokio::pin!(notified);
            let cancelled = async {
                match self.signal.as_ref() {
                    Some(signal) => {
                        signal.wait().await;
                    }
                    None => std::future::pending().await,
                }
            };
            tokio::pin!(cancelled);
            tokio::select! {
                () = &mut deadline_timer => return self.settle_for_terminal_agent_or_check_coordination().unwrap_or(WaitNotificationsWake::Timeout),
                _ = &mut cancelled => return WaitNotificationsWake::Cancelled,
                () = &mut sleep => {
                    if let Some(wake) = self.on_poll() {
                        return wake;
                    }
                }
                () = &mut notified => {
                    if let Some(wake) = self.on_notified(&wake_up) {
                        return wake;
                    }
                }
            }
        }
    }

    fn on_notified(&self, wake_up: &Mutex<Option<WaitAgentsWakeUp>>) -> Option<WaitNotificationsWake> {
        if let Some(wake) = self.read_tracked_terminal_wake() {
            return Some(wake);
        }
        if let Some(wake_up) = wake_up.lock().expect("wait wake up").take() {
            if let Some(agent_id) = wake_up.agent_id.as_ref()
                && !self.tracked_agent_ids.contains(agent_id)
            {
                return None;
            }
            if wake_up.kind == super::shared::WaitAgentsWakeKind::Coordination {
                return Some(WaitNotificationsWake::Coordination {
                    prompt: wake_up.coordination_prompt,
                });
            }
            return Some(WaitNotificationsWake::WakeUp { wake_up });
        }
        self.check_coordination()
    }

    fn on_poll(&self) -> Option<WaitNotificationsWake> {
        if let Some(wake) = self.reconcile_dead_detached_runtimes() {
            return Some(wake);
        }
        self.settle_for_terminal_agent_or_check_coordination().or_else(|| {
            self.deadline
                .filter(|deadline| now_ms() >= *deadline)
                .map(|_| WaitNotificationsWake::Timeout)
        })
    }

    fn settle_for_terminal_agent_or_check_coordination(&self) -> Option<WaitNotificationsWake> {
        if let Some(wake) = self.read_tracked_terminal_wake() {
            return Some(wake);
        }
        self.check_coordination()
    }

    fn read_tracked_terminal_wake(&self) -> Option<WaitNotificationsWake> {
        let agent_ids: Vec<JsString> = self.tracked_agent_ids.iter().cloned().collect();
        match read_multi_agent_agents(&self.control_db_path, &self.session_path, &agent_ids) {
            Ok(agents) => agents
                .into_iter()
                .find(|agent| !is_active_lifecycle(agent.lifecycle))
                .map(|agent| WaitNotificationsWake::Agent { agent: Box::new(agent) }),
            Err(error) => Some(WaitNotificationsWake::Error {
                message: error.message().clone(),
            }),
        }
    }

    fn reconcile_dead_detached_runtimes(&self) -> Option<WaitNotificationsWake> {
        let now_iso = crate::utils::iso_time::now_iso_string();
        let artifact_root = super::runtime::artifact_root_for_store(&self.store);
        match LifecycleCoordinator::reconcile_dead_detached_runtimes(
            &self.control_db_path,
            &now_iso,
            artifact_root.as_ref(),
        ) {
            Ok(0) => None,
            Ok(_) => {
                let claim_id = match pi_ai::utils::random_uuid::random_uuid() {
                    Ok(id) => id,
                    Err(error) => pi_js_compat::callback_error::throw_callback_uncaught(error.into()),
                };
                // Frozen runtime.ts:2244-2253 reuses this reconciliation's nowIso.
                let outbox_now = now_iso.clone();
                if let Err(error) = deliver_terminal_outbox_projections(&DeliverTerminalOutboxOptions {
                    artifact_root,
                    claim_id: claim_id.into(),
                    control_db_path: self.control_db_path.clone(),
                    now: Arc::new(move || Ok(outbox_now.clone().into())),
                    store: Arc::clone(&self.store),
                }) {
                    return Some(WaitNotificationsWake::Error {
                        message: error.message.clone(),
                    });
                }
                None
            }
            Err(error) => Some(WaitNotificationsWake::Error {
                message: error.message().clone(),
            }),
        }
    }

    fn check_coordination(&self) -> Option<WaitNotificationsWake> {
        let recipient = self.recipient.as_ref()?;
        match has_pending_runtime_coordination_message(&recipient.control_db_path, &recipient.address) {
            Ok(true) => Some(WaitNotificationsWake::Coordination { prompt: None }),
            Ok(false) => None,
            Err(error) => Some(WaitNotificationsWake::Error {
                message: error.message().clone(),
            }),
        }
    }
}

pub async fn wait_notifications_runtime(
    store: &Arc<MultiAgentStore>,
    runtime_handles: &super::runtime::MultiAgentRuntimeHandles,
    signal: Option<pi_js_compat::abort::AbortSignal>,
    ctx: Option<&ExtensionContext>,
) -> Result<WaitNotificationsWake, HandlerError> {
    if let Some(ctx) = ctx
        && is_child_agent_runtime(ctx)?
    {
        return Ok(WaitNotificationsWake::Unavailable {
            message: CHILD_ORCHESTRATION_UNAVAILABLE_MESSAGE.into(),
        });
    }
    if let Some(ctx) = ctx {
        mirror_pending_lifecycle_runtime_mailbox_messages(store, ctx)?;
    }
    let Some(persistence) = store.get_persistence_target() else {
        return Ok(WaitNotificationsWake::Unavailable {
            message: "wait_agent requires a persisted supervisor session.".into(),
        });
    };
    let pending = list_pending_terminal_notifications(store, &persistence.control_db_path, &persistence.session_path)?;
    if let Some(first) = pending.first() {
        return Ok(WaitNotificationsWake::Agent {
            agent: Box::new(first.0.clone()),
        });
    }
    let recipient = runtime_coordination_recipient(ctx)?;
    let agents = read_multi_agent_state(&persistence.control_db_path, &persistence.session_path)
        .map_err(|error| HandlerError::new(error.message()))?
        .map(|state| {
            state
                .agents
                .into_iter()
                .filter_map(|value| pi_js_compat::js_json::from_value::<AgentSnapshot>(&value).ok())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let deadline = runtime_handles
        .latest_model_request_started_at
        .lock()
        .expect("model request start")
        .map(|started| started + 25 * 60_000);
    Ok(WaitAgentsWakeWatcher::new(WaitAgentsWakeWatcherOptions {
        deadline,
        active_agents: agents
            .into_iter()
            .filter(|agent| is_active_lifecycle(agent.lifecycle))
            .collect(),
        control_db_path: persistence.control_db_path,
        recipient,
        session_path: persistence.session_path,
        signal,
        store: Arc::clone(store),
        wait_wake_listeners: runtime_handles.wait_wake_listeners.clone(),
    })
    .wait()
    .await)
}

pub fn consume_notifications_runtime(
    store: &Arc<MultiAgentStore>,
    wake: &WaitNotificationsWake,
    ctx: Option<&ExtensionContext>,
    format_agent_status: &dyn Fn(&AgentSnapshot) -> pi_js_compat::js_json::JsString,
) -> Result<AgentToolResult, HandlerError> {
    match wake {
        WaitNotificationsWake::Agent { agent } => consume_agent_wake(store, agent, format_agent_status),
        other => consume_non_agent_wake(store, other, ctx),
    }
}

fn consume_non_agent_wake(
    store: &Arc<MultiAgentStore>,
    wake: &WaitNotificationsWake,
    ctx: Option<&ExtensionContext>,
) -> Result<AgentToolResult, HandlerError> {
    match wake {
        WaitNotificationsWake::Agent { .. } => unreachable!("agent wakes are handled separately"),
        WaitNotificationsWake::Cancelled => Ok(tool_result("Wait cancelled.", json!({}))),
        WaitNotificationsWake::Coordination { prompt } => {
            if let Some(prompt) = prompt.as_ref().filter(|prompt| !prompt.is_empty()) {
                return Ok(tool_result(prompt.clone(), json!({})));
            }
            let coordination = take_pending_wait_coordination(runtime_coordination_recipient(ctx)?)?;
            Ok(match coordination {
                Some((prompt, delivery)) => tool_result(
                    prompt,
                    match delivery {
                        Some(delivery) => json!({"runtimeMailboxDelivery": delivery}),
                        None => json!({}),
                    },
                ),
                None => tool_result("Coordination input changed before delivery.", json!({})),
            })
        }
        WaitNotificationsWake::Error { message } => Ok(tool_result(js_format!("Wait failed: {}", message), json!({}))),
        WaitNotificationsWake::None => Ok(empty_tool_result()),
        WaitNotificationsWake::Timeout => Ok(tool_result(
            "Wait slice timed out: agents are still running. No agents were cancelled. You can wait again after the next model request.",
            json!({"timedOut":true}),
        )),
        WaitNotificationsWake::Unavailable { message } => Ok(tool_result(message.clone(), json!({}))),
        WaitNotificationsWake::WakeUp { wake_up } => {
            let agent = wake_up.agent_id.as_ref().and_then(|agent_id| store.get_agent(agent_id));
            let text = match agent {
                Some(agent) => {
                    pi_js_compat::js_json::JsString::from("Woken after steering ") + &agent.display_name + "."
                }
                None => "Woken after supervisor steering.".into(),
            };
            Ok(tool_result(text, json!({"wakeUp": wake_up})))
        }
    }
}

fn consume_agent_wake(
    store: &Arc<MultiAgentStore>,
    agent: &AgentSnapshot,
    format_agent_status: &dyn Fn(&AgentSnapshot) -> pi_js_compat::js_json::JsString,
) -> Result<AgentToolResult, HandlerError> {
    if let Some(persistence) = store.get_persistence_target()
        && let Some(pending) = consume_pending_terminal_notifications(
            store,
            &persistence.control_db_path,
            &persistence.session_path,
            format_agent_status,
        )?
    {
        return Ok(pending);
    }
    Ok(tool_result(format_agent_status(agent), json!({"agent": agent})))
}

fn runtime_coordination_recipient(
    ctx: Option<&ExtensionContext>,
) -> Result<Option<RuntimeCoordinationRecipient>, HandlerError> {
    let Some(ctx) = ctx else {
        return Ok(None);
    };
    let Some(control_db_path) = ctx.control_db_path()? else {
        return Ok(None);
    };
    let address = resolve_own_main_runtime_coordination_recipient(&control_db_path)
        .map_err(|error| HandlerError::new(error.message()))?;
    Ok(address.map(|address| RuntimeCoordinationRecipient {
        address,
        control_db_path,
    }))
}

fn take_pending_wait_coordination(
    recipient: Option<RuntimeCoordinationRecipient>,
) -> Result<Option<(JsString, Option<pi_js_compat::js_json::JsValue>)>, HandlerError> {
    let Some(recipient) = recipient else {
        return Ok(None);
    };
    let mailbox_messages = take_runtime_mailbox_messages_for_delivery(
        &recipient.control_db_path,
        &recipient.address,
        is_runtime_coordination_mailbox_message,
        usize::MAX,
    )
    .map_err(|error| HandlerError::new(error.message()))?;
    let result = (|| {
        let channel_messages = take_pending_wait_shared_channel_messages(&recipient)?;
        let mut sections: Vec<JsString> = mailbox_messages
            .iter()
            .map(|message| format_runtime_mailbox_prompt(message, &recipient.address.session_id))
            .collect::<Result<Vec<_>, _>>()
            .map_err(HandlerError::new)?;
        if !channel_messages.is_empty() {
            sections.push(format_shared_channel_prompt(
                &channel_messages,
                &recipient.address.session_id,
            ));
        }
        let delivery = (!mailbox_messages.is_empty()).then(|| json!({
            "controlDbPath": recipient.control_db_path,
            "messages": mailbox_messages.iter().map(|message| json!({"id":message.id, "storeRef":message.store_ref})).collect::<Vec<_>>(),
        }));
        Ok((!sections.is_empty()).then(|| (JsString::join(sections, "\n\n"), delivery)))
    })();
    if result.is_err() {
        for message in &mailbox_messages {
            release_runtime_mailbox_message_claim(&recipient.control_db_path, message.id)
                .map_err(|error| HandlerError::new(error.message()))?;
        }
    }
    result
}

fn take_pending_wait_shared_channel_messages(
    recipient: &RuntimeCoordinationRecipient,
) -> Result<Vec<SharedChannelMessage>, HandlerError> {
    if recipient.address.agent_id.is_some() {
        return Ok(Vec::new());
    }
    let Some(cursor) = read_shared_channel_cursor(&recipient.control_db_path, &recipient.address)
        .map_err(|error| HandlerError::new(error.message()))?
    else {
        return Ok(Vec::new());
    };
    let last_message_id =
        read_shared_channel_tail(&recipient.control_db_path).map_err(|error| HandlerError::new(error.message()))?;
    let messages = read_shared_channel_messages_through(&recipient.control_db_path, cursor, last_message_id)?;
    advance_shared_channel_cursor(&recipient.control_db_path, &recipient.address, last_message_id)
        .map_err(|error| HandlerError::new(error.message()))?;
    Ok(messages
        .into_iter()
        .filter(|message| {
            message.sender.agent_id.is_none()
                && (message.origin_host.is_some() || message.sender.session_id != recipient.address.session_id)
        })
        .collect())
}

fn read_shared_channel_messages_through(
    control_db_path: &JsString,
    cursor: i64,
    last_message_id: i64,
) -> Result<Vec<SharedChannelMessage>, HandlerError> {
    let mut messages = Vec::new();
    let mut cursor = cursor;
    while cursor < last_message_id {
        let page =
            list_shared_channel_messages_after(control_db_path, cursor, SHARED_CHANNEL_PAGE_LIMIT, last_message_id)
                .map_err(|error| HandlerError::new(error.message()))?;
        let Some(last_page_message) = page.last() else {
            break;
        };
        cursor = last_page_message.id;
        messages.extend(page);
    }
    Ok(messages)
}

fn terminal_notification_lifecycle(
    agent: &AgentSnapshot,
) -> Option<crate::core::multi_agent_store::LifecycleNotificationLifecycle> {
    match agent.lifecycle {
        crate::core::multi_agent_store::AgentLifecycle::Completed => {
            Some(crate::core::multi_agent_store::LifecycleNotificationLifecycle::Completed)
        }
        crate::core::multi_agent_store::AgentLifecycle::Failed => {
            Some(crate::core::multi_agent_store::LifecycleNotificationLifecycle::Failed)
        }
        _ => None,
    }
}

fn list_pending_terminal_notifications(
    store: &MultiAgentStore,
    control_db_path: &JsString,
    session_path: &JsString,
) -> Result<Vec<(AgentSnapshot, AgentMailboxMessage)>, HandlerError> {
    let persisted_agents: Vec<AgentSnapshot> = read_multi_agent_state(control_db_path, session_path)
        .map_err(|error| HandlerError::new(error.message()))?
        .map(|state| {
            state
                .agents
                .iter()
                .map(|value| {
                    pi_js_compat::js_json::from_value(value).map_err(|error| HandlerError::new(error.to_string()))
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?
        .unwrap_or_default();
    let mut pending = Vec::new();
    for agent in persisted_agents {
        let Some(lifecycle) = terminal_notification_lifecycle(&agent) else {
            continue;
        };
        if let Some(message) = store
            .list_pending_lifecycle_notifications_for_agent(&agent.id, lifecycle)
            .map_err(|error| HandlerError::new(error.message))?
            .into_iter()
            .next()
        {
            pending.push((agent, message));
        }
    }
    Ok(pending)
}

fn consume_pending_terminal_notifications(
    store: &MultiAgentStore,
    control_db_path: &JsString,
    session_path: &JsString,
    format_agent_status: &dyn Fn(&AgentSnapshot) -> pi_js_compat::js_json::JsString,
) -> Result<Option<AgentToolResult>, HandlerError> {
    let pending = list_pending_terminal_notifications(store, control_db_path, session_path)?;
    if pending.is_empty() {
        return Ok(None);
    }
    for (_, message) in &pending {
        store
            .mark_mailbox_message_delivered(&message.id)
            .map_err(|error| HandlerError::new(error.message))?;
    }
    let agents: Vec<AgentSnapshot> = pending.iter().map(|(agent, _)| agent.clone()).collect();
    let messages: Vec<AgentMailboxMessage> = pending.iter().map(|(_, message)| message.clone()).collect();
    let text = pi_js_compat::js_json::JsString::join(
        pending
            .iter()
            .map(|(agent, message)| {
                message
                    .body
                    .clone()
                    .map(pi_js_compat::js_json::JsString::from)
                    .unwrap_or_else(|| format_agent_status(agent))
            })
            .collect::<Vec<_>>(),
        "\n",
    );
    Ok(Some(tool_result(
        text,
        json!({
            "agent": agents.first(),
            "agents": agents,
            "message": messages.first(),
            "messages": messages,
        }),
    )))
}

#[cfg(test)]
mod tracked_read_failure_tests {
    use super::*;
    use crate::core::multi_agent_store::MultiAgentStoreOptions;

    #[test]
    fn tracked_terminal_wake_preserves_captured_agent_order() {
        let directory = tempfile::tempdir().unwrap();
        let db_path: JsString = directory.path().join("control.db").to_str().unwrap().into();
        let session_path: JsString = "/fixture/session".into();
        read_multi_agent_state(&db_path, &session_path).unwrap();
        let db = crate::core::sqlite::create_sqlite_database(&db_path.to_utf8().unwrap()).unwrap();
        let mut agents = (0..32)
            .map(|id| super::super::shared::empty_agent(format!("agent_{id}")))
            .collect::<Vec<_>>();
        for agent in &agents {
            db.execute(
                "INSERT INTO multi_agent_agents(session_path, agent_id, data, updated_at) VALUES (?, ?, ?, ?)",
                [
                    session_path.to_utf8().unwrap(),
                    agent.id.to_utf8().unwrap(),
                    pi_js_compat::js_json::stringify_lossless(&json!(agent)),
                    "1970-01-01T00:00:00.000Z".to_owned(),
                ],
            )
            .unwrap();
        }
        drop(db);
        for _ in 0..16 {
            agents.reverse();
            let watcher = WaitAgentsWakeWatcher::new(WaitAgentsWakeWatcherOptions {
                deadline: None,
                active_agents: agents.clone(),
                control_db_path: db_path.clone(),
                recipient: None,
                session_path: session_path.clone(),
                signal: None,
                store: Arc::new(MultiAgentStore::new(Default::default())),
                wait_wake_listeners: Default::default(),
            });
            let Some(WaitNotificationsWake::Agent { agent }) = watcher.read_tracked_terminal_wake() else {
                panic!("terminal wake required")
            };
            assert_eq!(agent.id, agents[0].id);
        }
    }

    #[test]
    fn tracked_agent_read_failure_wakes_with_error_instead_of_disappearing() {
        let directory = tempfile::tempdir().unwrap();
        let blocked_db: JsString = directory.path().to_str().unwrap().into();
        let session_path: JsString = "/fixture/session".into();
        let agent_ids = vec![JsString::from("agent_1")];
        let expected = read_multi_agent_agents(&blocked_db, &session_path, &agent_ids).unwrap_err();
        let watcher = WaitAgentsWakeWatcher {
            deadline: None,
            control_db_path: blocked_db,
            recipient: None,
            session_path,
            signal: None,
            store: Arc::new(MultiAgentStore::new(MultiAgentStoreOptions::default())),
            tracked_agent_ids: agent_ids.into_iter().collect(),
            wait_wake_listeners: WaitWakeListeners::default(),
        };
        for wake in [
            watcher.settle_for_terminal_agent_or_check_coordination(),
            watcher.on_notified(&Mutex::new(None)),
        ] {
            let Some(WaitNotificationsWake::Error { message }) = wake else {
                panic!("tracked DB failure must become an error wake, not an absent agent");
            };
            assert_eq!(&message, expected.message());
        }
    }
}
