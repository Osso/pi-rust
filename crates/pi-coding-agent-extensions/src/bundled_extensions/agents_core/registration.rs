//! Agents-core tool and command registration
//! (`extensions/agents-core/src/registration.ts`).

use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::core::extensions::types::NotifyType;
use pi_ai::request::BoxFuture;
use serde_json::Value;

use crate::core::desktop_notification::DesktopNotifier;
use crate::core::extensions::types::{
    ExtensionApi, ExtensionCommandContext, HandlerError, SessionShutdownEvent, ToolDefinition,
};
use crate::core::multi_agent_store::{AgentSnapshot, MultiAgentStore};

use super::control_runtime::{CancelAgentParams, SteerAgentParams, cancel_agent, steer_agent};
use super::dispatch_runtime::{
    ActiveAgentDispatches, BackgroundSessionHandles, WaitingDesktopNotificationHandles,
    close_waiting_desktop_notification,
};
use super::lifecycle_runtime::{create_lifecycle_coordinator, publish_coordinator_snapshot};
use super::mailbox_runtime::RuntimeLifecycleMirror;
use super::runtime::{
    MultiAgentExtensionOptions, MultiAgentRuntimeHandles, NoopDesktopNotifier, assert_no_wait_agents_params,
    create_multi_agent_runtime_handles, format_agent_status, jobs_command, resolve_multi_agent_store, wait_agents,
};
use super::runtime_utils::abort_agent_handle_safely;
use super::session_factories::{
    background_prompt_validation_message as background_validation_message, is_production_child_session_factory,
    validate_goal_objective,
};
use super::shared::{
    AttachedSessionFactory, CHILD_ORCHESTRATION_UNAVAILABLE_MESSAGE, ChildAgentSession, ChildAgentSessionFactory,
    SpawnContext, is_child_agent_runtime,
};
use super::spawn_attach_runtime::{
    AttachSessionAgentParams, AttachSessionAgentRuntimeInput, RecoverAgentsInput, SpawnAgentRuntimeInput,
    attach_session_agent, attach_session_agent_schema, recover_agents, spawn_agent, spawn_agent_schema,
};
use super::viewer::{ListAgentsParams, list_agents};

#[derive(Clone)]
pub struct BackgroundDispatchContext {
    pub create_child_session: Option<ChildAgentSessionFactory>,
    pub dispatches: ActiveAgentDispatches,
    pub handles: BackgroundSessionHandles,
    pub ownerships: Arc<Mutex<HashMap<JsString, Arc<super::control_runtime::OwnedAgentRuntime>>>>,
    pub store: Arc<MultiAgentStore>,
}

pub struct AgentsCoreRegistrationContext {
    pub active_dispatches: ActiveAgentDispatches,
    pub background_dispatch: BackgroundDispatchContext,
    pub background_sessions: BackgroundSessionHandles,
    pub create_attached_session: Option<AttachedSessionFactory>,
    pub create_child_session: Option<ChildAgentSessionFactory>,
    pub desktop_notifier: Arc<dyn DesktopNotifier>,
    pub ownerships: Arc<Mutex<HashMap<JsString, Arc<super::control_runtime::OwnedAgentRuntime>>>>,
    pub runtime_handles: MultiAgentRuntimeHandles,
    pub runtime_lifecycle_mirror: RuntimeLifecycleMirror,
    pub store: Arc<MultiAgentStore>,
    pub waiting_desktop_notifications: WaitingDesktopNotificationHandles,
}

fn create_registration_context(options: &MultiAgentExtensionOptions) -> AgentsCoreRegistrationContext {
    let store = resolve_multi_agent_store(options);
    let runtime_handles = options
        .runtime_handles
        .clone()
        .unwrap_or_else(create_multi_agent_runtime_handles);
    let background_sessions = Arc::clone(&runtime_handles.sessions);
    let active_dispatches = Arc::clone(&runtime_handles.dispatches);
    let ownerships = Arc::clone(&runtime_handles.ownerships);
    AgentsCoreRegistrationContext {
        active_dispatches: Arc::clone(&active_dispatches),
        background_dispatch: BackgroundDispatchContext {
            create_child_session: options.create_child_session.clone(),
            dispatches: active_dispatches,
            handles: Arc::clone(&background_sessions),
            ownerships: Arc::clone(&ownerships),
            store: Arc::clone(&store),
        },
        background_sessions,
        create_attached_session: options.create_attached_session.clone(),
        create_child_session: options.create_child_session.clone(),
        desktop_notifier: options
            .desktop_notifier
            .clone()
            .unwrap_or_else(|| Arc::new(NoopDesktopNotifier)),
        ownerships,
        runtime_lifecycle_mirror: RuntimeLifecycleMirror::new(Arc::clone(&store)),
        runtime_handles,
        store,
        waiting_desktop_notifications: Arc::new(Mutex::new(HashMap::new())),
    }
}

