//! Agent spawn, attach, and recovery
//! (`extensions/agents-core/src/spawn-attach-runtime.ts`).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use pi_agent_core::AgentToolResult;
use pi_js_compat::{
    js_format, js_json as json,
    js_json::{JsString, JsValue as Value},
};
use serde::Deserialize;

use crate::core::desktop_notification::DesktopNotifier;
use crate::core::extensions::types::{ExtensionContext, HandlerError};
use crate::core::lifecycle_coordinator::{OwnedLifecycleCommandInput, PrepareChildCommandInput};
use crate::core::model_resolver::find_exact_model_reference_match;
use crate::core::multi_agent_store::{
    AgentError, AgentLifecycle, AgentModel, AgentPermission, AgentSnapshot, MultiAgentStore, SpawnAgent,
};
use crate::core::runtime_process::is_process_identity_alive;
use crate::core::session_control_db::{CreateMultiAgentAttachmentResult, read_multi_agent_runtime_ownership};
use crate::core::session_manager::{SessionInfo, SessionManager};

use super::control_runtime::OwnedAgentRuntime;
use super::dispatch_runtime::{
    ActiveAgentDispatches, BackgroundSessionHandles, StartToolDispatchInput, WaitingDesktopNotificationHandles,
    dispatch_attached_session_agent, release_ownership_after_dispatch, run_agent_session, start_tool_dispatch,
};
use super::lifecycle_runtime::{create_lifecycle_coordinator, publish_coordinator_snapshot, runtime_process_identity};
use super::parent_agent_journal::read_active_parent_agent_ids as read_journal_ids;
use super::session_factories::{
    is_production_child_session_factory, spawn_prompt_validation_message, validate_goal_objective,
};
use super::shared::{
    AttachedSessionFactory, CHILD_ORCHESTRATION_UNAVAILABLE_MESSAGE, ChildAgentDispatchInput, ChildAgentSession,
    ChildAgentSessionFactory, SpawnContext, empty_agent, is_child_agent_runtime, is_in_flight_lifecycle,
};
use super::tool_results::tool_result;

const CRASH_RECOVERY_PROMPT: &str = "Continue the conversation from where it left off without asking the user any further questions. Resume directly from the saved session context.";

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpawnAgentParams {
    pub agent_type: Option<JsString>,
    pub context: Option<String>,
    pub display_name: Option<JsString>,
    pub parent_id: Option<JsString>,
    pub prompt: JsString,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachSessionAgentParams {
    pub agent_type: Option<JsString>,
    pub display_name: Option<JsString>,
    pub name: Option<JsString>,
    pub parent_id: Option<JsString>,
    pub path: Option<JsString>,
    pub prompt: Option<JsString>,
    pub session_id: Option<JsString>,
}

pub fn spawn_agent_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "required": ["prompt"],
        "properties": {
            "agentType": {"type": "string"},
            "context": {
                "anyOf": [
                    {"type": "string", "const": "fresh"},
                    {"type": "string", "const": "inherit"}
                ]
            },
            "displayName": {"type": "string", "description": "Short task slug shown in the agent status list, e.g. fix-footer-count (max ~24 chars)."},
            "parentId": {"type": "string"},
            "prompt": {"type": "string"}
        }
    })
}

pub fn attach_session_agent_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "agentType": {"type": "string"},
            "displayName": {"type": "string"},
            "name": {"type": "string"},
            "parentId": {"type": "string"},
            "path": {"type": "string"},
            "prompt": {"type": "string"},
            "sessionId": {"type": "string"}
        }
    })
}

#[derive(Clone)]
pub struct SpawnAgentRuntimeInput<'a> {
    pub active_tool_call_id: Option<String>,
    pub create_child_session: Option<ChildAgentSessionFactory>,
    pub ctx: ExtensionContext,
    pub desktop_notifier: Arc<dyn DesktopNotifier>,
    pub dispatches: ActiveAgentDispatches,
    pub handles: Option<BackgroundSessionHandles>,
    pub ownerships: Arc<Mutex<HashMap<JsString, Arc<OwnedAgentRuntime>>>>,
    pub params: Value,
    pub store: Arc<MultiAgentStore>,
    pub waiting_desktop_notifications: WaitingDesktopNotificationHandles,
    pub _marker: std::marker::PhantomData<&'a ()>,
}

struct ValidSpawnInput {
    active_tool_call_id: Option<String>,
    create_child_session: ChildAgentSessionFactory,
    ctx: ExtensionContext,
    desktop_notifier: Arc<dyn DesktopNotifier>,
    dispatches: ActiveAgentDispatches,
    handles: Option<BackgroundSessionHandles>,
    ownerships: Arc<Mutex<HashMap<JsString, Arc<OwnedAgentRuntime>>>>,
    params: SpawnAgentParams,
    store: Arc<MultiAgentStore>,
    waiting_desktop_notifications: WaitingDesktopNotificationHandles,
}

