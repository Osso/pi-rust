//! Child agent dispatch (`extensions/agents-core/src/dispatch-runtime.ts`).

use pi_js_compat::js_json::JsString;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use futures_util::FutureExt;
use pi_ai::request::BoxFuture;
use pi_ai::types::AssistantContent;

use crate::core::desktop_notification::{
    DesktopNotification, DesktopNotificationHandle, DesktopNotifier, PERSISTENT_DESKTOP_NOTIFICATION_EXPIRE_TIME_MS,
};
use crate::core::extensions::types::{ExtensionContext, HandlerError};
use crate::core::lifecycle_coordinator::{
    FinalizeChildCommandInput, LifecycleCommandResult, OwnedLifecycleCommandInput,
};
use crate::core::multi_agent_store::{
    AgentError, AgentLifecycle, AgentMailboxMessage, AgentResult, AgentSnapshot, MailboxMessageKindValue,
    MailboxMessageStatusValue, MultiAgentStore, is_active_lifecycle,
};

use super::control_runtime::OwnedAgentRuntime;
use super::lifecycle_runtime::{create_lifecycle_coordinator, publish_coordinator_snapshot};
use super::parent_agent_journal::{ParentAgentJournalWriter, append_parent_agent_record};
use super::runtime::SharedDispatch;
use super::shared::AttachedSessionDispatchInput as AttachedSessionFactoryInput;
use super::shared::{
    AttachedSessionFactory, ChildAgentDispatchInput, ChildAgentSession, ChildAgentSessionFactory, SpawnContext,
};

pub const AGENT_START_CUSTOM_TYPE: &str = "agent_start";
pub const AGENT_COMPLETE_CUSTOM_TYPE: &str = "agent_complete";

pub type BackgroundSessionHandles = Arc<Mutex<HashMap<JsString, Arc<dyn ChildAgentSession>>>>;
pub type ActiveAgentDispatches = Arc<Mutex<HashMap<JsString, SharedDispatch>>>;

pub struct WaitingDesktopNotificationRegistration {
    pub handle: Box<dyn DesktopNotificationHandle>,
    pub unsubscribe_transition: crate::core::multi_agent_store::AgentSubscription,
}

pub type WaitingDesktopNotificationHandles =
    Arc<Mutex<HashMap<pi_js_compat::js_json::JsString, WaitingDesktopNotificationRegistration>>>;

/// `AttachSessionDispatchInput` from `dispatch-runtime.ts`.
#[derive(Clone)]
pub struct AttachSessionDispatchInput {
    pub create_attached_session: Option<AttachedSessionFactory>,
    pub ctx: ExtensionContext,
    pub desktop_notifier: Arc<dyn DesktopNotifier>,
    pub dispatches: ActiveAgentDispatches,
    pub handles: Option<BackgroundSessionHandles>,
    pub ownerships: Arc<Mutex<HashMap<JsString, Arc<OwnedAgentRuntime>>>>,
    pub prompt: pi_js_compat::js_json::JsString,
    pub store: Arc<MultiAgentStore>,
    pub target: AgentSnapshot,
    pub waiting_desktop_notifications: WaitingDesktopNotificationHandles,
}

pub struct StartToolDispatchInput {
    pub agent: AgentSnapshot,
    pub desktop_notifier: Arc<dyn DesktopNotifier>,
    pub dispatch:
        Arc<dyn Fn() -> BoxFuture<'static, Result<AgentSnapshot, pi_js_compat::js_json::JsString>> + Send + Sync>,
    pub dispatches: ActiveAgentDispatches,
    pub handles: Option<BackgroundSessionHandles>,
    pub reserved_runtime: Arc<OwnedAgentRuntime>,
    pub store: Arc<MultiAgentStore>,
    pub waiting_desktop_notifications: WaitingDesktopNotificationHandles,
}

pub struct TrackAgentDispatchInput {
    pub agent: AgentSnapshot,
    pub dispatch: BoxFuture<'static, Result<AgentSnapshot, pi_js_compat::js_json::JsString>>,
    pub dispatches: ActiveAgentDispatches,
    pub handles: Option<BackgroundSessionHandles>,
    pub reserved_runtime: Arc<OwnedAgentRuntime>,
    pub store: Arc<MultiAgentStore>,
}