fn register_session_start_hook(
    pi: &ExtensionApi,
    registration: &AgentsCoreRegistrationContext,
) -> Result<(), HandlerError> {
    let model_request_started = registration.runtime_handles.latest_model_request_started_at.clone();
    let registration = registration.clone_for_hooks();
    pi.on_session_start(move |_event, ctx| {
        let registration = registration.clone();
        let model_request_started = model_request_started.clone();
        let future: crate::core::extensions::types::BoxFuture<'static, Result<Option<()>, HandlerError>> =
            Box::pin(async move {
                if !is_child_agent_runtime(&ctx)? {
                    *model_request_started.lock().expect("model request start") = None;
                }
                if !is_child_agent_runtime(&ctx)? {
                    registration.runtime_lifecycle_mirror.bind(&ctx)?;
                }
                recover_agents(&RecoverAgentsInput {
                    create_attached_session: registration.create_attached_session.clone(),
                    ctx: ctx.clone(),
                    desktop_notifier: Arc::clone(&registration.desktop_notifier),
                    dispatches: Arc::clone(&registration.active_dispatches),
                    handles: Some(Arc::clone(&registration.background_sessions)),
                    ownerships: Arc::clone(&registration.ownerships),
                    store: Arc::clone(&registration.store),
                    waiting_desktop_notifications: Arc::clone(&registration.waiting_desktop_notifications),
                })?;
                Ok(None)
            });
        future
    })
    .map_err(HandlerError::from)?;
    Ok(())
}

fn register_session_shutdown_hook(
    pi: &ExtensionApi,
    registration: &AgentsCoreRegistrationContext,
) -> Result<(), HandlerError> {
    let registration = registration.clone_for_hooks();
    pi.on_session_shutdown(move |event: SessionShutdownEvent, ctx| {
        let registration = registration.clone();
        let future: crate::core::extensions::types::BoxFuture<'static, Result<Option<()>, HandlerError>> =
            Box::pin(async move {
                registration.runtime_lifecycle_mirror.dispose();
                if event.reason == crate::core::extensions::types::SessionShutdownReason::Reload
                    || is_child_agent_runtime(&ctx)?
                {
                    return Ok(None);
                }
                registration.store.invalidate_in_flight_dispatches();
                for runtime in registration.ownerships.lock().expect("ownerships").values() {
                    runtime.abort_controller.abort(None);
                }
                let background_ids: Vec<JsString> = registration
                    .background_sessions
                    .lock()
                    .expect("sessions")
                    .keys()
                    .cloned()
                    .collect();
                for agent_id in background_ids {
                    if !registration
                        .ownerships
                        .lock()
                        .expect("ownerships")
                        .contains_key(&agent_id)
                    {
                        abort_agent_handle_safely(&registration.store, &agent_id);
                    }
                }
                let waiting_ids: Vec<pi_js_compat::js_json::JsString> = registration
                    .waiting_desktop_notifications
                    .lock()
                    .expect("waiting notifications")
                    .keys()
                    .cloned()
                    .collect();
                for agent_id in waiting_ids {
                    close_waiting_desktop_notification(&agent_id, &registration.waiting_desktop_notifications);
                }
                registration.background_sessions.lock().expect("sessions").clear();
                registration.active_dispatches.lock().expect("dispatches").clear();
                Ok(None)
            });
        future
    })
    .map_err(HandlerError::from)?;
    Ok(())
}