struct PreparedSpawn {
    abort_controller: pi_js_compat::abort::AbortController,
    context: SpawnContext,
    coordinator: Arc<crate::core::lifecycle_coordinator::LifecycleCoordinator>,
    prepared: AgentSnapshot,
}

struct ConstructedSpawn {
    prepared: PreparedSpawn,
    child_session: Arc<dyn ChildAgentSession>,
}

struct CommittedSpawn {
    spawn: ConstructedSpawn,
    runtime: Arc<OwnedAgentRuntime>,
    running_agent: AgentSnapshot,
}

#[derive(Clone, Default)]
pub struct ResolvedAgentProfile {
    pub context: Option<SpawnContext>,
    pub model: Option<pi_ai::types::Model>,
    pub model_metadata: Option<AgentModel>,
}

pub async fn spawn_agent(input: SpawnAgentRuntimeInput<'_>) -> Result<AgentToolResult, HandlerError> {
    let valid = match validate_spawn_input(input)? {
        SpawnValidation::Valid(valid) => valid,
        SpawnValidation::Error(result) => return Ok(*result),
    };
    let prepared = match prepare_spawn(&valid)? {
        PrepareResult::Prepared(prepared) => prepared,
        PrepareResult::Error(result) => return Ok(*result),
    };
    let constructed = match construct_child_session(&valid, *prepared).await {
        Ok(constructed) => constructed,
        Err(result) => return Ok(*result),
    };
    let session_manager = valid.ctx.session_manager()?;
    let owner_session_id = session_manager.get_session_id().to_owned();
    let committed = match commit_running_child(&valid, constructed, &owner_session_id) {
        Ok(committed) => committed,
        Err(result) => return Ok(*result),
    };
    input_store_ownership(&valid, &committed);
    dispatch_spawned_agent(&valid, committed)
}

enum SpawnValidation {
    Valid(Box<ValidSpawnInput>),
    Error(Box<AgentToolResult>),
}

fn validate_spawn_input(input: SpawnAgentRuntimeInput<'_>) -> Result<SpawnValidation, HandlerError> {
    let params = require_spawn_agent_params(&input.params)?;
    if let Some(unavailable) = validate_spawn_availability(&input, &params) {
        return Ok(SpawnValidation::Error(Box::new(unavailable)));
    }
    let Some(create_child_session) = input.create_child_session.clone() else {
        return Ok(SpawnValidation::Error(Box::new(spawn_error(
            "spawn_agent is unavailable: no child session runtime is configured.",
            &params.prompt,
        ))));
    };
    Ok(SpawnValidation::Valid(Box::new(ValidSpawnInput {
        active_tool_call_id: input.active_tool_call_id,
        create_child_session,
        ctx: input.ctx,
        desktop_notifier: input.desktop_notifier,
        dispatches: input.dispatches,
        handles: input.handles,
        ownerships: input.ownerships,
        params,
        store: input.store,
        waiting_desktop_notifications: input.waiting_desktop_notifications,
    })))
}

fn require_spawn_agent_params(params: &Value) -> Result<SpawnAgentParams, HandlerError> {
    let Some(object) = params.as_object() else {
        return Err(HandlerError::new(
            "spawn_agent context must be \"fresh\" or \"inherit\"",
        ));
    };
    if let Some(context) = object.get("context").filter(|context| !context.is_null())
        && !context.as_string().is_some_and(|value| value == "fresh")
        && !context.as_string().is_some_and(|value| value == "inherit")
    {
        return Err(HandlerError::new(
            "spawn_agent context must be \"fresh\" or \"inherit\"",
        ));
    }
    if !object.get("prompt").is_some_and(Value::is_string) {
        return Err(HandlerError::new("spawn_agent prompt must be a string"));
    }
    pi_js_compat::js_json::from_value(params).map_err(|error| HandlerError::new(error.to_string()))
}

fn validate_spawn_availability(
    input: &SpawnAgentRuntimeInput<'_>,
    params: &SpawnAgentParams,
) -> Option<AgentToolResult> {
    if is_child_agent_runtime(&input.ctx).unwrap_or(false) {
        return Some(spawn_error(CHILD_ORCHESTRATION_UNAVAILABLE_MESSAGE, &params.prompt));
    }
    if let Some(factory) = input.create_child_session.as_ref()
        && is_production_child_session_factory(factory)
    {
        let validation = validate_goal_objective(&params.prompt);
        if !validation.ok {
            return Some(spawn_error(
                &spawn_prompt_validation_message(&validation),
                &params.prompt,
            ));
        }
    }
    if input.create_child_session.is_none() {
        return Some(spawn_error(
            "spawn_agent is unavailable: no child session runtime is configured.",
            &params.prompt,
        ));
    }
    None
}