#[derive(Clone)]
pub struct RunAgentSessionInput {
    pub create_child_session: ChildAgentSessionFactory,
    pub created_session: Option<Arc<dyn ChildAgentSession>>,
    pub ctx: ExtensionContext,
    pub expected_restore_generation: Option<u64>,
    pub handles: Option<BackgroundSessionHandles>,
    pub prompt: pi_js_compat::js_json::JsString,
    pub reserved_runtime: Arc<OwnedAgentRuntime>,
    pub running_agent: AgentSnapshot,
    pub store: Arc<MultiAgentStore>,
}

#[derive(Clone)]
struct DispatchReservedAgentSessionInput {
    create_child_session: ChildAgentSessionFactory,
    ctx: ExtensionContext,
    handles: Option<BackgroundSessionHandles>,
    prompt: pi_js_compat::js_json::JsString,
    reserved_runtime: Arc<OwnedAgentRuntime>,
    store: Arc<MultiAgentStore>,
}

#[derive(Clone)]
struct FinalizeReservedRuntimeInput {
    agent: AgentSnapshot,
    expected_restore_generation: Option<u64>,
    error: Option<AgentError>,
    result: Option<AgentResult>,
    reserved_runtime: Arc<OwnedAgentRuntime>,
    store: Arc<MultiAgentStore>,
    terminal_lifecycle: &'static str,
}

pub fn is_terminal_agent_lifecycle(lifecycle: AgentLifecycle) -> bool {
    matches!(
        lifecycle,
        AgentLifecycle::Completed | AgentLifecycle::Failed | AgentLifecycle::Aborted
    )
}

pub fn append_parent_agent_record_for(
    pi: &dyn ParentAgentJournalWriter,
    custom_type: &str,
    agent: &AgentSnapshot,
) -> Result<(), HandlerError> {
    append_parent_agent_record(pi, custom_type, agent)
}

pub fn dispatch_attached_session_agent(
    input: &AttachSessionDispatchInput,
) -> Result<Option<AgentSnapshot>, HandlerError> {
    let Some(reserved_runtime) = reserve_attached_runtime(input)? else {
        return Ok(None);
    };
    let target = reserved_runtime.lifecycle.agent.clone();
    input
        .ownerships
        .lock()
        .expect("ownerships")
        .insert(target.id.clone(), Arc::clone(&reserved_runtime));
    input
        .store
        .publish_lifecycle_coordinator_snapshot(&target)
        .map_err(|error| HandlerError::new(error.message))?;
    let Some(create_attached_session) = input.create_attached_session.clone() else {
        return Ok(None);
    };
    let dispatch_input = input.clone();
    let dispatch_runtime = Arc::clone(&reserved_runtime);
    let dispatched = start_tool_dispatch(&StartToolDispatchInput {
        agent: target.clone(),
        desktop_notifier: Arc::clone(&input.desktop_notifier),
        dispatch: Arc::new(move || {
            dispatch_reserved_attached_child_session(
                &dispatch_input,
                create_attached_session.clone(),
                Arc::clone(&dispatch_runtime),
            )
        }),
        dispatches: Arc::clone(&input.dispatches),
        handles: input.handles.clone(),
        reserved_runtime: Arc::clone(&reserved_runtime),
        store: Arc::clone(&input.store),
        waiting_desktop_notifications: Arc::clone(&input.waiting_desktop_notifications),
    });
    release_ownership_after_dispatch(&input.dispatches, &input.ownerships, &target.id);
    Ok(Some(dispatched))
}

fn reserve_attached_runtime(
    input: &AttachSessionDispatchInput,
) -> Result<Option<Arc<OwnedAgentRuntime>>, HandlerError> {
    let Some(coordinator) = create_lifecycle_coordinator(&input.store) else {
        return Ok(None);
    };
    let owner_session_id = input.ctx.session_manager()?.get_session_id().to_owned();
    let acquired = coordinator
        .acquire_attached_runtime(&input.target, &owner_session_id)
        .map_err(|error| HandlerError::new(error.message()))?;
    match acquired {
        crate::core::session_control_db::AcquireAttachedRuntimeOwnershipResult::Ok { agent, ownership } => {
            Ok(Some(Arc::new(OwnedAgentRuntime {
                abort_controller: pi_js_compat::abort::AbortController::default(),
                coordinator,
                lifecycle: OwnedLifecycleCommandInput {
                    agent: *agent,
                    ownership: *ownership,
                },
            })))
        }
        crate::core::session_control_db::AcquireAttachedRuntimeOwnershipResult::Err { .. } => Ok(None),
    }
}