impl AgentsCoreRegistrationContext {
    fn clone_for_hooks(&self) -> Arc<HookRegistration> {
        Arc::new(HookRegistration {
            active_dispatches: Arc::clone(&self.active_dispatches),
            background_sessions: Arc::clone(&self.background_sessions),
            create_attached_session: self.create_attached_session.clone(),
            desktop_notifier: Arc::clone(&self.desktop_notifier),
            ownerships: Arc::clone(&self.ownerships),
            runtime_lifecycle_mirror: self.runtime_lifecycle_mirror.clone(),
            store: Arc::clone(&self.store),
            waiting_desktop_notifications: Arc::clone(&self.waiting_desktop_notifications),
        })
    }
}

struct HookRegistration {
    active_dispatches: ActiveAgentDispatches,
    background_sessions: BackgroundSessionHandles,
    create_attached_session: Option<AttachedSessionFactory>,
    desktop_notifier: Arc<dyn DesktopNotifier>,
    ownerships: Arc<Mutex<HashMap<JsString, Arc<super::control_runtime::OwnedAgentRuntime>>>>,
    runtime_lifecycle_mirror: RuntimeLifecycleMirror,
    store: Arc<MultiAgentStore>,
    waiting_desktop_notifications: WaitingDesktopNotificationHandles,
}

fn register_background_command(
    pi: &ExtensionApi,
    registration: &AgentsCoreRegistrationContext,
) -> Result<(), HandlerError> {
    let background = registration.background_dispatch.clone();
    let mirror = registration.runtime_lifecycle_mirror.clone();
    pi.register_command(
        "bg",
        Some("Run a prompt as a background agent job.".into()),
        None,
        Arc::new(
            move |args: pi_js_compat::js_json::JsString, ctx: ExtensionCommandContext| {
                let background = background.clone();
                let mirror = mirror.clone();
                Box::pin(async move {
                    mirror.bind(&ctx.context)?;
                    background_command(&background, &args, &ctx).await
                })
            },
        ),
    )
    .map_err(HandlerError::from)?;
    Ok(())
}

fn register_jobs_command(pi: &ExtensionApi, registration: &AgentsCoreRegistrationContext) -> Result<(), HandlerError> {
    let store = Arc::clone(&registration.store);
    pi.register_command(
        "jobs",
        Some("List background agent jobs.".into()),
        None,
        Arc::new(
            move |_args: pi_js_compat::js_json::JsString, ctx: ExtensionCommandContext| {
                let store = Arc::clone(&store);
                Box::pin(async move { jobs_command(&store, &ctx) })
            },
        ),
    )
    .map_err(HandlerError::from)?;
    Ok(())
}

pub(super) fn tool_definition(
    name: &str,
    label: &str,
    description: &str,
    prompt_guidelines: Option<Vec<JsString>>,
    parameters: Value,
    execute: crate::core::extensions::types::ToolExecute,
) -> ToolDefinition {
    ToolDefinition {
        name: name.to_owned(),
        label: label.into(),
        description: description.into(),
        prompt_snippet: None,
        prompt_guidelines,
        approval_required: Some(false),
        approval_kind: None,
        parameters,
        render_shell: None,
        prepare_arguments: None,
        prepare_execution: None,
        execution_mode: None,
        execute,
        render_call: None,
        render_result: None,
    }
}