enum PrepareResult {
    Prepared(Box<PreparedSpawn>),
    Error(Box<AgentToolResult>),
}

fn prepare_spawn(input: &ValidSpawnInput) -> Result<PrepareResult, HandlerError> {
    let Some(coordinator) = create_lifecycle_coordinator(&input.store) else {
        return Ok(PrepareResult::Error(Box::new(spawn_error(
            "spawn_agent requires a persisted supervisor session.",
            &input.params.prompt,
        ))));
    };
    let agent_type = input
        .params
        .agent_type
        .as_ref()
        .map(JsString::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "default".into());
    let profile = resolve_configured_agent_profile(&agent_type, &input.ctx)?;
    let display_name = input
        .params
        .display_name
        .as_ref()
        .map(JsString::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            input
                .params
                .agent_type
                .as_ref()
                .map(JsString::trim)
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_else(|| "Agent".into());
    let prepared = coordinator
        .prepare_child(&PrepareChildCommandInput {
            spawn: SpawnAgent {
                parent_id: input.params.parent_id.clone(),
                display_name,
                agent_type,
                origin: None,
                cwd: input.ctx.cwd()?,
                permission: AgentPermission {
                    policy: "on-request".into(),
                    inherited_from: None,
                    narrowed: true,
                },
                worktree: None,
                model: profile.model_metadata,
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
    let context = match input.params.context.as_deref() {
        Some("fresh") => SpawnContext::Fresh,
        Some("inherit") => SpawnContext::Inherit,
        _ => profile.context.unwrap_or(SpawnContext::Inherit),
    };
    Ok(PrepareResult::Prepared(Box::new(PreparedSpawn {
        abort_controller: pi_js_compat::abort::AbortController::default(),
        context,
        coordinator,
        prepared,
    })))
}

async fn construct_child_session(
    input: &ValidSpawnInput,
    spawn: PreparedSpawn,
) -> Result<ConstructedSpawn, Box<AgentToolResult>> {
    match input
        .create_child_session
        .call(ChildAgentDispatchInput {
            active_tool_call_id: input.active_tool_call_id.clone(),
            agent: spawn.prepared.clone(),
            context: spawn.context,
            ctx: input.ctx.clone(),
            prompt: input.params.prompt.clone(),
            signal: Some(spawn.abort_controller.signal()),
        })
        .await
    {
        Ok(child_session) => {
            let mut prepared = spawn.prepared.clone();
            if let Some(transcript) = child_session.transcript() {
                prepared.transcript = Some(transcript);
            }
            Ok(ConstructedSpawn {
                prepared: PreparedSpawn { prepared, ..spawn },
                child_session,
            })
        }
        Err(error) => {
            let runtime_error = AgentError {
                code: Some("runtime_spawn_failed".to_owned()),
                message: error.clone(),
            };
            let failed = spawn
                .coordinator
                .commit_failed_child(&spawn.prepared, runtime_error)
                .map_err(|error| HandlerError::new(error.message()));
            let agent = match failed {
                Ok(crate::core::session_control_db::CreateFailedMultiAgentChildResult::Ok { agent }) => {
                    let _ = publish_coordinator_snapshot(&input.store, &agent);
                    *agent
                }
                _ => spawn.prepared.clone(),
            };
            Err(Box::new(tool_result(
                JsString::from("spawn_agent failed to construct child session: ") + &error,
                json!({"agent": agent, "dispatched": false, "prompt": input.params.prompt}),
            )))
        }
    }
}

fn commit_running_child(
    input: &ValidSpawnInput,
    spawn: ConstructedSpawn,
    owner_session_id: &JsString,
) -> Result<CommittedSpawn, Box<AgentToolResult>> {
    let created = spawn
        .prepared
        .coordinator
        .commit_running_child(
            &spawn.prepared.prepared,
            owner_session_id,
            runtime_process_identity(),
            None,
        )
        .map_err(|error| HandlerError::new(error.message()));
    let (agent, ownership) = match created {
        Ok(crate::core::session_control_db::CreateMultiAgentChildWithRuntimeOwnershipResult::Ok {
            agent,
            ownership,
        }) => (*agent, *ownership),
        Ok(crate::core::session_control_db::CreateMultiAgentChildWithRuntimeOwnershipResult::Err { error }) => {
            spawn.child_session.dispose();
            return Err(Box::new(tool_result(
                format!("spawn_agent failed: {}", error.as_str()),
                json!({
                    "agent": spawn.prepared.prepared,
                    "dispatched": false,
                    "prompt": input.params.prompt,
                }),
            )));
        }
        Err(error) => {
            spawn.child_session.dispose();
            return Err(Box::new(tool_result(
                JsString::from("spawn_agent failed: ") + &error.message,
                json!({
                    "agent": spawn.prepared.prepared,
                    "dispatched": false,
                    "prompt": input.params.prompt,
                }),
            )));
        }
    };
    if let Err(error) = input.store.publish_lifecycle_coordinator_snapshot(&agent) {
        spawn.child_session.dispose();
        return Err(Box::new(tool_result(
            JsString::from("spawn_agent failed: ") + &error.message,
            json!({"agent":spawn.prepared.prepared,"dispatched":false,"prompt":input.params.prompt}),
        )));
    }
    let runtime = Arc::new(OwnedAgentRuntime {
        abort_controller: spawn.prepared.abort_controller.clone(),
        coordinator: Arc::clone(&spawn.prepared.coordinator),
        lifecycle: OwnedLifecycleCommandInput {
            agent: agent.clone(),
            ownership,
        },
    });
    Ok(CommittedSpawn {
        spawn,
        runtime,
        running_agent: agent,
    })
}

fn input_store_ownership(input: &ValidSpawnInput, committed: &CommittedSpawn) {
    input
        .ownerships
        .lock()
        .expect("ownerships")
        .insert(committed.running_agent.id.clone(), Arc::clone(&committed.runtime));
}

fn dispatch_spawned_agent(input: &ValidSpawnInput, spawn: CommittedSpawn) -> Result<AgentToolResult, HandlerError> {
    let restore_generation = input.store.get_restore_generation();
    let store = Arc::clone(&input.store);
    let factory = input.create_child_session.clone();
    let created_session = spawn.spawn.child_session.clone();
    let ctx = input.ctx.clone();
    let handles = input.handles.clone();
    let prompt = input.params.prompt.clone();
    let running_agent = spawn.running_agent.clone();
    let dispatch_runtime = Arc::clone(&spawn.runtime);
    let agent = start_tool_dispatch(&StartToolDispatchInput {
        agent: spawn.running_agent.clone(),
        desktop_notifier: Arc::clone(&input.desktop_notifier),
        dispatch: Arc::new(move || {
            run_agent_session(super::dispatch_runtime::RunAgentSessionInput {
                create_child_session: factory.clone(),
                created_session: Some(created_session.clone()),
                ctx: ctx.clone(),
                expected_restore_generation: Some(restore_generation),
                handles: handles.clone(),
                prompt: prompt.clone(),
                reserved_runtime: Arc::clone(&dispatch_runtime),
                running_agent: running_agent.clone(),
                store: Arc::clone(&store),
            })
        }),
        dispatches: Arc::clone(&input.dispatches),
        handles: input.handles.clone(),
        reserved_runtime: Arc::clone(&spawn.runtime),
        store: Arc::clone(&input.store),
        waiting_desktop_notifications: Arc::clone(&input.waiting_desktop_notifications),
    });
    release_ownership_after_dispatch(&input.dispatches, &input.ownerships, &agent.id);
    Ok(tool_result(
        JsString::from("Spawned ")
            + &agent.display_name
            + js_format!(
                " ({}) [type: {}, context: {}]",
                agent.id,
                agent.agent_type,
                context_name(spawn.spawn.prepared.context)
            ),
        json!({"agent": agent, "dispatched": true, "prompt": input.params.prompt}),
    ))
}

fn context_name(context: SpawnContext) -> &'static str {
    match context {
        SpawnContext::Fresh => "fresh",
        SpawnContext::Inherit => "inherit",
    }
}

fn spawn_error(message: &str, prompt: &JsString) -> AgentToolResult {
    tool_result(
        message,
        json!({"agent": empty_agent("spawn_agent"), "dispatched": false, "prompt": prompt}),
    )
}

fn attach_error(message: impl Into<JsString>, prompt: Option<&JsString>) -> AgentToolResult {
    let mut details = json!({"agent": empty_agent("attach_session_agent"), "dispatched": false});
    if let Some(prompt) = prompt {
        details["prompt"] = json!(prompt);
    }
    tool_result(message, details)
}

#[derive(Clone)]
pub struct AttachSessionAgentRuntimeInput {
    pub create_attached_session: Option<AttachedSessionFactory>,
    pub ctx: ExtensionContext,
    pub desktop_notifier: Arc<dyn DesktopNotifier>,
    pub dispatches: ActiveAgentDispatches,
    pub handles: Option<BackgroundSessionHandles>,
    pub params: AttachSessionAgentParams,
    pub ownerships: Arc<Mutex<HashMap<JsString, Arc<OwnedAgentRuntime>>>>,
    pub store: Arc<MultiAgentStore>,
    pub waiting_desktop_notifications: WaitingDesktopNotificationHandles,
}

struct AttachSessionTarget {
    cwd: JsString,
    name: Option<JsString>,
    path: JsString,
    session_id: JsString,
}

pub async fn attach_session_agent(input: &AttachSessionAgentRuntimeInput) -> Result<AgentToolResult, HandlerError> {
    if is_child_agent_runtime(&input.ctx)? {
        return Ok(attach_error(CHILD_ORCHESTRATION_UNAVAILABLE_MESSAGE, None));
    }
    let resolution = load_attach_session_target(&input.params, &input.ctx)?;
    let target = match resolution {
        Ok(target) => target,
        Err(message) => return Ok(attach_error(&message, None)),
    };
    let prompt = input
        .params
        .prompt
        .as_ref()
        .map(JsString::trim)
        .filter(|value| !value.is_empty());
    if let Some(prompt) = prompt.as_ref()
        && input.create_attached_session.is_none()
    {
        let message = js_format!(
            "Could not resume session {}: no attached session runtime is configured.",
            target.session_id
        );
        return Ok(attach_error(&message, Some(&prompt)));
    }
    let agent = match create_attached_agent(&input.store, &input.params, &target, &input.ctx)? {
        Ok(agent) => agent,
        Err((error, parent)) => {
            return Ok(tool_result(
                js_format!("Could not attach session {}: {}", target.session_id, error),
                json!({
                    "agent": parent.unwrap_or_else(|| {
                        empty_agent(input.params.parent_id.clone().unwrap_or_else(|| "attach_session_agent".into()))
                    }),
                    "dispatched": false,
                }),
            ));
        }
    };
    Ok(dispatch_attached_agent(input, agent, prompt.as_ref()))
}

fn dispatch_attached_agent(
    input: &AttachSessionAgentRuntimeInput,
    agent: AgentSnapshot,
    prompt: Option<&JsString>,
) -> AgentToolResult {
    let Some(prompt) = prompt else {
        return tool_result(
            JsString::from("Attached ") + &agent.display_name + " (" + &agent.id + ")",
            json!({"agent": agent, "dispatched": false}),
        );
    };
    let dispatched = dispatch_attached_session_agent(&super::dispatch_runtime::AttachSessionDispatchInput {
        create_attached_session: input.create_attached_session.clone(),
        ctx: input.ctx.clone(),
        desktop_notifier: Arc::clone(&input.desktop_notifier),
        dispatches: Arc::clone(&input.dispatches),
        handles: input.handles.clone(),
        ownerships: Arc::clone(&input.ownerships),
        prompt: prompt.to_owned(),
        store: Arc::clone(&input.store),
        target: agent.clone(),
        waiting_desktop_notifications: Arc::clone(&input.waiting_desktop_notifications),
    })
    .unwrap_or(None);
    match dispatched {
        Some(dispatched) => tool_result(
            JsString::from("Attached and resumed ") + &dispatched.display_name + " (" + &dispatched.id + ")",
            json!({"agent": dispatched, "dispatched": true, "prompt": prompt}),
        ),
        None => tool_result(
            JsString::from("Attached ") + &agent.display_name + " (" + &agent.id + ")",
            json!({"agent": agent, "dispatched": false}),
        ),
    }
}

fn create_attached_agent(
    store: &Arc<MultiAgentStore>,
    params: &AttachSessionAgentParams,
    target: &AttachSessionTarget,
    ctx: &ExtensionContext,
) -> Result<Result<AgentSnapshot, (JsString, Option<AgentSnapshot>)>, HandlerError> {
    let agent_type = params
        .agent_type
        .as_ref()
        .map(JsString::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "resumed-session".into());
    let parent = params
        .parent_id
        .as_ref()
        .and_then(|parent_id| store.get_agent(parent_id));
    let Some(coordinator) = create_lifecycle_coordinator(store) else {
        return Ok(Err(("lifecycle_coordinator_unavailable".into(), None)));
    };
    let display_name = params
        .display_name
        .as_ref()
        .map(JsString::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| target.name.clone())
        .unwrap_or_else(|| JsString::from("Session ") + &target.session_id);
    let model = resolve_configured_agent_profile(&agent_type, ctx)?
        .model_metadata
        .or_else(|| parent.as_ref().and_then(|parent| parent.model.clone()));
    let attached = coordinator
        .create_attachment(&SpawnAgent {
            account: parent.as_ref().and_then(|parent| parent.account.clone()),
            agent_type,
            cwd: if target.cwd.is_empty() {
                ctx.cwd()?
            } else {
                target.cwd.clone()
            },
            display_name,
            model,
            parent_id: params.parent_id.clone(),
            permission: build_attached_session_permission(store, params.parent_id.as_ref(), ctx)?,
            transcript: Some(crate::core::multi_agent_store::AgentTranscriptMetadata {
                session_id: target.session_id.clone(),
                path: Some(target.path.clone()),
            }),
            origin: None,
            worktree: None,
            slot: None,
            event_stream: None,
            worker: None,
        })
        .map_err(|error| HandlerError::new(error.message()))?;
    match attached {
        CreateMultiAgentAttachmentResult::Ok { agent } => {
            store
                .publish_lifecycle_coordinator_snapshot(&agent)
                .map_err(|error| HandlerError::new(error.message))?;
            Ok(Ok(*agent))
        }
        CreateMultiAgentAttachmentResult::Err { error } => Ok(Err((error.as_str().into(), parent))),
    }
}

fn build_attached_session_permission(
    store: &MultiAgentStore,
    parent_id: Option<&JsString>,
    ctx: &ExtensionContext,
) -> Result<AgentPermission, HandlerError> {
    let Some(parent_id) = parent_id else {
        let policy: JsString = match ctx.settings_manager()? {
            Some(settings) => settings.lock().expect("settings").get_approval_policy().as_str().into(),
            None => "on-request".into(),
        };
        return Ok(AgentPermission {
            policy,
            inherited_from: None,
            narrowed: true,
        });
    };
    let parent = store.get_agent(parent_id);
    Ok(AgentPermission {
        policy: parent
            .as_ref()
            .map(|parent| parent.permission.policy.clone())
            .unwrap_or_else(|| "on-request".into()),
        inherited_from: Some(parent_id.to_owned()),
        narrowed: true,
    })
}

fn load_attach_session_target(
    params: &AttachSessionAgentParams,
    ctx: &ExtensionContext,
) -> Result<Result<AttachSessionTarget, JsString>, HandlerError> {
    let selectors = (
        params
            .name
            .as_ref()
            .map(JsString::trim)
            .filter(|value| !value.is_empty()),
        params
            .path
            .as_ref()
            .map(JsString::trim)
            .filter(|value| !value.is_empty()),
        params
            .session_id
            .as_ref()
            .map(JsString::trim)
            .filter(|value| !value.is_empty()),
    );
    let count = [selectors.0.is_some(), selectors.1.is_some(), selectors.2.is_some()]
        .into_iter()
        .filter(|selected| *selected)
        .count();
    if count != 1 {
        return Ok(Err(
            "Could not attach session: provide exactly one of path, sessionId, or name.".into(),
        ));
    }
    let session_manager = ctx.session_manager()?;
    let session_dir = session_manager.get_session_dir().to_owned();
    if let Some(path) = selectors.1 {
        let session = SessionManager::open(&path, Some(&session_dir), None)
            .map_err(|error| HandlerError::new(error.message().clone()))?;
        return Ok(Ok(AttachSessionTarget {
            cwd: session.get_cwd().to_owned(),
            name: session.get_session_name(),
            path: session.get_session_file().unwrap_or(&path).clone(),
            session_id: session.get_session_id().to_owned(),
        }));
    }
    let control_db_path = ctx.control_db_path()?;
    let local_sessions = SessionManager::list(&ctx.cwd()?, Some(&session_dir), None, control_db_path.as_ref())
        .map_err(|error| HandlerError::new(error.to_string()))?;
    let all_sessions = SessionManager::list_all(Some(&session_dir), None, control_db_path.as_ref())
        .map_err(|error| HandlerError::new(error.to_string()))?;
    let mut sessions = local_sessions;
    sessions.extend(all_sessions);
    let sessions = dedupe_sessions_by_path(sessions);
    Ok(
        match find_matching_session(&sessions, selectors.0.as_ref(), selectors.2.as_ref()) {
            SessionMatch::Found(session) => Ok(session_info_to_target(session)),
            SessionMatch::Ambiguous(selector) => {
                Err(JsString::from("Could not attach session: ") + selector + " matches multiple sessions.")
            }
            SessionMatch::NotFound => Err("Could not attach session: session not found.".into()),
        },
    )
}

enum SessionMatch<'a> {
    Found(&'a SessionInfo),
    Ambiguous(JsString),
    NotFound,
}

fn dedupe_sessions_by_path(sessions: Vec<SessionInfo>) -> Vec<SessionInfo> {
    let mut deduped: indexmap::IndexMap<JsString, SessionInfo> = indexmap::IndexMap::new();
    for session in sessions {
        deduped.insert(session.path.clone(), session);
    }
    deduped.into_values().collect()
}

fn find_matching_session<'a>(
    sessions: &'a [SessionInfo],
    name: Option<&JsString>,
    session_id: Option<&JsString>,
) -> SessionMatch<'a> {
    if let Some(session_id) = session_id {
        return find_matching_session_id(sessions, session_id);
    }
    let matches: Vec<&SessionInfo> = sessions
        .iter()
        .filter(|session| session.name.as_ref().map(JsString::from).as_ref() == name)
        .collect();
    if matches.len() > 1 {
        return SessionMatch::Ambiguous(name.cloned().unwrap_or_else(|| "name".into()));
    }
    match matches.first() {
        Some(session) => SessionMatch::Found(session),
        None => SessionMatch::NotFound,
    }
}

fn find_matching_session_id<'a>(sessions: &'a [SessionInfo], session_id: &JsString) -> SessionMatch<'a> {
    let exact: Vec<&SessionInfo> = sessions.iter().filter(|session| session.id == *session_id).collect();
    if exact.len() > 1 {
        return SessionMatch::Ambiguous(session_id.into());
    }
    if let Some(session) = exact.first() {
        return SessionMatch::Found(session);
    }
    let prefix: Vec<&SessionInfo> = sessions
        .iter()
        .filter(|session| session.id.starts_with(session_id))
        .collect();
    if prefix.len() > 1 {
        return SessionMatch::Ambiguous(session_id.into());
    }
    match prefix.first() {
        Some(session) => SessionMatch::Found(session),
        None => SessionMatch::NotFound,
    }
}