fn dispatch_reserved_attached_child_session(
    input: &AttachSessionDispatchInput,
    create_attached_session: AttachedSessionFactory,
    reserved_runtime: Arc<OwnedAgentRuntime>,
) -> BoxFuture<'static, Result<AgentSnapshot, pi_js_compat::js_json::JsString>> {
    let session_path = input
        .target
        .transcript
        .as_ref()
        .and_then(|transcript| transcript.path.clone())
        .unwrap_or_default();
    let factory = ChildAgentSessionFactory::new(move |dispatch_input: ChildAgentDispatchInput| {
        create_attached_session.call(AttachedSessionFactoryInput {
            active_tool_call_id: dispatch_input.active_tool_call_id,
            agent: dispatch_input.agent,
            ctx: dispatch_input.ctx,
            prompt: dispatch_input.prompt,
            signal: dispatch_input.signal,
            session_path: session_path.clone(),
        })
    });
    if reserved_runtime.lifecycle.agent.lifecycle != AgentLifecycle::WaitingForInput {
        return run_agent_session(RunAgentSessionInput {
            create_child_session: factory,
            created_session: None,
            ctx: input.ctx.clone(),
            expected_restore_generation: None,
            handles: input.handles.clone(),
            prompt: input.prompt.clone(),
            reserved_runtime: Arc::clone(&reserved_runtime),
            running_agent: reserved_runtime.lifecycle.agent.clone(),
            store: Arc::clone(&input.store),
        });
    }
    Box::pin(dispatch_reserved_agent_session(DispatchReservedAgentSessionInput {
        create_child_session: factory,
        ctx: input.ctx.clone(),
        handles: input.handles.clone(),
        prompt: input.prompt.clone(),
        reserved_runtime,
        store: Arc::clone(&input.store),
    }))
}

pub fn start_tool_dispatch(input: &StartToolDispatchInput) -> AgentSnapshot {
    let unsubscribe = subscribe_waiting_notifications(input);
    let _tracked = track_agent_dispatch(TrackAgentDispatchInput {
        agent: input.agent.clone(),
        dispatch: (input.dispatch)(),
        dispatches: Arc::clone(&input.dispatches),
        handles: input.handles.clone(),
        reserved_runtime: Arc::clone(&input.reserved_runtime),
        store: Arc::clone(&input.store),
    });
    let dispatch = input
        .dispatches
        .lock()
        .expect("dispatches")
        .get(&input.agent.id)
        .cloned();
    let store = Arc::clone(&input.store);
    let agent_id = input.agent.id.clone();
    let waiting = Arc::clone(&input.waiting_desktop_notifications);
    tokio::spawn(async move {
        if let Some(dispatch) = dispatch {
            let _ = dispatch.await;
        }
        unsubscribe();
        close_waiting_desktop_notification_when_not_waiting(&store, &agent_id, &waiting);
    });
    input
        .store
        .get_agent(&input.agent.id)
        .unwrap_or_else(|| input.agent.clone())
}

fn subscribe_waiting_notifications(input: &StartToolDispatchInput) -> impl FnOnce() + Send + 'static {
    let agent_id = input.agent.id.clone();
    let desktop_notifier = Arc::clone(&input.desktop_notifier);
    let waiting = Arc::clone(&input.waiting_desktop_notifications);
    let store = Arc::clone(&input.store);
    let listener_store = Arc::clone(&store);
    let subscription = listener_store.subscribe_lifecycle_notifications(move |message| {
        if message.from_agent_id != agent_id {
            return;
        }
        let notification_handle = notify_waiting_agent(message, desktop_notifier.as_ref());
        if is_waiting_for_input_notification(message) {
            remember_waiting_desktop_notification(&store, message, notification_handle, &waiting);
            return;
        }
        close_waiting_desktop_notification(&message.from_agent_id, &waiting);
    });
    move || drop(subscription)
}