fn register_spawn_agent_tool(
    pi: &ExtensionApi,
    registration: &AgentsCoreRegistrationContext,
) -> Result<(), HandlerError> {
    let store = Arc::clone(&registration.store);
    let create_child_session = registration.create_child_session.clone();
    let dispatches = Arc::clone(&registration.active_dispatches);
    let ownerships = Arc::clone(&registration.ownerships);
    let desktop_notifier = Arc::clone(&registration.desktop_notifier);
    let waiting = Arc::clone(&registration.waiting_desktop_notifications);
    let handles = Arc::clone(&registration.background_sessions);
    let mirror = registration.runtime_lifecycle_mirror.clone();
    pi.register_tool(tool_definition(
        "spawn_agent",
        "Spawn Agent",
        "Create a child agent record and optionally dispatch it through the multi-agent runtime.",
        Some(vec![
            "For read-only codebase research or exploration, prefer spawn_agent with agentType \"explore\".".into(),
            "Context defaults to \"inherit\"; choose \"fresh\" explicitly for isolated work, verification, or falsification. Reviewer profiles default to fresh context.".into(),
            "For scoped code changes, use agentType \"implement\"; for proof commands before completion, use agentType \"verifier\"; for final code review or second opinions, use agentType \"reviewer\".".into(),
        ]),
        spawn_agent_schema(),
        Arc::new(move |tool_call_id, params, _signal, _on_update, ctx| {
            let store = Arc::clone(&store);
            let create_child_session = create_child_session.clone();
            let dispatches = Arc::clone(&dispatches);
            let ownerships = Arc::clone(&ownerships);
            let desktop_notifier = Arc::clone(&desktop_notifier);
            let waiting = Arc::clone(&waiting);
            let handles = Some(Arc::clone(&handles));
            let mirror = mirror.clone();
            Box::pin(async move {
            let ctx = crate::core::extensions::types::require_extension_context(ctx)?;
                mirror.bind(&ctx)?;
                spawn_agent(SpawnAgentRuntimeInput {
                    active_tool_call_id: Some(tool_call_id),
                    create_child_session,
                    ctx,
                    desktop_notifier,
                    dispatches,
                    handles,
                    ownerships,
                    params,
                    store,
                    waiting_desktop_notifications: waiting,
                    _marker: std::marker::PhantomData,
                })
                .await
            })
        }),
    )).map_err(HandlerError::from)?;
    Ok(())
}

fn register_list_agents_tool(
    pi: &ExtensionApi,
    registration: &AgentsCoreRegistrationContext,
) -> Result<(), HandlerError> {
    let store = Arc::clone(&registration.store);
    pi.register_tool(tool_definition(
        "list_agents",
        "List Agents",
        "List agents from the authoritative multi-agent store.",
        None,
        list_agents_schema(),
        Arc::new(move |_tool_call_id, params, _signal, _on_update, _ctx| {
            let store = Arc::clone(&store);
            Box::pin(async move {
                let params: ListAgentsParams =
                    pi_js_compat::js_json::from_value(&params).map_err(|error| HandlerError::new(error.to_string()))?;
                list_agents(&store, &params)
            })
        }),
    ))
    .map_err(HandlerError::from)?;
    Ok(())
}

fn register_attach_session_agent_tool(
    pi: &ExtensionApi,
    registration: &AgentsCoreRegistrationContext,
) -> Result<(), HandlerError> {
    let store = Arc::clone(&registration.store);
    let create_attached_session = registration.create_attached_session.clone();
    let desktop_notifier = Arc::clone(&registration.desktop_notifier);
    let dispatches = Arc::clone(&registration.active_dispatches);
    let handles = Arc::clone(&registration.background_sessions);
    let ownerships = Arc::clone(&registration.ownerships);
    let waiting = Arc::clone(&registration.waiting_desktop_notifications);
    let mirror = registration.runtime_lifecycle_mirror.clone();
    pi.register_tool(tool_definition(
        "attach_session_agent",
        "Attach Session Agent",
        "Attach or resume an existing saved session as an agent without changing its session ID.",
        None,
        attach_session_agent_schema(),
        Arc::new(move |_tool_call_id, params, _signal, _on_update, ctx| {
            let store = Arc::clone(&store);
            let create_attached_session = create_attached_session.clone();
            let desktop_notifier = Arc::clone(&desktop_notifier);
            let dispatches = Arc::clone(&dispatches);
            let handles = Some(Arc::clone(&handles));
            let ownerships = Arc::clone(&ownerships);
            let waiting = Arc::clone(&waiting);
            let mirror = mirror.clone();
            Box::pin(async move {
                let ctx = crate::core::extensions::types::require_extension_context(ctx)?;
                mirror.bind(&ctx)?;
                let params: AttachSessionAgentParams =
                    pi_js_compat::js_json::from_value(&params).map_err(|error| HandlerError::new(error.to_string()))?;
                attach_session_agent(&AttachSessionAgentRuntimeInput {
                    create_attached_session,
                    ctx,
                    desktop_notifier,
                    dispatches,
                    handles,
                    params,
                    ownerships,
                    store,
                    waiting_desktop_notifications: waiting,
                })
                .await
            })
        }),
    ))
    .map_err(HandlerError::from)?;
    Ok(())
}