fn session_info_to_target(session: &SessionInfo) -> AttachSessionTarget {
    AttachSessionTarget {
        cwd: session.cwd.clone(),
        name: session.name.clone().map(JsString::from),
        path: session.path.clone(),
        session_id: session.id.clone(),
    }
}

#[derive(Clone)]
pub struct RecoverAgentsInput {
    pub create_attached_session: Option<AttachedSessionFactory>,
    pub ctx: ExtensionContext,
    pub desktop_notifier: Arc<dyn DesktopNotifier>,
    pub dispatches: ActiveAgentDispatches,
    pub handles: Option<BackgroundSessionHandles>,
    pub ownerships: Arc<Mutex<HashMap<JsString, Arc<OwnedAgentRuntime>>>>,
    pub store: Arc<MultiAgentStore>,
    pub waiting_desktop_notifications: WaitingDesktopNotificationHandles,
}

pub fn recover_agents(input: &RecoverAgentsInput) -> Result<(), HandlerError> {
    let active_parent_agent_ids = Some(read_journal_ids(&input.ctx)?);
    let active_agents = query_recovery_targets(input)?;
    for agent in order_recovery_agents(active_agents) {
        let admitted = active_parent_agent_ids.as_ref().is_none_or(|ids| {
            agent.transcript.as_ref().and_then(|t| t.path.as_ref()).is_none() || ids.contains(&agent.id)
        });
        if admitted {
            recover_agent(input, &agent)?;
        } else {
            recover_and_publish_dead_agent(input, &agent)?;
        }
    }
    Ok(())
}

