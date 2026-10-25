//! Lifecycle command coordinator (`core/lifecycle-coordinator.ts`).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use pi_js_compat::js_json::JsValue as Value;
use pi_js_compat::js_json::{self, JsString, JsValue};

use crate::core::multi_agent_store::{
    AgentError, AgentMailboxMessage, AgentResult, AgentSnapshot, SpawnAgent, SteeringCheckpoint,
};
use crate::core::runtime_process::ProcessIdentity;
use crate::core::session_control_db::RuntimeMailboxAddress;
use crate::core::session_control_db::agents::{
    AgentDetails, LifecycleCommandError, RecoverDeadRuntimeError, SteeringCommandError,
};
use crate::core::session_control_db::{
    AcquireAttachedRuntimeOwnershipInput, AcquireAttachedRuntimeOwnershipResult, CommitMultiAgentDetachMarkInput,
    CommitMultiAgentDetachMarkResult, CommitMultiAgentLifecycleMutationInput, CommitMultiAgentSteeringDeliveryInput,
    CommitMultiAgentSteeringDeliveryResult, CommitMultiAgentSteeringMutationInput,
    CommitMultiAgentSteeringMutationResult, CommitMultiAgentTerminalMutationInput, CreateFailedMultiAgentChildInput,
    CreateFailedMultiAgentChildResult, CreateMultiAgentAttachmentInput, CreateMultiAgentAttachmentResult,
    CreateMultiAgentChildWithRuntimeOwnershipInput, CreateMultiAgentChildWithRuntimeOwnershipResult,
    MultiAgentRuntimeOwnerIdentity, MultiAgentRuntimeOwnership, MultiAgentRuntimeOwnershipIdentity,
    RecoverDeadMultiAgentRuntimeInput, RecoverDeadMultiAgentRuntimeResult, SupervisorRuntimeOwnership,
    acquire_attached_runtime_ownership, commit_multi_agent_detach_mark, commit_multi_agent_lifecycle_mutation,
    commit_multi_agent_steering_delivery, commit_multi_agent_steering_mutation, commit_multi_agent_terminal_mutation,
    create_failed_multi_agent_child, create_multi_agent_attachment, create_multi_agent_child_with_runtime_ownership,
    read_multi_agent_agent, reconcile_dead_detached_agent_runtimes, recover_dead_multi_agent_runtime,
};
use crate::core::sqlite::ControlDbError;
use crate::utils::iso_time;

const MAIN_THREAD_AGENT_ID: &str = "main";
const DEAD_DETACHED_RECONCILIATION_RETRY_DELAY_MS: u64 = 250;

type RetryTimers = HashMap<JsString, Arc<()>>;

fn retry_timers() -> &'static Mutex<RetryTimers> {
    static TIMERS: OnceLock<Mutex<RetryTimers>> = OnceLock::new();
    TIMERS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn schedule_dead_detached_reconciliation_retry(control_db_path: &JsString, artifact_root: Option<&JsString>) {
    let mut timers = retry_timers().lock().expect("retry timers mutex");
    if timers.contains_key(control_db_path) {
        return;
    }
    let timer = Arc::new(());
    timers.insert(control_db_path.clone(), timer.clone());
    drop(timers);
    let path = control_db_path.clone();
    let artifact_root = artifact_root.cloned();
    std::thread::spawn(move || run_dead_detached_reconciliation_retry(path, artifact_root, timer));
}

fn run_dead_detached_reconciliation_retry(path: JsString, artifact_root: Option<JsString>, timer: Arc<()>) {
    std::thread::sleep(Duration::from_millis(DEAD_DETACHED_RECONCILIATION_RETRY_DELAY_MS));
    let mut timers = retry_timers().lock().expect("retry timers mutex");
    let is_pending = timers.get(&path).is_some_and(|pending| Arc::ptr_eq(pending, &timer));
    if !is_pending {
        return;
    }
    timers.remove(&path);
    drop(timers);
    if let Err(error) = LifecycleCoordinator::reconcile_dead_detached_runtimes(
        &path,
        &iso_time::now_iso_string(),
        artifact_root.as_ref(),
    ) {
        if let Some(native) = error.native_error() {
            crate::utils::uncaught::throw_native_uncaught(native.clone());
        }
        crate::utils::uncaught::throw_uncaught("Error", error.message().clone());
    }
}