fn register_wait_agents_tool(
    pi: &ExtensionApi,
    registration: &AgentsCoreRegistrationContext,
) -> Result<(), HandlerError> {
    let store = Arc::clone(&registration.store);
    let runtime_handles = registration.runtime_handles.clone();
    let mirror = registration.runtime_lifecycle_mirror.clone();
    pi.register_tool(tool_definition(
        "wait_agent",
        "Wait Agent",
        "Wait until an active agent terminates, steering wakes the wait, persisted coordination polling finds input, or 25 minutes elapse since the latest model request started. Timeout leaves agents running. Terminal wakes consume pending terminal notifications.",
        None,
        wait_agents_schema(),
        Arc::new(move |_tool_call_id, params, signal, _on_update, ctx| {
            let store = Arc::clone(&store);
            let runtime_handles = runtime_handles.clone();
            let mirror = mirror.clone();
            Box::pin(async move {
            let ctx = crate::core::extensions::types::require_extension_context(ctx)?;
                mirror.bind(&ctx)?;
                assert_no_wait_agents_params(&params, "wait_agent")?;
                wait_agents(&store, &runtime_handles, signal, Some(&ctx)).await
            })
        }),
    )).map_err(HandlerError::from)?;
    Ok(())
}

fn register_cancel_agent_tool(
    pi: &ExtensionApi,
    registration: &AgentsCoreRegistrationContext,
) -> Result<(), HandlerError> {
    let store = Arc::clone(&registration.store);
    let runtime_handles = registration.runtime_handles.clone();
    pi.register_tool(tool_definition(
        "close_agent",
        "Close Agent",
        "Close an agent through the multi-agent store using the current store revision.",
        None,
        cancel_agent_schema(),
        Arc::new(move |_tool_call_id, params, _signal, _on_update, _ctx| {
            let store = Arc::clone(&store);
            let runtime_handles = runtime_handles.clone();
            Box::pin(async move {
                let params: CancelAgentParams =
                    pi_js_compat::js_json::from_value(&params).map_err(|error| HandlerError::new(error.to_string()))?;
                cancel_agent(&store, &runtime_handles, &params).await
            })
        }),
    ))
    .map_err(HandlerError::from)?;
    Ok(())
}

fn register_steer_agent_tool(
    pi: &ExtensionApi,
    registration: &AgentsCoreRegistrationContext,
) -> Result<(), HandlerError> {
    let store = Arc::clone(&registration.store);
    let runtime_handles = registration.runtime_handles.clone();
    pi.register_tool(tool_definition(
        "steer_agent",
        "Steer Agent",
        "Queue a steering message through the multi-agent mailbox.",
        None,
        steer_agent_schema(),
        Arc::new(move |_tool_call_id, params, _signal, _on_update, ctx| {
            let store = Arc::clone(&store);
            let runtime_handles = runtime_handles.clone();
            Box::pin(async move {
                let params: SteerAgentParams =
                    pi_js_compat::js_json::from_value(&params).map_err(|error| HandlerError::new(error.to_string()))?;
                steer_agent(&store, &runtime_handles, &params, ctx.as_ref())
            })
        }),
    ))
    .map_err(HandlerError::from)?;
    Ok(())
}

fn register_wait_slice_hooks(
    pi: &ExtensionApi,
    registration: &AgentsCoreRegistrationContext,
) -> Result<(), HandlerError> {
    let started = registration.runtime_handles.latest_model_request_started_at.clone();
    pi.on_model_request_start(move |_event, ctx| {
        let started = started.clone();
        Box::pin(async move {
            if !is_child_agent_runtime(&ctx)? {
                *started.lock().expect("model request start") = Some(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .expect("system clock")
                        .as_millis() as i64,
                );
            }
            Ok(None)
        }) as crate::core::extensions::types::BoxFuture<'static, Result<Option<()>, HandlerError>>
    })
    .map_err(HandlerError::from)?;
    let started = registration.runtime_handles.latest_model_request_started_at.clone();
    pi.on_agent_end(move |_event, ctx| {
        let started = started.clone();
        Box::pin(async move {
            if !is_child_agent_runtime(&ctx)? {
                *started.lock().expect("model request start") = None;
            }
            Ok(None)
        }) as crate::core::extensions::types::BoxFuture<'static, Result<Option<()>, HandlerError>>
    })
    .map_err(HandlerError::from)?;
    Ok(())
}