pub fn read_active_parent_agent_ids(
    ctx: &ExtensionContext,
) -> Result<std::collections::HashSet<JsString>, HandlerError> {
    read_journal_ids(ctx)
}

fn query_recovery_targets(input: &RecoverAgentsInput) -> Result<Vec<AgentSnapshot>, HandlerError> {
    let active_agents = input.store.list_active_agents();
    let parent_id = input.ctx.multi_agent_agent_id()?;
    Ok(match parent_id {
        Some(parent_id) => active_agents
            .into_iter()
            .filter(|agent| agent.parent_id.as_ref() == Some(&parent_id))
            .collect(),
        None => active_agents,
    })
}

fn order_recovery_agents(agents: Vec<AgentSnapshot>) -> Vec<AgentSnapshot> {
    let by_id: HashMap<JsString, AgentSnapshot> =
        agents.iter().map(|agent| (agent.id.clone(), agent.clone())).collect();
    let mut depths: HashMap<JsString, usize> = HashMap::new();
    let mut ordered = agents;
    ordered.sort_by_key(|agent| std::cmp::Reverse(depth_of(agent, &by_id, &mut depths)));
    ordered
}

fn depth_of(
    agent: &AgentSnapshot,
    by_id: &HashMap<JsString, AgentSnapshot>,
    depths: &mut HashMap<JsString, usize>,
) -> usize {
    if let Some(depth) = depths.get(&agent.id) {
        return *depth;
    }
    let parent = agent.parent_id.as_ref().and_then(|parent_id| by_id.get(parent_id));
    let depth = parent.map_or(0, |parent| depth_of(parent, by_id, depths) + 1);
    depths.insert(agent.id.clone(), depth);
    depth
}