pub fn track_agent_dispatch(
    input: TrackAgentDispatchInput,
) -> BoxFuture<'static, Result<AgentSnapshot, pi_js_compat::js_json::JsString>> {
    let restore_generation = input.store.get_restore_generation();
    let dispatch = input.dispatch;
    let store = Arc::clone(&input.store);
    let reserved_runtime = Arc::clone(&input.reserved_runtime);
    let agent = input.agent.clone();
    let dispatches = Arc::clone(&input.dispatches);
    let handles = input.handles.clone();
    let tracked: BoxFuture<'static, Result<AgentSnapshot, pi_js_compat::js_json::JsString>> = Box::pin(async move {
        let result = match dispatch.await {
            Ok(agent) => Ok(agent),
            Err(error) => {
                let current = store.get_agent(&agent.id).unwrap_or_else(|| agent.clone());
                Ok(finalize_reserved_runtime(FinalizeReservedRuntimeInput {
                    agent: current,
                    expected_restore_generation: Some(restore_generation),
                    error: Some(AgentError {
                        message: error,
                        code: None,
                    }),
                    result: None,
                    reserved_runtime,
                    store: Arc::clone(&store),
                    terminal_lifecycle: "failed",
                }))
            }
        };
        if let Some(handles) = handles.as_ref() {
            handles.lock().expect("handles").remove(&agent.id);
        }
        dispatches.lock().expect("dispatches").remove(&agent.id);
        result
    });
    let shared: SharedDispatch = tracked.shared();
    input
        .dispatches
        .lock()
        .expect("dispatches")
        .insert(input.agent.id.clone(), shared.clone());
    Box::pin(shared)
}

async fn dispatch_reserved_agent_session(
    input: DispatchReservedAgentSessionInput,
) -> Result<AgentSnapshot, pi_js_compat::js_json::JsString> {
    let restore_generation = input.store.get_restore_generation();
    let creation = create_reserved_child_session(&input).await;
    let (child_session, running_agent) = match creation {
        Ok(created) => created,
        Err(agent) => return Ok(*agent),
    };
    if input.store.get_restore_generation() != restore_generation {
        child_session.abort();
        child_session.dispose();
        return Ok(input
            .store
            .get_agent(&input.reserved_runtime.lifecycle.agent.id)
            .unwrap_or_else(|| input.reserved_runtime.lifecycle.agent.clone()));
    }
    let running = input
        .reserved_runtime
        .coordinator
        .confirm_child_runtime(&input.reserved_runtime.lifecycle)
        .map_err(|error| error.message().to_owned())?;
    let running_agent = match running {
        LifecycleCommandResult::Ok { agent } => {
            input
                .store
                .publish_lifecycle_coordinator_snapshot(&agent)
                .map_err(|error| error.message)?;
            *agent
        }
        LifecycleCommandResult::Err { .. } => {
            child_session.abort();
            child_session.dispose();
            return Ok(running_agent);
        }
    };
    run_confirmed_child_session(input, child_session, running_agent, restore_generation).await
}

async fn create_reserved_child_session(
    input: &DispatchReservedAgentSessionInput,
) -> Result<(Arc<dyn ChildAgentSession>, AgentSnapshot), Box<AgentSnapshot>> {
    let lifecycle = input.reserved_runtime.lifecycle.clone();
    match input
        .create_child_session
        .call(ChildAgentDispatchInput {
            active_tool_call_id: None,
            agent: lifecycle.agent.clone(),
            context: SpawnContext::Fresh,
            ctx: input.ctx.clone(),
            prompt: input.prompt.clone(),
            signal: Some(input.reserved_runtime.abort_controller.signal()),
        })
        .await
    {
        Ok(session) => Ok((session, lifecycle.agent)),
        Err(error) => {
            let failed = input
                .reserved_runtime
                .coordinator
                .finalize_child(&FinalizeChildCommandInput {
                    agent: lifecycle.agent.clone(),
                    ownership: lifecycle.ownership,
                    error: Some(AgentError {
                        code: Some("runtime_spawn_failed".to_owned()),
                        message: error,
                    }),
                    result: None,
                    terminal_lifecycle: "failed".to_owned(),
                });
            match failed {
                Ok(LifecycleCommandResult::Ok { agent }) => {
                    let _ = publish_coordinator_snapshot(&input.store, &agent);
                    Err(agent)
                }
                _ => Err(Box::new(lifecycle.agent)),
            }
        }
    }
}

