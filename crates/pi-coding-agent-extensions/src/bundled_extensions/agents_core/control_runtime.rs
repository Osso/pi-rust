//! Agent cancellation and steering (`extensions/agents-core/src/control-runtime.ts`).

use std::sync::Arc;
use std::time::Duration;

use pi_agent_core::AgentToolResult;
use pi_js_compat::js_format;
use pi_js_compat::js_json::{JsString, JsValue};

use crate::core::extensions::types::{ExtensionContext, HandlerError};
use crate::core::lifecycle_coordinator::{
    DetachedCancellationCommandInput, OwnedLifecycleCommandInput, SteeringCommandInput, SteeringCommandResult,
};
use crate::core::multi_agent_store::{
    AgentLifecycle, AgentMailboxMessage, AgentSnapshot, MultiAgentStore, SteeringCheckpoint, is_active_lifecycle,
};
use crate::core::runtime_process::is_process_identity_alive;
use crate::core::session_control_db::{MultiAgentRuntimeOwnership, read_multi_agent_runtime_ownership};
use crate::core::session_manager::lock_session_manager;

use super::lifecycle_runtime::{create_lifecycle_coordinator, publish_coordinator_snapshot};
use super::mailbox_runtime::current_message_sender_id;
use super::runtime::MultiAgentRuntimeHandles;
use super::runtime_utils::{abort_agent_handle_safely, create_failed_mailbox_message};
use super::shared::{WaitAgentsWakeUp, empty_agent};
use super::tool_results::{error_tool_result, tool_result};

const CANCELLATION_SETTLEMENT_TIMEOUT_MS: u64 = 5_000;
const MAIN_THREAD_AGENT_ID: &str = "main";