fn recover_agent(input: &RecoverAgentsInput, agent: &AgentSnapshot) -> Result<(), HandlerError> {
    if input.dispatches.lock().expect("dispatches").contains_key(&agent.id) || !is_in_flight_lifecycle(agent.lifecycle)
    {
        return Ok(());
    }
    if agent.lifecycle != AgentLifecycle::Cancelling
        && input.create_attached_session.is_some()
        && agent.transcript.as_ref().and_then(|t| t.path.as_ref()).is_some()
    {
        dispatch_attached_session_agent(&super::dispatch_runtime::AttachSessionDispatchInput {
            create_attached_session: input.create_attached_session.clone(),
            ctx: input.ctx.clone(),
            desktop_notifier: Arc::clone(&input.desktop_notifier),
            dispatches: Arc::clone(&input.dispatches),
            handles: input.handles.clone(),
            ownerships: Arc::clone(&input.ownerships),
            prompt: CRASH_RECOVERY_PROMPT.to_owned().into(),
            store: Arc::clone(&input.store),
            target: agent.clone(),
            waiting_desktop_notifications: Arc::clone(&input.waiting_desktop_notifications),
        })?;
        return Ok(());
    }
    if has_live_agent_owner(input, agent)? {
        return Ok(());
    }
    if agent.lifecycle == AgentLifecycle::Cancelling
        || agent.transcript.as_ref().and_then(|t| t.path.as_ref()).is_none()
    {
        recover_and_publish_dead_agent(input, agent)?;
    }
    Ok(())
}