pub fn register_agents_core_tools(pi: &ExtensionApi, options: &MultiAgentExtensionOptions) -> Result<(), HandlerError> {
    let registration = create_registration_context(options);
    register_wait_slice_hooks(pi, &registration)?;
    register_session_start_hook(pi, &registration)?;
    register_session_shutdown_hook(pi, &registration)?;
    register_background_command(pi, &registration)?;
    register_jobs_command(pi, &registration)?;
    register_spawn_agent_tool(pi, &registration)?;
    super::agent_model_tool::register_set_agent_model_tool(
        pi,
        Arc::clone(&registration.store),
        registration.runtime_handles.clone(),
    )?;
    register_list_agents_tool(pi, &registration)?;
    register_attach_session_agent_tool(pi, &registration)?;
    register_wait_agents_tool(pi, &registration)?;
    register_cancel_agent_tool(pi, &registration)?;
    register_steer_agent_tool(pi, &registration)?;
    Ok(())
}

async fn background_command(
    background: &BackgroundDispatchContext,
    args: &pi_js_compat::js_json::JsString,
    ctx: &ExtensionCommandContext,
) -> Result<(), HandlerError> {
    let Some(prompt) = validate_and_notify_background_command(background, args, ctx)? else {
        return Ok(());
    };
    let Some(coordinator) = create_lifecycle_coordinator(&background.store) else {
        notify(
            ctx,
            "Background jobs require a persisted supervisor session.",
            NotifyType::Error,
        )?;
        return Ok(());
    };
    let Some(prepared_runtime) = prepare_background_runtime(background, &coordinator, &prompt, ctx).await? else {
        return Ok(());
    };
    let session_id = ctx.session_manager()?.get_session_id().to_owned();
    let created = coordinator
        .commit_running_child(
            &prepared_runtime.prepared,
            &session_id,
            super::lifecycle_runtime::runtime_process_identity(),
            None,
        )
        .map_err(|error| HandlerError::new(error.message()))?;
    let (agent, ownership) = match created {
        crate::core::session_control_db::CreateMultiAgentChildWithRuntimeOwnershipResult::Ok { agent, ownership } => {
            (*agent, *ownership)
        }
        crate::core::session_control_db::CreateMultiAgentChildWithRuntimeOwnershipResult::Err { error } => {
            prepared_runtime
                .child_session
                .dispose()
                .await
                .map_err(HandlerError::new)?;
            notify(
                ctx,
                &format!("Could not create background job: {}", error.as_str()),
                NotifyType::Error,
            )?;
            return Ok(());
        }
    };
    background
        .store
        .publish_lifecycle_coordinator_snapshot(&agent)
        .map_err(|error| HandlerError::new(error.message))?;
    let runtime = Arc::new(super::control_runtime::OwnedAgentRuntime {
        abort_controller: prepared_runtime.abort_controller,
        coordinator: Arc::clone(&coordinator),
        lifecycle: crate::core::lifecycle_coordinator::OwnedLifecycleCommandInput {
            agent: agent.clone(),
            ownership,
        },
    });
    background
        .ownerships
        .lock()
        .expect("ownerships")
        .insert(agent.id.clone(), Arc::clone(&runtime));
    let agent = start_background_dispatch(background, runtime, &prompt, ctx, Some(prepared_runtime.child_session));
    ctx.ui()?.set_editor_text(&JsString::default())?;
    notify(
        ctx,
        &js_format!(
            "Background job {} started. Use /jobs to inspect it or wait_agent to wait for any completion.",
            agent.id
        ),
        NotifyType::Info,
    )?;
    Ok(())
}

struct PreparedBackgroundRuntime {
    abort_controller: pi_js_compat::abort::AbortController,
    child_session: Arc<dyn ChildAgentSession>,
    prepared: AgentSnapshot,
}