async fn run_confirmed_child_session(
    input: DispatchReservedAgentSessionInput,
    child_session: Arc<dyn ChildAgentSession>,
    running_agent: AgentSnapshot,
    restore_generation: u64,
) -> Result<AgentSnapshot, pi_js_compat::js_json::JsString> {
    run_agent_session(RunAgentSessionInput {
        create_child_session: input.create_child_session,
        created_session: Some(child_session),
        ctx: input.ctx,
        expected_restore_generation: Some(restore_generation),
        handles: input.handles,
        prompt: input.prompt,
        reserved_runtime: Arc::new(OwnedAgentRuntime {
            abort_controller: input.reserved_runtime.abort_controller.clone(),
            coordinator: Arc::clone(&input.reserved_runtime.coordinator),
            lifecycle: OwnedLifecycleCommandInput {
                agent: running_agent.clone(),
                ownership: input.reserved_runtime.lifecycle.ownership.clone(),
            },
        }),
        running_agent,
        store: input.store,
    })
    .await
}

pub fn run_agent_session(
    input: RunAgentSessionInput,
) -> BoxFuture<'static, Result<AgentSnapshot, pi_js_compat::js_json::JsString>> {
    Box::pin(async move {
        let restore_generation = input
            .expected_restore_generation
            .unwrap_or_else(|| input.store.get_restore_generation());
        let active_session = match create_active_child_session(&input).await {
            Ok(session) => session,
            Err(error) => return fail_child_session(&input, &error, restore_generation).await,
        };
        let lease_loss_abort = register_lease_loss_abort(&input, Arc::clone(&active_session));
        lease_loss_abort.abort_if_lease_already_lost();
        if !admit_active_child_session(&input, &active_session, restore_generation) {
            let agent = input
                .store
                .get_agent(&input.running_agent.id)
                .unwrap_or_else(|| input.running_agent.clone());
            return Ok(agent);
        }
        let abort_registration = register_store_abort_handler(&input);
        publish_active_session_identity(&input, &active_session);
        let outcome = prompt_child_and_wait_for_descendants(&input, &active_session).await;
        let agent = match outcome {
            Ok(()) => complete_prompted_child_session(&input, &active_session, restore_generation).await,
            Err(error) => fail_child_session(&input, &error, restore_generation).await,
        };
        drop(abort_registration);
        drop(lease_loss_abort);
        if let Some(handles) = input.handles.as_ref() {
            handles.lock().expect("handles").remove(&input.running_agent.id);
        }
        active_session.dispose();
        agent
    })
}

async fn create_active_child_session(
    input: &RunAgentSessionInput,
) -> Result<Arc<dyn ChildAgentSession>, pi_js_compat::js_json::JsString> {
    if let Some(session) = input.created_session.clone() {
        return Ok(session);
    }
    input
        .create_child_session
        .call(ChildAgentDispatchInput {
            active_tool_call_id: None,
            agent: input.running_agent.clone(),
            context: SpawnContext::Fresh,
            ctx: input.ctx.clone(),
            prompt: input.prompt.clone(),
            signal: Some(input.reserved_runtime.abort_controller.signal()),
        })
        .await
}

struct LeaseLossAbort {
    task: tokio::task::JoinHandle<()>,
    token: pi_js_compat::abort::AbortSignal,
    session: Arc<dyn ChildAgentSession>,
}

impl LeaseLossAbort {
    fn abort_if_lease_already_lost(&self) {
        if self.token.aborted() {
            self.session.abort();
        }
    }
}