fn recover_and_publish_dead_agent(input: &RecoverAgentsInput, agent: &AgentSnapshot) -> Result<(), HandlerError> {
    let Some(persistence) = input.store.get_persistence_target() else {
        return Ok(());
    };
    let Some(coordinator) = create_lifecycle_coordinator(&input.store) else {
        return Ok(());
    };
    let dead_ownership =
        read_multi_agent_runtime_ownership(&persistence.control_db_path, &persistence.session_path, &agent.id)
            .map_err(|error| HandlerError::new(error.message()))?;
    let Some(dead_ownership) = dead_ownership else {
        return Ok(());
    };
    let owner_session_id = input.ctx.session_manager()?.get_session_id().to_owned();
    let recovered = coordinator
        .recover_dead_child(&crate::core::lifecycle_coordinator::RecoverDeadChildCommandInput {
            agent: agent.clone(),
            ownership: dead_ownership,
            owner_session_id,
            supervisor_agent_id: input.ctx.multi_agent_agent_id()?,
        })
        .map_err(|error| HandlerError::new(error.message()))?;
    if let crate::core::lifecycle_coordinator::RecoverDeadChildCommandResult::Ok { agent } = recovered {
        publish_coordinator_snapshot(&input.store, &agent).map_err(HandlerError::from)?;
    }
    Ok(())
}