fn validate_and_notify_background_command(
    background: &BackgroundDispatchContext,
    args: &pi_js_compat::js_json::JsString,
    ctx: &ExtensionCommandContext,
) -> Result<Option<pi_js_compat::js_json::JsString>, HandlerError> {
    if is_child_agent_runtime(&ctx.context)? {
        notify(ctx, CHILD_ORCHESTRATION_UNAVAILABLE_MESSAGE, NotifyType::Error)?;
        return Ok(None);
    }
    let Some(create_child_session) = background.create_child_session.as_ref() else {
        notify(
            ctx,
            "Background jobs are unavailable: no child session runtime is configured.",
            NotifyType::Error,
        )?;
        return Ok(None);
    };
    let prompt = args.trim();
    if !is_production_child_session_factory(create_child_session) {
        return Ok(Some(prompt));
    }
    let validation = validate_goal_objective(&prompt);
    if validation.ok {
        return Ok(Some(prompt));
    }
    notify(ctx, &background_validation_message(&validation), NotifyType::Error)?;
    Ok(None)
}

async fn prepare_background_runtime(
    background: &BackgroundDispatchContext,
    coordinator: &Arc<crate::core::lifecycle_coordinator::LifecycleCoordinator>,
    prompt: &pi_js_compat::js_json::JsString,
    ctx: &ExtensionCommandContext,
) -> Result<Option<PreparedBackgroundRuntime>, HandlerError> {
    let Some(create_child_session) = background.create_child_session.as_ref() else {
        return Ok(None);
    };
    let mut prepared = coordinator
        .prepare_child(&crate::core::lifecycle_coordinator::PrepareChildCommandInput {
            spawn: crate::core::multi_agent_store::SpawnAgent {
                parent_id: None,
                display_name: "Background Job".into(),
                agent_type: "background".into(),
                origin: None,
                cwd: ctx.cwd()?,
                permission: crate::core::multi_agent_store::AgentPermission {
                    policy: "on-request".into(),
                    inherited_from: None,
                    narrowed: true,
                },
                worktree: None,
                model: None,
                account: None,
                slot: None,
                transcript: None,
                event_stream: None,
                worker: None,
            },
            agent_id: None,
            detached: None,
            result: None,
        })
        .map_err(|error| HandlerError::new(error.message()))?;
    let abort_controller = pi_js_compat::abort::AbortController::new();
    match create_child_session
        .call(super::shared::ChildAgentDispatchInput {
            active_tool_call_id: None,
            agent: prepared.clone(),
            context: SpawnContext::Fresh,
            ctx: ctx.context.clone(),
            prompt: prompt.to_owned().into(),
            signal: Some(abort_controller.signal()),
        })
        .await
    {
        Ok(child_session) => {
            if let Some(transcript) = child_session.transcript() {
                prepared.transcript = Some(transcript);
            }
            Ok(Some(PreparedBackgroundRuntime {
                abort_controller,
                child_session,
                prepared,
            }))
        }
        Err(error) => {
            let runtime_error = crate::core::multi_agent_store::AgentError {
                code: Some("runtime_spawn_failed".to_owned()),
                message: error.clone(),
            };
            let failed = coordinator
                .commit_failed_child(&prepared, runtime_error)
                .map_err(|error| HandlerError::new(error.message()))?;
            if let crate::core::session_control_db::CreateFailedMultiAgentChildResult::Ok { agent } = failed {
                publish_coordinator_snapshot(&background.store, &agent).map_err(HandlerError::from)?;
            }
            notify(
                ctx,
                pi_js_compat::js_json::JsString::from("Could not create background job: ") + &error,
                NotifyType::Error,
            )?;
            Ok(None)
        }
    }
}

fn start_background_dispatch(
    background: &BackgroundDispatchContext,
    runtime: Arc<super::control_runtime::OwnedAgentRuntime>,
    prompt: &pi_js_compat::js_json::JsString,
    ctx: &ExtensionCommandContext,
    created_session: Option<Arc<dyn ChildAgentSession>>,
) -> AgentSnapshot {
    let agent = runtime.lifecycle.agent.clone();
    let Some(create_child_session) = background.create_child_session.clone() else {
        return agent;
    };
    let promise = track_background_agent_dispatch(
        background,
        create_child_session,
        Arc::clone(&runtime),
        prompt,
        ctx,
        created_session,
    );
    super::dispatch_runtime::release_ownership_after_dispatch(
        &background.dispatches,
        &background.ownerships,
        &agent.id,
    );
    notify_background_dispatch(promise, &background.handles, ctx);
    background.store.get_agent(&agent.id).unwrap_or(agent)
}