fn clear_dead_detached_reconciliation_retry(control_db_path: &JsString) {
    retry_timers()
        .lock()
        .expect("retry timers mutex")
        .remove(control_db_path);
}

#[derive(Clone)]
pub struct LifecycleCoordinatorOptions {
    pub control_db_path: JsString,
    pub create_agent_id: Arc<dyn Fn() -> Result<JsString, ControlDbError> + Send + Sync>,
    pub now: Arc<dyn Fn() -> String + Send + Sync>,
    pub process_identity: ProcessIdentity,
    pub session_path: JsString,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrepareChildCommandInput {
    pub spawn: SpawnAgent,
    pub agent_id: Option<JsString>,
    pub detached: Option<bool>,
    pub result: Option<AgentResult>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OwnedLifecycleCommandInput {
    pub agent: AgentSnapshot,
    pub ownership: MultiAgentRuntimeOwnership,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SteeringDeliveryCommandInput {
    pub agent: AgentSnapshot,
    pub ownership: MultiAgentRuntimeOwnership,
    pub message_id: JsString,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SteeringCommandInput {
    pub agent: AgentSnapshot,
    pub ownership: MultiAgentRuntimeOwnership,
    pub recipient: RuntimeMailboxAddress,
    pub from_agent_id: JsString,
    pub body: JsString,
    pub target_checkpoint: Option<SteeringCheckpoint>,
    pub thread_id: Option<JsString>,
    pub file_refs: Option<JsValue>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DetachedCancellationCommandInput {
    pub agent: AgentSnapshot,
    pub ownership: MultiAgentRuntimeOwnership,
    pub output_label: JsString,
    pub reason: Option<JsString>,
    pub suppress_terminal_notification: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecoverDeadChildCommandInput {
    pub agent: AgentSnapshot,
    pub ownership: MultiAgentRuntimeOwnership,
    pub owner_session_id: JsString,
    pub supervisor_agent_id: Option<JsString>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FinalizeChildCommandInput {
    pub agent: AgentSnapshot,
    pub ownership: MultiAgentRuntimeOwnership,
    pub error: Option<AgentError>,
    pub result: Option<AgentResult>,
    pub terminal_lifecycle: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LifecycleCommandResult {
    Ok { agent: Box<AgentSnapshot> },
    Err { error: LifecycleCommandError },
}

#[derive(Debug, Clone, PartialEq)]
pub enum SteeringCommandResult {
    Ok {
        agent: Box<AgentSnapshot>,
        message: Box<AgentMailboxMessage>,
    },
    Err {
        error: SteeringCommandError,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum RecoverDeadChildCommandResult {
    Ok { agent: Box<AgentSnapshot> },
    Err { error: RecoverDeadRuntimeError },
}

pub struct LifecycleCoordinator {
    options: LifecycleCoordinatorOptions,
}

impl LifecycleCoordinator {
    pub fn reconcile_dead_detached_runtimes(
        control_db_path: &JsString,
        now_iso: &str,
        artifact_root: Option<&JsString>,
    ) -> Result<i64, ControlDbError> {
        match reconcile_dead_detached_agent_runtimes(control_db_path, now_iso) {
            Ok(reconciled) => {
                clear_dead_detached_reconciliation_retry(control_db_path);
                if reconciled > 0 {
                    crate::core::detached_job_cleanup::run_detached_job_artifact_cleanup(
                        control_db_path,
                        artifact_root,
                        Some(pi_js_compat::date::date_parse(now_iso)),
                    )?;
                }
                Ok(reconciled)
            }
            Err(error) if error.is_contention() => {
                schedule_dead_detached_reconciliation_retry(control_db_path, artifact_root);
                Ok(0)
            }
            Err(error) => Err(error),
        }
    }

    pub fn new(options: LifecycleCoordinatorOptions) -> Self {
        Self { options }
    }

    pub fn create_attachment(&self, input: &SpawnAgent) -> Result<CreateMultiAgentAttachmentResult, ControlDbError> {
        let now_iso = self.now();
        let mut agent = snapshot_from_spawn(input, now_iso.clone(), (self.options.create_agent_id)()?);
        agent.lifecycle = crate::core::multi_agent_store::AgentLifecycle::WaitingForInput;
        agent.origin = Some(crate::core::multi_agent_store::AgentOrigin::Attached);
        agent.revision = 1;
        create_multi_agent_attachment(
            &self.options.control_db_path,
            &CreateMultiAgentAttachmentInput {
                agent_id: agent.id.clone(),
                agent,
                now_iso,
                session_path: self.options.session_path.clone(),
            },
        )
    }

    pub fn acquire_attached_runtime(
        &self,
        agent: &AgentSnapshot,
        owner_session_id: &JsString,
    ) -> Result<AcquireAttachedRuntimeOwnershipResult, ControlDbError> {
        acquire_attached_runtime_ownership(
            &self.options.control_db_path,
            &AcquireAttachedRuntimeOwnershipInput {
                session_path: self.options.session_path.clone(),
                agent_id: agent.id.clone(),
                process_identity: self.options.process_identity.clone(),
                owner: MultiAgentRuntimeOwnerIdentity {
                    session_id: owner_session_id.to_owned(),
                    agent_id: None,
                },
                now_iso: self.now(),
                supervisor: SupervisorRuntimeOwnership {
                    agent_id: None,
                    process_identity: self.options.process_identity.clone(),
                    session_id: owner_session_id.to_owned(),
                },
            },
        )
    }

    pub fn confirm_child_runtime(
        &self,
        input: &OwnedLifecycleCommandInput,
    ) -> Result<LifecycleCommandResult, ControlDbError> {
        self.commit_reserved_lifecycle(input, "running", None, true)
    }

    pub fn acknowledge_steering_delivery(
        &self,
        input: &SteeringDeliveryCommandInput,
    ) -> Result<SteeringCommandResult, ControlDbError> {
        let Some(identity) = self.read_ownership_identity(&input.ownership, &input.agent.id, true) else {
            return Ok(SteeringCommandResult::Err {
                error: SteeringCommandError::MutationMismatch,
            });
        };
        Ok(commit_multi_agent_steering_delivery(
            &self.options.control_db_path,
            &CommitMultiAgentSteeringDeliveryInput {
                session_path: self.options.session_path.clone(),
                agent_id: input.agent.id.clone(),
                process_identity: identity.process_identity,
                owner: identity.owner,
                requested_lifecycle: "running".to_owned(),
                updated_at: self.now(),
                message_id: input.message_id.clone(),
            },
        )?
        .into())
    }

    pub fn request_steering(&self, input: &SteeringCommandInput) -> Result<SteeringCommandResult, ControlDbError> {
        let Some(identity) = self.read_ownership_identity(&input.ownership, &input.agent.id, true) else {
            return Ok(SteeringCommandResult::Err {
                error: SteeringCommandError::MutationMismatch,
            });
        };
        Ok(commit_multi_agent_steering_mutation(
            &self.options.control_db_path,
            &CommitMultiAgentSteeringMutationInput {
                session_path: self.options.session_path.clone(),
                agent_id: input.agent.id.clone(),
                process_identity: identity.process_identity,
                owner: identity.owner,
                requested_lifecycle: "steering_pending".to_owned(),
                updated_at: self.now(),
                body: input.body.clone(),
                file_refs: input.file_refs.clone(),
                from_agent_id: input.from_agent_id.clone(),
                recipient: input.recipient.clone(),
                target_checkpoint: input.target_checkpoint,
                thread_id: input.thread_id.clone(),
            },
        )?
        .into())
    }

    pub fn mark_waiting_for_input(
        &self,
        input: &OwnedLifecycleCommandInput,
    ) -> Result<LifecycleCommandResult, ControlDbError> {
        self.commit_reserved_lifecycle(input, "waiting_for_input", None, true)
    }

    /// Mark an owned background job as detached from its waiting tool call so its
    /// terminal state notifies the supervisor through the runtime mailbox.
    pub fn mark_detached(&self, input: &OwnedLifecycleCommandInput) -> Result<LifecycleCommandResult, ControlDbError> {
        // Detached job ownership records the runner's process identity, not this
        // supervisor's, so the mutation is fenced by the full owner predicate only.
        let Some(identity) = self.read_ownership_identity(&input.ownership, &input.agent.id, false) else {
            return Ok(LifecycleCommandResult::Err {
                error: LifecycleCommandError::MutationMismatch,
            });
        };
        match commit_multi_agent_detach_mark(
            &self.options.control_db_path,
            &CommitMultiAgentDetachMarkInput {
                agent_id: input.agent.id.clone(),
                owner: identity.owner,
                process_identity: identity.process_identity,
                session_path: self.options.session_path.clone(),
                updated_at: self.now(),
            },
        )? {
            CommitMultiAgentDetachMarkResult::Ok { agent } => Ok(LifecycleCommandResult::Ok { agent }),
            CommitMultiAgentDetachMarkResult::Err { error } => Ok(LifecycleCommandResult::Err { error }),
        }
    }

    pub fn request_cancellation(
        &self,
        input: &OwnedLifecycleCommandInput,
    ) -> Result<LifecycleCommandResult, ControlDbError> {
        self.commit_reserved_lifecycle(input, "cancelling", None, true)
    }

    pub fn request_detached_cancellation(
        &self,
        input: &DetachedCancellationCommandInput,
    ) -> Result<LifecycleCommandResult, ControlDbError> {
        self.commit_reserved_lifecycle(
            &OwnedLifecycleCommandInput {
                agent: input.agent.clone(),
                ownership: input.ownership.clone(),
            },
            "cancelling",
            Some(crate::core::session_control_db::agents::DetachedCancellation {
                output_label: input.output_label.clone(),
                reason: input.reason.clone(),
                suppress_terminal_notification: input.suppress_terminal_notification,
            }),
            false,
        )
    }

    pub fn acknowledge_cancellation(
        &self,
        input: &OwnedLifecycleCommandInput,
    ) -> Result<LifecycleCommandResult, ControlDbError> {
        self.finalize_child(&FinalizeChildCommandInput {
            agent: input.agent.clone(),
            ownership: input.ownership.clone(),
            error: None,
            result: None,
            terminal_lifecycle: "aborted".to_owned(),
        })
    }

    pub fn recover_dead_child(
        &self,
        input: &RecoverDeadChildCommandInput,
    ) -> Result<RecoverDeadChildCommandResult, ControlDbError> {
        let Some(identity) = self.read_ownership_identity(&input.ownership, &input.agent.id, false) else {
            return Ok(RecoverDeadChildCommandResult::Err {
                error: RecoverDeadRuntimeError::MutationMismatch,
            });
        };
        // The recorded owner session may be a dead prior incarnation of this session
        // (every resume registers a fresh session ID), so owner.sessionId is not
        // required to equal the recovering supervisor's session. Authorization comes
        // from recoverDeadMultiAgentRuntime: the supervisor must be the live
        // registered listener asserted on this session path, and the owner process
        // must be provably dead before ownership is released in the same transaction.
        if identity.agent_id != input.agent.id
            || identity.session_path != self.options.session_path
            || identity.owner.agent_id != input.supervisor_agent_id
        {
            return Ok(RecoverDeadChildCommandResult::Err {
                error: RecoverDeadRuntimeError::MutationMismatch,
            });
        }
        let now_iso = self.now();
        let recovered = recover_dead_multi_agent_runtime(
            &self.options.control_db_path,
            &RecoverDeadMultiAgentRuntimeInput {
                expected_owner: identity,
                now_iso,
                supervisor: SupervisorRuntimeOwnership {
                    agent_id: input.supervisor_agent_id.clone(),
                    process_identity: self.options.process_identity.clone(),
                    session_id: input.owner_session_id.clone(),
                },
            },
        )?;
        match recovered {
            RecoverDeadMultiAgentRuntimeResult::Ok { agent, .. } => match js_json::from_value(&Value::Object(agent)) {
                Ok(agent) => Ok(RecoverDeadChildCommandResult::Ok { agent: Box::new(agent) }),
                Err(_) => Ok(RecoverDeadChildCommandResult::Err {
                    error: RecoverDeadRuntimeError::MutationMismatch,
                }),
            },
            RecoverDeadMultiAgentRuntimeResult::Err { error } => Ok(RecoverDeadChildCommandResult::Err { error }),
        }
    }

    pub fn finalize_child(&self, input: &FinalizeChildCommandInput) -> Result<LifecycleCommandResult, ControlDbError> {
        self.finalize_owned_child(input, true)
    }

    pub fn fail_detached_launch(
        &self,
        input: &FinalizeChildCommandInput,
    ) -> Result<LifecycleCommandResult, ControlDbError> {
        self.finalize_owned_child(input, false)
    }

    fn finalize_owned_child(
        &self,
        input: &FinalizeChildCommandInput,
        require_current_process: bool,
    ) -> Result<LifecycleCommandResult, ControlDbError> {
        let Some(identity) = self.read_ownership_identity(&input.ownership, &input.agent.id, require_current_process)
        else {
            return Ok(LifecycleCommandResult::Err {
                error: LifecycleCommandError::MutationMismatch,
            });
        };
        let updated_at = self.now();
        let result = commit_multi_agent_terminal_mutation(
            &self.options.control_db_path,
            &CommitMultiAgentTerminalMutationInput {
                session_path: self.options.session_path.clone(),
                agent_id: input.agent.id.clone(),
                process_identity: identity.process_identity,
                owner: identity.owner,
                terminal_lifecycle: input.terminal_lifecycle.clone(),
                event_kind: input.terminal_lifecycle.clone(),
                agent_details: AgentDetails {
                    error: input
                        .error
                        .as_ref()
                        .map(js_json::to_value)
                        .transpose()
                        .map_err(|error| ControlDbError::new(error.to_string()))?,
                    result: input
                        .result
                        .as_ref()
                        .map(js_json::to_value)
                        .transpose()
                        .map_err(|error| ControlDbError::new(error.to_string()))?,
                },
                updated_at,
            },
        )?;
        match result {
            crate::core::session_control_db::agents::CommitMultiAgentTerminalMutationResult::Err { error } => {
                Ok(LifecycleCommandResult::Err { error })
            }
            crate::core::session_control_db::agents::CommitMultiAgentTerminalMutationResult::Ok { .. } => {
                match read_multi_agent_agent(
                    &self.options.control_db_path,
                    &self.options.session_path,
                    &input.agent.id,
                )? {
                    Some(committed) => Ok(LifecycleCommandResult::Ok {
                        agent: Box::new(committed),
                    }),
                    None => Ok(LifecycleCommandResult::Err {
                        error: LifecycleCommandError::AgentNotFound,
                    }),
                }
            }
        }
    }

    pub fn prepare_child(&self, input: &PrepareChildCommandInput) -> Result<AgentSnapshot, ControlDbError> {
        self.build_child_snapshot(input)
    }

    pub fn commit_failed_child(
        &self,
        agent: &AgentSnapshot,
        error: AgentError,
    ) -> Result<CreateFailedMultiAgentChildResult, ControlDbError> {
        if agent.revision != 1 {
            return Err(ControlDbError::new(
                "Constructed child failure must persist at revision 1",
            ));
        }
        let now_iso = self.now();
        let mut failed_agent = agent.clone();
        failed_agent.error = Some(error);
        failed_agent.lifecycle = crate::core::multi_agent_store::AgentLifecycle::Failed;
        failed_agent.updated_at = now_iso.clone().into();
        create_failed_multi_agent_child(
            &self.options.control_db_path,
            &CreateFailedMultiAgentChildInput {
                agent: failed_agent,
                now_iso,
                session_path: self.options.session_path.clone(),
            },
        )
    }

    pub fn commit_running_child(
        &self,
        agent: &AgentSnapshot,
        owner_session_id: &JsString,
        process_identity: &ProcessIdentity,
        owner_agent_id: Option<JsString>,
    ) -> Result<CreateMultiAgentChildWithRuntimeOwnershipResult, ControlDbError> {
        if agent.lifecycle != crate::core::multi_agent_store::AgentLifecycle::Running || agent.revision != 1 {
            return Err(ControlDbError::new(
                "Constructed child must enter persistence as running revision 1",
            ));
        }
        self.persist_child_with_ownership(agent, owner_session_id, process_identity, owner_agent_id)
    }

    fn build_child_snapshot(&self, input: &PrepareChildCommandInput) -> Result<AgentSnapshot, ControlDbError> {
        let now_iso = self.now();
        let id = match &input.agent_id {
            Some(id) => id.clone(),
            None => (self.options.create_agent_id)()?,
        };
        let mut agent = snapshot_from_spawn(&input.spawn, now_iso, id);
        agent.lifecycle = crate::core::multi_agent_store::AgentLifecycle::Running;
        agent.revision = 1;
        agent.detached = input.detached;
        agent.result = input.result.clone();
        Ok(agent)
    }

    fn persist_child_with_ownership(
        &self,
        agent: &AgentSnapshot,
        owner_session_id: &JsString,
        process_identity: &ProcessIdentity,
        owner_agent_id: Option<JsString>,
    ) -> Result<CreateMultiAgentChildWithRuntimeOwnershipResult, ControlDbError> {
        create_multi_agent_child_with_runtime_ownership(
            &self.options.control_db_path,
            &CreateMultiAgentChildWithRuntimeOwnershipInput {
                session_path: self.options.session_path.clone(),
                agent_id: agent.id.clone(),
                process_identity: process_identity.clone(),
                owner: MultiAgentRuntimeOwnerIdentity {
                    session_id: owner_session_id.to_owned(),
                    agent_id: owner_agent_id,
                },
                now_iso: self.now(),
                agent: agent.clone(),
            },
        )
    }

    fn commit_reserved_lifecycle(
        &self,
        input: &OwnedLifecycleCommandInput,
        requested_lifecycle: &str,
        detached_cancellation: Option<crate::core::session_control_db::agents::DetachedCancellation>,
        require_current_process: bool,
    ) -> Result<LifecycleCommandResult, ControlDbError> {
        let Some(identity) = self.read_ownership_identity(&input.ownership, &input.agent.id, require_current_process)
        else {
            return Ok(LifecycleCommandResult::Err {
                error: LifecycleCommandError::MutationMismatch,
            });
        };
        let updated_at = self.now();
        let result = commit_multi_agent_lifecycle_mutation(
            &self.options.control_db_path,
            &CommitMultiAgentLifecycleMutationInput {
                session_path: self.options.session_path.clone(),
                agent_id: input.agent.id.clone(),
                process_identity: identity.process_identity,
                owner: identity.owner,
                requested_lifecycle: requested_lifecycle.to_owned(),
                updated_at,
                detached_cancellation,
            },
        )?;
        match result {
            crate::core::session_control_db::agents::CommitMultiAgentLifecycleMutationResult::Ok { agent } => {
                match js_json::from_value(&JsValue::Object(agent)) {
                    Ok(agent) => Ok(LifecycleCommandResult::Ok { agent: Box::new(agent) }),
                    Err(_) => Ok(LifecycleCommandResult::Err {
                        error: LifecycleCommandError::MutationMismatch,
                    }),
                }
            }
            crate::core::session_control_db::agents::CommitMultiAgentLifecycleMutationResult::Err { error } => {
                Ok(LifecycleCommandResult::Err { error })
            }
        }
    }

    fn read_ownership_identity(
        &self,
        ownership: &MultiAgentRuntimeOwnership,
        expected_agent_id: &JsString,
        require_current_process: bool,
    ) -> Option<MultiAgentRuntimeOwnershipIdentity> {
        if ownership.agent_id != *expected_agent_id
            || ownership.session_path != self.options.session_path
            || ownership.process_identity.is_none()
            || ownership.owner.session_id.is_none()
        {
            return None;
        }
        let process_identity = ownership.process_identity.clone()?;
        if require_current_process
            && (process_identity.pid != self.options.process_identity.pid
                || process_identity.start_time_ticks != self.options.process_identity.start_time_ticks)
        {
            return None;
        }
        Some(MultiAgentRuntimeOwnershipIdentity {
            agent_id: ownership.agent_id.clone(),
            owner: MultiAgentRuntimeOwnerIdentity {
                agent_id: ownership.owner.agent_id.clone(),
                session_id: ownership.owner.session_id.clone()?,
            },
            process_identity,
            session_path: ownership.session_path.clone(),
        })
    }

    fn now(&self) -> String {
        (self.options.now)()
    }
}

fn snapshot_from_spawn(input: &SpawnAgent, now_iso: String, agent_id: JsString) -> AgentSnapshot {
    AgentSnapshot {
        key_order: Vec::new(),
        account: input.account.clone(),
        agent_type: input.agent_type.clone(),
        created_at: now_iso.clone().into(),
        cwd: input.cwd.clone(),
        detached: None,
        display_name: input.display_name.clone(),
        event_stream: input.event_stream.clone(),
        id: agent_id,
        lifecycle: crate::core::multi_agent_store::AgentLifecycle::Running,
        model: input.model.clone(),
        origin: input.origin,
        parent_id: input.parent_id.clone().or_else(|| Some(MAIN_THREAD_AGENT_ID.into())),
        permission: input.permission.clone(),
        result: None,
        revision: 1,
        slot: input.slot.clone(),
        transcript: input.transcript.clone(),
        updated_at: now_iso.into(),
        worker: input.worker.clone(),
        worktree: input.worktree.clone(),
        current_activity: None,
        last_activity: None,
        error: None,
        extra: pi_js_compat::js_json::JsObject::new(),
    }
}

impl From<CommitMultiAgentSteeringMutationResult> for SteeringCommandResult {
    fn from(result: CommitMultiAgentSteeringMutationResult) -> Self {
        match result {
            CommitMultiAgentSteeringMutationResult::Ok { agent, message } => Self::Ok { agent, message },
            CommitMultiAgentSteeringMutationResult::Err { error } => Self::Err { error },
        }
    }
}

impl From<CommitMultiAgentSteeringDeliveryResult> for SteeringCommandResult {
    fn from(result: CommitMultiAgentSteeringDeliveryResult) -> Self {
        match result {
            CommitMultiAgentSteeringDeliveryResult::Ok { agent, message } => Self::Ok { agent, message },
            CommitMultiAgentSteeringDeliveryResult::Err { error } => Self::Err { error },
        }
    }
}