fn has_live_agent_owner(input: &RecoverAgentsInput, agent: &AgentSnapshot) -> Result<bool, HandlerError> {
    let Some(persistence) = input.store.get_persistence_target() else {
        return Ok(false);
    };
    let ownership =
        read_multi_agent_runtime_ownership(&persistence.control_db_path, &persistence.session_path, &agent.id)
            .map_err(|error| HandlerError::new(error.message()))?;
    Ok(ownership
        .and_then(|ownership| ownership.process_identity)
        .is_some_and(|identity| is_process_identity_alive(&identity)))
}

fn resolve_configured_agent_profile(
    agent_type: &JsString,
    ctx: &ExtensionContext,
) -> Result<ResolvedAgentProfile, HandlerError> {
    let Some(settings) = ctx.settings_manager()? else {
        return Ok(ResolvedAgentProfile::default());
    };
    let profile = settings.lock().expect("settings").get_agent_profile(agent_type);
    let Some(profile) = profile.filter(pi_js_compat::js_json::is_truthy) else {
        return Ok(ResolvedAgentProfile::default());
    };
    let model = read_configured_profile_model(&profile, ctx)?;
    let model_metadata = model
        .as_ref()
        .map(|model| {
            let thinking_level = profile
                .get("thinkingLevel")
                .filter(|value| !value.is_null())
                .map(pi_js_compat::js_json::from_value::<String>)
                .transpose()
                .map_err(|error| HandlerError::new(error.to_string()))?;
            Ok::<_, HandlerError>(AgentModel {
                provider_id: model.provider.to_string(),
                model_id: model.id.clone(),
                thinking_level,
            })
        })
        .transpose()?;
    // Production child-session creation selects inheritance by strict equality.
    let context = profile.get("context").filter(|value| !value.is_null()).map(|value| {
        if value.as_string().is_some_and(|context| context == "inherit") {
            SpawnContext::Inherit
        } else {
            SpawnContext::Fresh
        }
    });
    Ok(ResolvedAgentProfile {
        context,
        model,
        model_metadata,
    })
}

fn read_configured_profile_model(
    profile: &Value,
    ctx: &ExtensionContext,
) -> Result<Option<pi_ai::types::Model>, HandlerError> {
    let Some(reference) = profile
        .get("model")
        .filter(|value| pi_js_compat::js_json::is_truthy(value))
    else {
        return Ok(None);
    };
    let models = ctx.model_registry()?.get_all();
    let reference = reference
        .as_string()
        .ok_or_else(|| HandlerError::new("modelReference.trim is not a function"))?;
    Ok(find_exact_model_reference_match(reference, &models))
}