fn notify_background_dispatch(
    promise: BoxFuture<'static, Result<AgentSnapshot, pi_js_compat::js_json::JsString>>,
    handles: &BackgroundSessionHandles,
    ctx: &ExtensionCommandContext,
) {
    let handles = Arc::clone(handles);
    let ctx = ctx.clone();
    tokio::spawn(async move {
        let agent = promise.await.unwrap_or_else(|error| {
            crate::utils::uncaught::throw_callback_uncaught(pi_js_compat::callback_error::CallbackError::new(error))
        });
        handles.lock().expect("sessions").remove(&agent.id);
        // Frozen runtime.ts:412-422 has no catch on the notification's promise continuation.
        let ui = ctx
            .ui()
            .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error.into()));
        ui.notify(
            &format_agent_status(&agent),
            Some(background_notification_level(&agent)),
        )
        .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error));
    });
}

fn background_notification_level(agent: &AgentSnapshot) -> NotifyType {
    match agent.lifecycle {
        crate::core::multi_agent_store::AgentLifecycle::Completed => NotifyType::Info,
        crate::core::multi_agent_store::AgentLifecycle::Failed => NotifyType::Error,
        _ => NotifyType::Warning,
    }
}

fn track_background_agent_dispatch(
    background: &BackgroundDispatchContext,
    create_child_session: ChildAgentSessionFactory,
    runtime: Arc<super::control_runtime::OwnedAgentRuntime>,
    prompt: &pi_js_compat::js_json::JsString,
    ctx: &ExtensionCommandContext,
    created_session: Option<Arc<dyn ChildAgentSession>>,
) -> BoxFuture<'static, Result<AgentSnapshot, pi_js_compat::js_json::JsString>> {
    let agent = runtime.lifecycle.agent.clone();
    super::dispatch_runtime::track_agent_dispatch(super::dispatch_runtime::TrackAgentDispatchInput {
        agent: agent.clone(),
        dispatch: super::dispatch_runtime::run_agent_session(super::dispatch_runtime::RunAgentSessionInput {
            create_child_session,
            created_session,
            ctx: ctx.context.clone(),
            expected_restore_generation: None,
            handles: Some(Arc::clone(&background.handles)),
            prompt: prompt.to_owned().into(),
            reserved_runtime: Arc::clone(&runtime),
            running_agent: agent,
            store: Arc::clone(&background.store),
        }),
        dispatches: Arc::clone(&background.dispatches),
        handles: Some(Arc::clone(&background.handles)),
        reserved_runtime: runtime,
        store: Arc::clone(&background.store),
    })
}

fn notify(
    ctx: &ExtensionCommandContext,
    message: impl Into<pi_js_compat::js_json::JsString>,
    kind: NotifyType,
) -> Result<(), HandlerError> {
    ctx.ui()?.notify(&message.into(), Some(kind))?;
    Ok(())
}

fn cancel_agent_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "required": ["agentId"],
        "properties": {
            "agentId": {"type": "string"},
            "reason": {"type": "string"}
        }
    })
}

fn steer_agent_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "required": ["agentId", "message"],
        "properties": {
            "agentId": {"type": "string"},
            "fileRefs": {
                "type": "array",
                "items": super::shared::file_reference_schema()
            },
            "message": {"type": "string"},
            "fromAgentId": {"type": "string"},
            "targetCheckpoint": {
                "anyOf": [
                    {"type": "string", "const": "next_model_call"},
                    {"type": "string", "const": "after_tool_result"},
                    {"type": "string", "const": "when_waiting"}
                ]
            }
        }
    })
}

fn list_agents_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "parentId": {"type": "string"}
        },
        "additionalProperties": false
    })
}

fn wait_agents_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {},
        "additionalProperties": false
    })
}