#[derive(Clone)]
pub struct OwnedAgentRuntime {
    pub abort_controller: pi_js_compat::abort::AbortController,
    pub coordinator: Arc<crate::core::lifecycle_coordinator::LifecycleCoordinator>,
    pub lifecycle: OwnedLifecycleCommandInput,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelAgentParams {
    pub agent_id: JsString,
    pub reason: Option<JsString>,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SteerAgentParams {
    pub agent_id: JsString,
    pub file_refs: Option<JsValue>,
    pub from_agent_id: Option<JsString>,
    pub message: JsString,
    pub target_checkpoint: Option<SteeringCheckpoint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelReservedError {
    AgentNotFound,
    RuntimeOwnershipUnavailable,
    MutationRejected,
}

impl CancelReservedError {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AgentNotFound => "agent_not_found",
            Self::RuntimeOwnershipUnavailable => "runtime_ownership_unavailable",
            Self::MutationRejected => "mutation_rejected",
        }
    }
}

#[derive(Debug, Clone)]
pub enum CancelReservedAgentResult {
    Ok {
        agent: AgentSnapshot,
    },
    Err {
        error: CancelReservedError,
        agent: Option<AgentSnapshot>,
    },
}

#[derive(Debug, Clone)]
pub struct AgentSteeringRuntimeBinding {
    pub actor_agent_id: Option<JsString>,
    pub control_db_path: JsString,
    pub session_id: JsString,
}

#[derive(Debug, Clone)]
pub struct AgentSteeringRequest {
    pub agent_id: JsString,
    pub message: JsString,
    pub file_refs: Option<JsValue>,
    pub target_checkpoint: Option<SteeringCheckpoint>,
}

#[derive(Debug, Clone)]
pub enum AgentSteeringRequestResult {
    Ok {
        agent: AgentSnapshot,
        message: AgentMailboxMessage,
    },
    Err {
        agent: AgentSnapshot,
        message: AgentMailboxMessage,
        error: JsString,
    },
}

struct SteeringAuthority {
    coordinator: Arc<crate::core::lifecycle_coordinator::LifecycleCoordinator>,
    ownership: MultiAgentRuntimeOwnership,
    target_session_id: JsString,
}

struct DetachedCancellationAuthority {
    coordinator: Arc<crate::core::lifecycle_coordinator::LifecycleCoordinator>,
    output_label: JsString,
    ownership: MultiAgentRuntimeOwnership,
}

pub async fn cancel_owned_agent_runtime(
    store: &Arc<MultiAgentStore>,
    runtime_handles: &MultiAgentRuntimeHandles,
    agent_id: &JsString,
    reason: Option<&JsString>,
) -> Result<CancelReservedAgentResult, HandlerError> {
    let mut descendants: Vec<JsString> = store
        .list_descendants(agent_id)
        .into_iter()
        .filter(|agent| is_active_lifecycle(agent.lifecycle))
        .map(|agent| agent.id)
        .collect();
    descendants.reverse();
    for descendant in descendants {
        let cancelled = cancel_one_owned_agent_runtime(store, runtime_handles, &descendant, reason).await?;
        if let CancelReservedAgentResult::Err { .. } = cancelled {
            return Ok(cancelled);
        }
    }
    cancel_one_owned_agent_runtime(store, runtime_handles, agent_id, reason).await
}

pub async fn cancel_agent(
    store: &Arc<MultiAgentStore>,
    runtime_handles: &MultiAgentRuntimeHandles,
    params: &CancelAgentParams,
) -> Result<AgentToolResult, HandlerError> {
    let cancelled =
        cancel_owned_agent_runtime(store, runtime_handles, &params.agent_id, params.reason.as_ref()).await?;
    match cancelled {
        CancelReservedAgentResult::Ok { agent } => Ok(tool_result(
            if agent.lifecycle == AgentLifecycle::Aborted {
                JsString::from("Cancelled ") + &agent.display_name + "."
            } else {
                JsString::from("Cancellation requested for ") + &agent.display_name + "."
            },
            pi_js_compat::js_json!({"agent": agent, "reason": params.reason}),
        )),
        CancelReservedAgentResult::Err { error, agent } => {
            let error = match error {
                CancelReservedError::RuntimeOwnershipUnavailable => "runtime ownership unavailable".to_owned(),
                other => other.as_str().to_owned(),
            };
            Ok(error_tool_result(
                js_format!("Could not cancel {}: {}", params.agent_id, error),
                pi_js_compat::js_json!({
                    "agent": agent.unwrap_or_else(|| empty_agent(&params.agent_id)),
                    "reason": params.reason,
                }),
            ))
        }
    }
}

pub fn wake_wait_agents_after_steering(runtime_handles: &MultiAgentRuntimeHandles) {
    runtime_handles
        .wait_wake_listeners
        .emit(&WaitAgentsWakeUp::steering(None));
}

pub fn request_interactive_agent_steering(
    store: &Arc<MultiAgentStore>,
    runtime_handles: &MultiAgentRuntimeHandles,
    agent_id: &JsString,
    message: &JsString,
    binding: &AgentSteeringRuntimeBinding,
) -> Result<(), JsString> {
    let steered = request_agent_steering(
        store,
        &AgentSteeringRequest {
            agent_id: agent_id.to_owned(),
            message: message.to_owned(),
            file_refs: None,
            target_checkpoint: Some(SteeringCheckpoint::NextModelCall),
        },
        binding,
        Some(runtime_handles),
    )
    .map_err(|error| error.message)?;
    match steered {
        AgentSteeringRequestResult::Ok { .. } => Ok(()),
        AgentSteeringRequestResult::Err { error, .. } => Err(error.into()),
    }
}

pub fn request_agent_steering(
    store: &Arc<MultiAgentStore>,
    params: &AgentSteeringRequest,
    binding: &AgentSteeringRuntimeBinding,
    runtime_handles: Option<&MultiAgentRuntimeHandles>,
) -> Result<AgentSteeringRequestResult, HandlerError> {
    let Some(current) = store.get_agent(&params.agent_id) else {
        return Ok(steering_not_found(params));
    };
    if !is_active_lifecycle(current.lifecycle) {
        return Ok(steering_inactive(&current, &params.message));
    }

    let Some(authority) = read_steering_authority(store, &current, binding)? else {
        return Ok(steering_ownership_unavailable(&current, &params.message));
    };
    let steered = authority
        .coordinator
        .request_steering(&SteeringCommandInput {
            agent: current.clone(),
            ownership: authority.ownership,
            recipient: crate::core::session_control_db::RuntimeMailboxAddress {
                agent_id: Some(current.id.clone().into()),
                session_id: authority.target_session_id.into(),
            },
            from_agent_id: binding.actor_agent_id.clone().unwrap_or_else(|| "supervisor".into()),
            body: params.message.clone(),
            target_checkpoint: params.target_checkpoint,
            thread_id: None,
            file_refs: params.file_refs.clone(),
        })
        .map_err(|error| HandlerError::new(error.message()))?;
    match steered {
        SteeringCommandResult::Ok { agent, message } => {
            store.publish_lifecycle_coordinator_steering(&agent, &message);
            if let Some(runtime_handles) = runtime_handles {
                runtime_handles
                    .wait_wake_listeners
                    .emit(&WaitAgentsWakeUp::steering(Some(agent.id.clone())));
            }
            Ok(AgentSteeringRequestResult::Ok {
                agent: *agent,
                message: *message,
            })
        }
        SteeringCommandResult::Err { error } => Ok(steering_rejected(&current, &params.message, error.as_str())),
    }
}

pub fn steer_agent(
    store: &Arc<MultiAgentStore>,
    runtime_handles: &MultiAgentRuntimeHandles,
    params: &SteerAgentParams,
    ctx: Option<&ExtensionContext>,
) -> Result<AgentToolResult, HandlerError> {
    let persistence = store.get_persistence_target();
    let ctx_control_db_path = match ctx {
        Some(ctx) => ctx.control_db_path()?,
        None => None,
    };
    let control_db_path =
        ctx_control_db_path.or_else(|| persistence.as_ref().map(|value| value.control_db_path.clone()));
    let sender_id = current_steering_sender_id(ctx)?;
    let has_steering_identity = ctx.is_some() && control_db_path.is_some() && sender_id.is_some();
    if !has_steering_identity {
        return Ok(steering_identity_unavailable(params));
    }
    let Some(ctx) = ctx else {
        return Ok(steering_identity_unavailable(params));
    };
    let Some(control_db_path) = control_db_path else {
        return Ok(steering_identity_unavailable(params));
    };
    let Some(sender_id) = sender_id else {
        return Ok(steering_identity_unavailable(params));
    };
    let session_id = lock_session_manager(&ctx.session_manager()?)
        .get_session_id()
        .to_owned();
    let steered = request_agent_steering(
        store,
        &AgentSteeringRequest {
            agent_id: params.agent_id.clone(),
            message: params.message.clone(),
            file_refs: params.file_refs.clone(),
            target_checkpoint: params.target_checkpoint,
        },
        &AgentSteeringRuntimeBinding {
            actor_agent_id: if sender_id == "supervisor" {
                None
            } else {
                Some(sender_id)
            },
            control_db_path,
            session_id,
        },
        Some(runtime_handles),
    )?;
    match steered {
        AgentSteeringRequestResult::Ok { agent, message } => Ok(tool_result(
            JsString::from("Queued steering for ") + &agent.display_name + ".",
            pi_js_compat::js_json!({"agent": agent, "message": message}),
        )),
        AgentSteeringRequestResult::Err { agent, message, error } => Ok(error_tool_result(
            error,
            pi_js_compat::js_json!({"agent": agent, "message": message}),
        )),
    }
}

fn steering_identity_unavailable(params: &SteerAgentParams) -> AgentToolResult {
    error_tool_result(
        "Could not steer agent: subagent runtime identity is unavailable.",
        pi_js_compat::js_json!({
            "agent": empty_agent(&params.agent_id),
            "message": empty_message(&params.agent_id, &params.message),
        }),
    )
}

async fn cancel_one_owned_agent_runtime(
    store: &Arc<MultiAgentStore>,
    runtime_handles: &MultiAgentRuntimeHandles,
    agent_id: &JsString,
    reason: Option<&JsString>,
) -> Result<CancelReservedAgentResult, HandlerError> {
    let Some(current) = store.get_agent(agent_id) else {
        return Ok(CancelReservedAgentResult::Err {
            error: CancelReservedError::AgentNotFound,
            agent: None,
        });
    };
    let reserved_runtime = runtime_handles
        .ownerships
        .lock()
        .expect("ownerships")
        .get(agent_id)
        .cloned();
    let Some(reserved_runtime) = reserved_runtime else {
        return cancel_persisted_detached_runtime(store, &current, reason);
    };
    let cancelling = reserved_runtime
        .coordinator
        .request_cancellation(&OwnedLifecycleCommandInput {
            agent: current.clone(),
            ownership: reserved_runtime.lifecycle.ownership.clone(),
        })
        .map_err(|error| HandlerError::new(error.message()))?;
    let cancelling_agent = match cancelling {
        crate::core::lifecycle_coordinator::LifecycleCommandResult::Ok { agent } => *agent,
        crate::core::lifecycle_coordinator::LifecycleCommandResult::Err { .. } => {
            return Ok(CancelReservedAgentResult::Err {
                error: CancelReservedError::MutationRejected,
                agent: Some(current),
            });
        }
    };
    store
        .publish_lifecycle_coordinator_snapshot(&cancelling_agent)
        .map_err(|error| HandlerError::new(error.message))?;
    abort_agent_handle_safely(store, agent_id);
    let dispatch = runtime_handles
        .dispatches
        .lock()
        .expect("dispatches")
        .get(agent_id)
        .cloned();
    if let Some(dispatch) = dispatch {
        let _ = tokio::time::timeout(Duration::from_millis(CANCELLATION_SETTLEMENT_TIMEOUT_MS), dispatch).await;
    }
    Ok(CancelReservedAgentResult::Ok {
        agent: store.get_agent(agent_id).unwrap_or(cancelling_agent),
    })
}

fn cancel_persisted_detached_runtime(
    store: &Arc<MultiAgentStore>,
    agent: &AgentSnapshot,
    reason: Option<&JsString>,
) -> Result<CancelReservedAgentResult, HandlerError> {
    let Some(authority) = read_detached_cancellation_authority(store, agent)? else {
        return Ok(CancelReservedAgentResult::Err {
            error: CancelReservedError::RuntimeOwnershipUnavailable,
            agent: Some(agent.clone()),
        });
    };
    let cancelled = authority
        .coordinator
        .request_detached_cancellation(&DetachedCancellationCommandInput {
            agent: agent.clone(),
            ownership: authority.ownership,
            output_label: authority.output_label,
            reason: reason.cloned(),
            suppress_terminal_notification: None,
        })
        .map_err(|error| HandlerError::new(error.message()))?;
    match cancelled {
        crate::core::lifecycle_coordinator::LifecycleCommandResult::Ok { agent } => {
            publish_coordinator_snapshot(store, &agent).map_err(HandlerError::from)?;
            Ok(CancelReservedAgentResult::Ok { agent: *agent })
        }
        crate::core::lifecycle_coordinator::LifecycleCommandResult::Err { .. } => Ok(CancelReservedAgentResult::Err {
            error: CancelReservedError::MutationRejected,
            agent: Some(agent.clone()),
        }),
    }
}

fn read_detached_cancellation_authority(
    store: &Arc<MultiAgentStore>,
    agent: &AgentSnapshot,
) -> Result<Option<DetachedCancellationAuthority>, HandlerError> {
    let Some(persistence) = store.get_persistence_target() else {
        return Ok(None);
    };
    let Some(output_label) = detached_runtime_output_label(agent) else {
        return Ok(None);
    };
    let ownership =
        read_multi_agent_runtime_ownership(&persistence.control_db_path, &persistence.session_path, &agent.id)
            .map_err(|error| HandlerError::new(error.message()))?;
    let Some(ownership) = ownership else {
        return Ok(None);
    };
    if ownership.process_identity.is_none() || ownership.owner.session_id.is_none() {
        return Ok(None);
    }
    let Some(coordinator) = create_lifecycle_coordinator(store) else {
        return Ok(None);
    };
    Ok(Some(DetachedCancellationAuthority {
        coordinator,
        output_label,
        ownership,
    }))
}

fn detached_runtime_output_label(agent: &AgentSnapshot) -> Option<JsString> {
    let is_detached_runtime = agent.agent_type == "background" && agent.worker.is_some();
    if !is_detached_runtime {
        return None;
    }
    agent
        .result
        .as_ref()?
        .file_refs
        .as_ref()?
        .iter()
        .filter_map(|file_ref| file_ref.label.as_ref())
        .find(|label| is_detached_runtime_output_label(label))
        .cloned()
}

fn is_detached_runtime_output_label(label: &JsString) -> bool {
    *label == "Bash output" || *label == "Pyrun output"
}

fn current_steering_sender_id(ctx: Option<&ExtensionContext>) -> Result<Option<JsString>, HandlerError> {
    let Some(sender_id) = current_message_sender_id(ctx)? else {
        return Ok(None);
    };
    Ok(Some(if sender_id == MAIN_THREAD_AGENT_ID {
        "supervisor".into()
    } else {
        sender_id
    }))
}

fn read_steering_authority(
    store: &Arc<MultiAgentStore>,
    agent: &AgentSnapshot,
    binding: &AgentSteeringRuntimeBinding,
) -> Result<Option<SteeringAuthority>, HandlerError> {
    let Some(persistence) = store.get_persistence_target() else {
        return Ok(None);
    };
    if persistence.control_db_path != binding.control_db_path {
        return Ok(None);
    }
    let ownership =
        read_multi_agent_runtime_ownership(&persistence.control_db_path, &persistence.session_path, &agent.id)
            .map_err(|error| HandlerError::new(error.message()))?;
    let Some(ownership) = ownership else {
        return Ok(None);
    };
    let Some(coordinator) = create_lifecycle_coordinator(store) else {
        return Ok(None);
    };
    Ok(authorize_steering(agent, binding, coordinator, ownership))
}

fn authorize_steering(
    agent: &AgentSnapshot,
    binding: &AgentSteeringRuntimeBinding,
    coordinator: Arc<crate::core::lifecycle_coordinator::LifecycleCoordinator>,
    ownership: MultiAgentRuntimeOwnership,
) -> Option<SteeringAuthority> {
    if binding.actor_agent_id != ownership.owner.agent_id {
        return None;
    }
    if Some(&binding.session_id) != ownership.owner.session_id.as_ref() {
        return None;
    }
    let process_identity = ownership.process_identity.as_ref()?;
    if !is_process_identity_alive(process_identity) {
        return None;
    }
    let target_session_id = agent.transcript.as_ref()?.session_id.clone();
    Some(SteeringAuthority {
        coordinator,
        ownership,
        target_session_id,
    })
}

fn steering_rejected(agent: &AgentSnapshot, message: &JsString, coordinator_error: &str) -> AgentSteeringRequestResult {
    let error = if coordinator_error == "invalid_transition" {
        "inactive"
    } else {
        coordinator_error
    };
    AgentSteeringRequestResult::Err {
        agent: agent.clone(),
        error: js_format!("Could not steer {}: {}", agent.id, error),
        message: empty_message(&agent.id, message),
    }
}

fn steering_not_found(params: &AgentSteeringRequest) -> AgentSteeringRequestResult {
    AgentSteeringRequestResult::Err {
        agent: empty_agent(&params.agent_id),
        error: js_format!("Could not steer {}: not_found", params.agent_id),
        message: empty_message(&params.agent_id, &params.message),
    }
}

fn steering_inactive(agent: &AgentSnapshot, message: &JsString) -> AgentSteeringRequestResult {
    AgentSteeringRequestResult::Err {
        agent: agent.clone(),
        error: js_format!(
            "Could not steer {}: inactive ({})",
            agent.id,
            lifecycle_name(agent.lifecycle)
        ),
        message: empty_message(&agent.id, message),
    }
}

fn steering_ownership_unavailable(agent: &AgentSnapshot, message: &JsString) -> AgentSteeringRequestResult {
    AgentSteeringRequestResult::Err {
        agent: agent.clone(),
        error: js_format!("Could not steer {}: runtime ownership unavailable", agent.id),
        message: empty_message(&agent.id, message),
    }
}

fn empty_message(agent_id: &JsString, body: &JsString) -> AgentMailboxMessage {
    create_failed_mailbox_message(
        body,
        "supervisor",
        crate::core::multi_agent_store::MailboxMessageKindValue::Steer,
        agent_id,
    )
}

fn lifecycle_name(lifecycle: AgentLifecycle) -> &'static str {
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