impl Drop for LeaseLossAbort {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn register_lease_loss_abort(
    input: &RunAgentSessionInput,
    active_session: Arc<dyn ChildAgentSession>,
) -> LeaseLossAbort {
    let token = input.reserved_runtime.abort_controller.signal();
    let task_token = token.clone();
    let task_session = Arc::clone(&active_session);
    let task = tokio::spawn(async move {
        task_token.wait().await;
        task_session.abort();
    });
    LeaseLossAbort {
        task,
        token,
        session: active_session,
    }
}

fn admit_active_child_session(
    input: &RunAgentSessionInput,
    active_session: &Arc<dyn ChildAgentSession>,
    restore_generation: u64,
) -> bool {
    let current = input.store.get_agent(&input.running_agent.id);
    let is_current_generation = input.store.get_restore_generation() == restore_generation;
    let is_current_agent_active = current
        .as_ref()
        .is_some_and(|agent| is_active_lifecycle(agent.lifecycle));
    if is_current_generation && is_current_agent_active {
        return true;
    }
    active_session.abort();
    false
}

fn register_store_abort_handler(input: &RunAgentSessionInput) -> crate::core::multi_agent_store::AbortRegistration {
    let abort_controller = input.reserved_runtime.abort_controller.clone();
    let handles = input.handles.clone();
    let agent_id = input.running_agent.id.clone();
    input
        .store
        .register_agent_abort_handler(&input.running_agent.id, move || {
            abort_controller.abort(None);
            if let Some(handles) = handles.as_ref() {
                handles.lock().expect("handles").remove(&agent_id);
            }
        })
}

fn publish_active_session_identity(input: &RunAgentSessionInput, active_session: &Arc<dyn ChildAgentSession>) {
    if let Some(handles) = input.handles.as_ref() {
        handles
            .lock()
            .expect("handles")
            .insert(input.running_agent.id.clone(), Arc::clone(active_session));
    }
    if let Some(transcript) = active_session.transcript() {
        let _ = input
            .store
            .update_agent_transcript(&input.running_agent.id, &transcript);
    }
}

async fn prompt_child_and_wait_for_descendants(
    input: &RunAgentSessionInput,
    active_session: &Arc<dyn ChildAgentSession>,
) -> Result<(), pi_js_compat::js_json::JsString> {
    active_session.prompt(input.prompt.clone()).await?;
    settle_descendants_before_terminalization(
        &input.store,
        &input.running_agent.id,
        Some(&input.reserved_runtime.abort_controller.signal()),
    )
    .await?;
    Ok(())
}

async fn complete_prompted_child_session(
    input: &RunAgentSessionInput,
    active_session: &Arc<dyn ChildAgentSession>,
    restore_generation: u64,
) -> Result<AgentSnapshot, JsString> {
    if let Some(cancelled) = acknowledge_cancelled_runtime(
        &input.store,
        &input.running_agent.id,
        Some(&input.reserved_runtime),
        Some(restore_generation),
    ) {
        return Ok(cancelled);
    }
    loop {
        let summary = last_assistant_text(&active_session.messages());
        let completion_result = summary.map(|summary| AgentResult {
            summary: Some(summary.into()),
            file_refs: None,
            duration_ms: None,
            tool_call_id: None,
        });
        let current = input
            .store
            .get_agent(&input.running_agent.id)
            .unwrap_or_else(|| input.running_agent.clone());
        let completed = finalize_reserved_runtime(FinalizeReservedRuntimeInput {
            agent: current,
            expected_restore_generation: Some(restore_generation),
            error: None,
            result: completion_result,
            reserved_runtime: Arc::clone(&input.reserved_runtime),
            store: Arc::clone(&input.store),
            terminal_lifecycle: "completed",
        });
        if completed.lifecycle != AgentLifecycle::SteeringPending {
            return Ok(completed);
        }
        if let Err(error) = drain_pending_steering(input, active_session).await {
            return fail_child_session(input, &error, restore_generation).await;
        }
    }
}

async fn drain_pending_steering(
    input: &RunAgentSessionInput,
    active_session: &Arc<dyn ChildAgentSession>,
) -> Result<(), pi_js_compat::js_json::JsString> {
    let Some(drain) = active_session.drain_runtime_coordination() else {
        return Err("Child session cannot drain pending steering before completion"
            .to_owned()
            .into());
    };
    drain.await?;
    let current = input.store.get_agent(&input.running_agent.id);
    if current
        .as_ref()
        .is_some_and(|agent| agent.lifecycle == AgentLifecycle::SteeringPending)
    {
        return Err("Child session did not deliver pending steering before completion"
            .to_owned()
            .into());
    }
    Ok(())
}

async fn fail_child_session(
    input: &RunAgentSessionInput,
    error: &pi_js_compat::js_json::JsString,
    restore_generation: u64,
) -> Result<AgentSnapshot, JsString> {
    settle_descendants_before_terminalization(&input.store, &input.running_agent.id, None).await?;
    if let Some(cancelled) = acknowledge_cancelled_runtime(
        &input.store,
        &input.running_agent.id,
        Some(&input.reserved_runtime),
        Some(restore_generation),
    ) {
        return Ok(cancelled);
    }
    let current = input
        .store
        .get_agent(&input.running_agent.id)
        .unwrap_or_else(|| input.running_agent.clone());
    Ok(finalize_reserved_runtime(FinalizeReservedRuntimeInput {
        agent: current,
        expected_restore_generation: Some(restore_generation),
        error: Some(AgentError {
            message: error.to_owned(),
            code: None,
        }),
        result: None,
        reserved_runtime: Arc::clone(&input.reserved_runtime),
        store: Arc::clone(&input.store),
        terminal_lifecycle: "failed",
    }))
}

pub use super::descendant_settlement::wait_for_active_descendants;

async fn settle_descendants_before_terminalization(
    store: &Arc<MultiAgentStore>,
    agent_id: &JsString,
    signal: Option<&pi_js_compat::abort::AbortSignal>,
) -> Result<(), JsString> {
    super::control_runtime::request_direct_detached_runtime_cancellations(
        store,
        agent_id,
        &JsString::from("Owning subagent reached a terminal path"),
    )
    .map_err(|error| error.message)?;
    wait_for_active_descendants(store, agent_id, signal).await
}

fn finalize_reserved_runtime(input: FinalizeReservedRuntimeInput) -> AgentSnapshot {
    if let Some(expected) = input.expected_restore_generation
        && input.store.get_restore_generation() != expected
    {
        return input.agent;
    }
    if !is_active_lifecycle(input.agent.lifecycle) {
        return input.agent;
    }
    let finalized = input
        .reserved_runtime
        .coordinator
        .finalize_child(&FinalizeChildCommandInput {
            agent: input.agent.clone(),
            error: input.error,
            ownership: input.reserved_runtime.lifecycle.ownership.clone(),
            result: input.result,
            terminal_lifecycle: input.terminal_lifecycle.to_owned(),
        });
    match finalized {
        Ok(LifecycleCommandResult::Ok { agent }) => {
            let _ = publish_coordinator_snapshot(&input.store, &agent);
            *agent
        }
        _ => input
            .store
            .get_agent(&input.agent.id)
            .unwrap_or_else(|| input.agent.clone()),
    }
}

fn acknowledge_cancelled_runtime(
    store: &Arc<MultiAgentStore>,
    agent_id: &JsString,
    reserved_runtime: Option<&Arc<OwnedAgentRuntime>>,
    expected_restore_generation: Option<u64>,
) -> Option<AgentSnapshot> {
    let reserved_runtime = reserved_runtime?;
    if let Some(expected) = expected_restore_generation
        && store.get_restore_generation() != expected
    {
        return None;
    }
    let current = store.get_agent(agent_id)?;
    if current.lifecycle != AgentLifecycle::Cancelling {
        return None;
    }
    let acknowledged = reserved_runtime
        .coordinator
        .acknowledge_cancellation(&OwnedLifecycleCommandInput {
            agent: current.clone(),
            ownership: reserved_runtime.lifecycle.ownership.clone(),
        })
        .ok()?;
    match acknowledged {
        LifecycleCommandResult::Ok { agent } => {
            let _ = publish_coordinator_snapshot(store, &agent);
            Some(*agent)
        }
        LifecycleCommandResult::Err { .. } => Some(current),
    }
}

pub fn release_ownership_after_dispatch(
    dispatches: &ActiveAgentDispatches,
    ownerships: &Arc<Mutex<HashMap<JsString, Arc<OwnedAgentRuntime>>>>,
    agent_id: &JsString,
) {
    let dispatch = dispatches.lock().expect("dispatches").get(agent_id).cloned();
    if let Some(dispatch) = dispatch {
        let ownerships = Arc::clone(ownerships);
        let agent_id = agent_id.to_owned();
        tokio::spawn(async move {
            let _ = dispatch.await;
            ownerships.lock().expect("ownerships").remove(&agent_id);
        });
    }
}

fn notify_waiting_agent(
    message: &AgentMailboxMessage,
    desktop_notifier: &dyn DesktopNotifier,
) -> Option<Box<dyn DesktopNotificationHandle>> {
    if !is_waiting_for_input_notification(message) {
        return None;
    }
    let notification = DesktopNotification {
        body: message
            .body
            .clone()
            .unwrap_or_else(|| message.from_agent_id.clone() + " is waiting for input."),
        expire_time_ms: Some(PERSISTENT_DESKTOP_NOTIFICATION_EXPIRE_TIME_MS),
        title: "Pi agent needs input".into(),
        urgency: None,
    };
    // Frozen agents-core/src/runtime.ts:2840-2851 logs notifier failure and returns no handle.
    match desktop_notifier.notify(&notification) {
        Ok(handle) => handle,
        Err(error) => {
            crate::core::desktop_notification::report_callback_error(
                "Failed to send agent input-needed desktop notification:",
                &error,
            );
            None
        }
    }
}

fn remember_waiting_desktop_notification(
    store: &Arc<MultiAgentStore>,
    message: &AgentMailboxMessage,
    notification_handle: Option<Box<dyn DesktopNotificationHandle>>,
    waiting: &WaitingDesktopNotificationHandles,
) {
    let Some(notification_handle) = notification_handle else {
        return;
    };
    close_waiting_desktop_notification(&message.from_agent_id, waiting);
    let agent_id = message.from_agent_id.clone();
    let waiting_for_close = Arc::clone(waiting);
    let unsubscribe_transition = store.subscribe_agent_transitions(move |previous, current| {
        if agent_id != current.id || previous.lifecycle != AgentLifecycle::WaitingForInput {
            return;
        }
        if current.lifecycle != AgentLifecycle::WaitingForInput {
            close_waiting_desktop_notification(&current.id, &waiting_for_close);
        }
    });
    waiting.lock().expect("waiting notifications").insert(
        message.from_agent_id.clone(),
        WaitingDesktopNotificationRegistration {
            handle: notification_handle,
            unsubscribe_transition,
        },
    );
}

fn close_waiting_desktop_notification_when_not_waiting(
    store: &MultiAgentStore,
    agent_id: &JsString,
    waiting: &WaitingDesktopNotificationHandles,
) {
    if store
        .get_agent(agent_id)
        .is_some_and(|agent| agent.lifecycle == AgentLifecycle::WaitingForInput)
    {
        return;
    }
    close_waiting_desktop_notification(agent_id, waiting);
}

pub fn close_waiting_desktop_notification(
    agent_id: impl Into<pi_js_compat::js_json::JsString>,
    waiting: &WaitingDesktopNotificationHandles,
) {
    let registration = waiting.lock().expect("waiting notifications").remove(&agent_id.into());
    let Some(registration) = registration else {
        return;
    };
    drop(registration.unsubscribe_transition);
    // Frozen agents-core/src/runtime.ts:2894-2908 catches close errors after removing the registration.
    if let Err(error) = registration.handle.close() {
        crate::core::desktop_notification::report_callback_error(
            "Failed to close agent input-needed desktop notification:",
            &error,
        );
    }
}

fn is_waiting_for_input_notification(message: &AgentMailboxMessage) -> bool {
    message.kind == MailboxMessageKindValue::System
        && message.status == MailboxMessageStatusValue::Pending
        && message
            .thread_id
            .as_ref()
            .is_some_and(|thread_id| thread_id.starts_with("agent-waiting-for-input:"))
}

fn last_assistant_text(messages: &[pi_agent_core::AgentMessage]) -> Option<pi_js_compat::js_json::JsString> {
    for message in messages.iter().rev() {
        let Some(assistant) = message.as_assistant() else {
            continue;
        };
        let text = pi_js_compat::js_json::JsString::join(
            assistant
                .content
                .iter()
                .filter_map(|part| match part {
                    AssistantContent::Text(text) => Some(text.text.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            "\n",
        );
        if !text.is_empty() {
            return Some(text);
        }
    }
    None
}
