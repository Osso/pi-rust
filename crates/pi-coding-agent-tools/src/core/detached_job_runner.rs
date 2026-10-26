//! Detached job artifacts and lifecycle controller (`core/detached-job-runner.ts`).

use std::fmt;
use std::fs::{DirBuilder, File};
use std::io::Read;
use std::os::unix::fs::DirBuilderExt;
use std::sync::Arc;

use crate::utils::paths::{node_basename, node_extname, node_join};
use pi_js_compat::js_format;
use pi_js_compat::js_json::{self, JsString, JsValue, encode_node_utf8};
use sha2::{Digest, Sha256};

use crate::core::detached_bash_runner::{DetachedBashLaunchManifestData, write_detached_bash_launch_manifest};
use crate::core::lifecycle_coordinator::{
    DetachedCancellationCommandInput, LifecycleCommandResult, LifecycleCoordinator, OwnedLifecycleCommandInput,
    PrepareChildCommandInput, RecoverDeadChildCommandInput, RecoverDeadChildCommandResult,
};
use crate::core::multi_agent_store::{
    AgentFileReference, AgentPermission, AgentResult, AgentSnapshot, AgentWorkerAdapter, AgentWorkerAdapterKind,
    MultiAgentStore, SpawnAgent, is_active_lifecycle,
};
use crate::core::runtime_process::{ProcessIdentity, is_process_identity_alive, read_process_identity};
use crate::core::session_control_db::{
    CreateMultiAgentChildWithRuntimeOwnershipResult, MultiAgentRuntimeOwnerIdentity, MultiAgentRuntimeOwnership,
    read_multi_agent_agent, read_multi_agent_runtime_ownership, read_multi_agent_state,
};
pub use crate::core::session_control_db::{
    DetachedJobOutcome, DetachedJobOwnershipIdentity, DetachedJobTerminalInput, DetachedJobTerminalOutput,
};

/// Output artifact paths for a detached job.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetachedJobArtifacts {
    pub directory: JsString,
    pub output_path: JsString,
}

/// A job id with artifacts already reserved on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReservedDetachedJob {
    pub artifacts: DetachedJobArtifacts,
    pub job_id: JsString,
}

/// Detached job kinds accepted by the lifecycle controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetachedJobAgentType {
    Bash,
    Pyrun,
}

impl DetachedJobAgentType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bash => "bash",
            Self::Pyrun => "pyrun",
        }
    }

    fn output_label(self) -> &'static str {
        match self {
            Self::Bash => "Bash output",
            Self::Pyrun => "Pyrun output",
        }
    }
}

/// A registered detached job with its persisted agent and ownership.
#[derive(Debug, Clone, PartialEq)]
pub struct DetachedJobOwnership {
    pub agent: AgentSnapshot,
    pub artifacts: DetachedJobArtifacts,
    pub control_ownership: MultiAgentRuntimeOwnership,
    pub identity: DetachedJobOwnershipIdentity,
}

/// Input for [`DetachedJobLifecycleController::register`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisterDetachedJobInput {
    pub agent_type: DetachedJobAgentType,
    pub cwd: JsString,
    pub display_name: JsString,
    pub job_id: JsString,
    pub process_identity: ProcessIdentity,
    pub worker_handle_id: JsString,
    pub tool_call_id: Option<JsString>,
    pub detached: Option<bool>,
    pub file_refs: Option<Vec<AgentFileReference>>,
}

/// Input for [`DetachedJobLifecycleController::launch_bash`].
#[derive(Debug, Clone, PartialEq)]
pub struct LaunchDetachedBashInput {
    pub args: Vec<pi_js_compat::js_json::JsString>,
    pub command: JsString,
    pub cwd: JsString,
    pub env: Vec<(JsString, JsString)>,
    pub timeout_ms: Option<f64>,
    pub tool_call_id: Option<JsString>,
}

/// A launched detached Bash runner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchedDetachedBashJob {
    pub activation_path: JsString,
    pub artifacts: DetachedJobArtifacts,
    pub foreground_completion_path: JsString,
    pub job_id: JsString,
    pub manifest_path: JsString,
    pub process_identity: ProcessIdentity,
    pub runner_pid: u32,
}

/// Durable contract implemented by detached runners.
pub trait DetachedJobRunnerContract: Send + Sync {
    fn artifacts(&self) -> &DetachedJobArtifacts;
    fn identity(&self) -> &DetachedJobOwnershipIdentity;
    fn cancel(&self, reason: Option<&JsString>);
}

/// Writes one launch manifest; injected by tests to force persistence failures.
pub type WriteBashLaunchManifest =
    Arc<dyn Fn(&JsString, &DetachedBashLaunchManifestData) -> Result<(), JsString> + Send + Sync>;

/// Construction options for [`DetachedJobLifecycleController`].
pub struct DetachedJobLifecycleControllerOptions {
    pub artifact_root: JsString,
    pub control_db_path: JsString,
    pub coordinator: Arc<LifecycleCoordinator>,
    pub owner_agent_id: Option<JsString>,
    pub owner_session_id: JsString,
    pub session_path: JsString,
    pub store: Arc<MultiAgentStore>,
    pub write_bash_launch_manifest: Option<WriteBashLaunchManifest>,
}

/// Detached job lifecycle controller.
#[derive(Clone)]
pub struct DetachedJobLifecycleController {
    options: Arc<DetachedJobLifecycleControllerOptions>,
}

#[derive(Debug)]
pub enum DetachedJobError {
    ArtifactExists(JsString),
    ControlDb(JsString),
    InvalidJobId,
    Launch(JsString),
    Serialization(String),
    Io(std::io::Error),
}

impl DetachedJobError {
    pub fn message(&self) -> JsString {
        match self {
            Self::ArtifactExists(directory) => {
                js_format!("Detached job artifact directory already exists: {}", directory)
            }
            Self::ControlDb(message) | Self::Launch(message) => message.clone(),
            Self::InvalidJobId => "Detached job ID must be one path segment".into(),
            Self::Serialization(message) => message.into(),
            Self::Io(source) => source.to_string().into(),
        }
    }
}
impl fmt::Display for DetachedJobError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&encode_node_utf8(&self.message()))
    }
}

impl std::error::Error for DetachedJobError {}

impl From<crate::core::multi_agent_store::MultiAgentStoreError> for DetachedJobError {
    fn from(error: crate::core::multi_agent_store::MultiAgentStoreError) -> Self {
        Self::ControlDb(error.message)
    }
}

impl From<std::io::Error> for DetachedJobError {
    fn from(source: std::io::Error) -> Self {
        Self::Io(source)
    }
}

impl From<serde_json::Error> for DetachedJobError {
    fn from(source: serde_json::Error) -> Self {
        Self::Serialization(source.to_string())
    }
}

impl From<crate::core::sqlite::ControlDbError> for DetachedJobError {
    fn from(source: crate::core::sqlite::ControlDbError) -> Self {
        Self::ControlDb(source.message().clone())
    }
}

/// `createDetachedJobArtifacts`.
pub fn create_detached_job_artifacts(
    root_directory: impl Into<JsString>,
    job_id: impl Into<JsString>,
) -> Result<DetachedJobArtifacts, DetachedJobError> {
    let directory = detached_job_directory(root_directory, job_id)?;
    DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(encode_node_utf8(&directory))?;
    Ok(DetachedJobArtifacts {
        output_path: node_join(&directory, "output.log"),
        directory,
    })
}

/// `reserveDetachedJobArtifacts`.
pub fn reserve_detached_job_artifacts(
    root_directory: impl Into<JsString>,
    job_id: impl Into<JsString>,
) -> Result<DetachedJobArtifacts, DetachedJobError> {
    let root_directory = root_directory.into();
    let directory = detached_job_directory(&root_directory, job_id)?;
    DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(encode_node_utf8(&root_directory))?;
    match DirBuilder::new().mode(0o700).create(encode_node_utf8(&directory)) {
        Ok(()) => {}
        Err(source) if source.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(DetachedJobError::ArtifactExists(directory));
        }
        Err(source) => return Err(DetachedJobError::Io(source)),
    }
    Ok(DetachedJobArtifacts {
        output_path: node_join(&directory, "output.log"),
        directory,
    })
}

fn detached_job_directory(
    root_directory: impl Into<JsString>,
    job_id: impl Into<JsString>,
) -> Result<JsString, DetachedJobError> {
    let job_id = job_id.into();
    if job_id.is_empty() || job_id.contains('/') || job_id.contains('\\') {
        return Err(DetachedJobError::InvalidJobId);
    }
    Ok(node_join(root_directory, job_id))
}

/// `createDetachedJobTerminalInput`.
pub fn create_detached_job_terminal_input(
    artifacts: &DetachedJobArtifacts,
    identity: &DetachedJobOwnershipIdentity,
    outcome: DetachedJobOutcome,
    terminal_at: &JsString,
    duration_ms: Option<i64>,
    tool_call_id: Option<&JsString>,
) -> Result<DetachedJobTerminalInput, DetachedJobError> {
    fsync_path(&artifacts.output_path)?;
    let data = read_file(&artifacts.output_path)?;
    Ok(DetachedJobTerminalInput {
        job_id: identity.job_id.clone(),
        owner: identity.owner.clone(),
        process_identity: identity.process_identity.clone(),
        output_label: identity.output_label.clone(),
        terminal_at: terminal_at.to_owned(),
        tool_call_id: tool_call_id.cloned(),
        duration_ms,
        outcome,
        output: DetachedJobTerminalOutput {
            label: identity.output_label.clone(),
            path: artifacts.output_path.clone(),
            sha256: sha256_hex(&data),
            size: data.len() as i64,
        },
    })
}

pub fn sha256_hex(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

fn fsync_path(path: &JsString) -> Result<(), DetachedJobError> {
    File::open(encode_node_utf8(path))?.sync_all()?;
    Ok(())
}

fn read_file(path: &JsString) -> Result<Vec<u8>, DetachedJobError> {
    let mut data = Vec::new();
    File::open(encode_node_utf8(path))?.read_to_end(&mut data)?;
    Ok(data)
}

impl DetachedJobLifecycleController {
    pub fn new(options: DetachedJobLifecycleControllerOptions) -> Self {
        Self {
            options: Arc::new(options),
        }
    }

    pub fn allocate_job_id(&self, agent_type: DetachedJobAgentType) -> Result<JsString, DetachedJobError> {
        self.options
            .store
            .allocate_agent_id_for_lifecycle_coordinator(agent_type.as_str())
            .map_err(|error| DetachedJobError::ControlDb(error.message))
    }

    pub fn cancel(
        &self,
        ownership: &DetachedJobOwnership,
        reason: Option<&JsString>,
    ) -> Result<LifecycleCommandResult, DetachedJobError> {
        let cancelled = self
            .options
            .coordinator
            .request_detached_cancellation(&DetachedCancellationCommandInput {
                agent: ownership.agent.clone(),
                ownership: ownership.control_ownership.clone(),
                output_label: ownership.identity.output_label.clone(),
                reason: reason.cloned(),
                suppress_terminal_notification: None,
            })?;
        if let LifecycleCommandResult::Ok { agent } = &cancelled {
            self.options.store.publish_lifecycle_coordinator_snapshot(agent)?;
        }
        Ok(cancelled)
    }

    pub fn create_artifacts(&self, job_id: &JsString) -> Result<DetachedJobArtifacts, DetachedJobError> {
        reserve_detached_job_artifacts(&self.artifact_root()?, job_id)
    }

    pub fn find_bash_job_by_tool_call_id(
        &self,
        tool_call_id: &JsString,
    ) -> Result<Option<AgentSnapshot>, DetachedJobError> {
        find_persisted_bash_job(self.options.as_ref(), tool_call_id)
    }

    pub fn launch_bash(&self, input: &LaunchDetachedBashInput) -> Result<LaunchedDetachedBashJob, DetachedJobError> {
        launch_detached_bash_job(self.options.as_ref(), input)
    }

    pub fn mark_detached(&self, ownership: &DetachedJobOwnership) -> Result<LifecycleCommandResult, DetachedJobError> {
        let marked = self.options.coordinator.mark_detached(&OwnedLifecycleCommandInput {
            agent: ownership.agent.clone(),
            ownership: ownership.control_ownership.clone(),
        })?;
        if let LifecycleCommandResult::Ok { agent } = &marked {
            self.options.store.publish_lifecycle_coordinator_snapshot(agent)?;
        }
        Ok(marked)
    }

    pub fn observe(&self, job_id: &JsString) -> Result<Option<AgentSnapshot>, DetachedJobError> {
        observe_detached_job(self.options.as_ref(), job_id)
    }

    pub fn register(&self, input: &RegisterDetachedJobInput) -> Result<DetachedJobOwnership, DetachedJobError> {
        register_detached_job(self.options.as_ref(), input)
    }

    pub fn reserve_job(&self, agent_type: DetachedJobAgentType) -> Result<ReservedDetachedJob, DetachedJobError> {
        reserve_detached_job(self.options.as_ref(), agent_type)
    }

    fn artifact_root(&self) -> Result<JsString, DetachedJobError> {
        detached_job_artifact_root(&self.options.artifact_root, &self.options.session_path)
    }
}

/// `detachedJobArtifactRoot`.
fn detached_job_artifact_root(artifact_root: &JsString, session_path: &JsString) -> Result<JsString, DetachedJobError> {
    let session_file_name = node_basename(session_path);
    let extension = node_extname(&session_file_name);
    let session_name = session_file_name.slice(0, session_file_name.len() - extension.len());
    if session_name.is_empty() {
        return Err(DetachedJobError::Launch(
            "Detached job session path must have a file name".into(),
        ));
    }
    Ok(node_join(node_join(artifact_root, "detached-jobs"), session_name))
}

fn find_persisted_bash_job(
    options: &DetachedJobLifecycleControllerOptions,
    tool_call_id: &JsString,
) -> Result<Option<AgentSnapshot>, DetachedJobError> {
    let parent_id = options.owner_agent_id.clone().unwrap_or_else(|| "main".into());
    let Some(state) = read_multi_agent_state(&options.control_db_path, &options.session_path)? else {
        return Ok(None);
    };
    let mut candidates = Vec::new();
    for value in state.agents {
        let agent: AgentSnapshot = js_json::from_value(&value)?;
        if !bash_job_matches(&agent, &parent_id, tool_call_id) {
            continue;
        }
        candidates.push(agent);
    }
    candidates.sort_by(compare_persisted_bash_jobs);
    Ok(candidates.into_iter().next())
}

fn bash_job_matches(agent: &AgentSnapshot, parent_id: &JsString, tool_call_id: &JsString) -> bool {
    if agent.parent_id.as_ref() != Some(parent_id) {
        return false;
    }
    let has_bash_output = agent.result.as_ref().is_some_and(|result| {
        result.file_refs.as_ref().is_some_and(|file_refs| {
            file_refs
                .iter()
                .any(|file_ref| file_ref.label.as_ref().is_some_and(|label| label == "Bash output"))
        })
    });
    if !has_bash_output {
        return false;
    }
    let worker_matches = agent.worker.as_ref().and_then(|worker| worker.tool_call_id.as_ref()) == Some(tool_call_id);
    let result_matches = agent.result.as_ref().and_then(|result| result.tool_call_id.as_ref()) == Some(tool_call_id);
    worker_matches || result_matches
}

fn compare_persisted_bash_jobs(left: &AgentSnapshot, right: &AgentSnapshot) -> std::cmp::Ordering {
    let active_difference =
        i32::from(is_active_lifecycle(right.lifecycle)) - i32::from(is_active_lifecycle(left.lifecycle));
    if active_difference != 0 {
        return active_difference.cmp(&0);
    }
    pi_js_compat::collator::locale_compare_utf16(right.updated_at.units(), left.updated_at.units())
}

fn launch_detached_bash_job(
    options: &DetachedJobLifecycleControllerOptions,
    input: &LaunchDetachedBashInput,
) -> Result<LaunchedDetachedBashJob, DetachedJobError> {
    let reserved = reserve_detached_job(options, DetachedJobAgentType::Bash)?;
    let artifacts = reserved.artifacts;
    let activation_path = node_join(&artifacts.directory, "activation.json");
    let foreground_completion_path = node_join(&artifacts.directory, "foreground-completed");
    let manifest_path = node_join(&artifacts.directory, "launch.json");
    let runner_pid = crate::core::detached_bash_runner::launch_detached_bash_runner(&manifest_path)
        .map_err(|error| DetachedJobError::Launch(error.message().clone()))?;
    let process_identity =
        read_process_identity(runner_pid).map_err(|error| DetachedJobError::Launch(error.message.clone()))?;
    let manifest = DetachedBashLaunchManifestData {
        activation_path: activation_path.clone(),
        args: input.args.clone(),
        artifacts: artifacts.clone(),
        command: input.command.clone(),
        control_db_path: options.control_db_path.clone(),
        cwd: input.cwd.clone(),
        env: input
            .env
            .iter()
            .map(|(name, value)| (name.clone(), JsValue::String(value.clone())))
            .collect(),
        foreground_completion_path: foreground_completion_path.clone(),
        runner_address: crate::core::session_control_db::RuntimeMailboxAddress {
            agent_id: Some(reserved.job_id.clone()),
            session_id: options.owner_session_id.clone(),
        },
        session_path: options.session_path.clone(),
        timeout_ms: input.timeout_ms,
        tool_call_id: input.tool_call_id.clone(),
    };
    let write_manifest = options.write_bash_launch_manifest.as_ref();
    let write_result = match write_manifest {
        Some(write_manifest) => write_manifest(&manifest_path, &manifest),
        None => write_detached_bash_launch_manifest(&manifest_path, &manifest).map_err(|error| error.message().clone()),
    };
    if let Err(message) = write_result {
        let _ =
            crate::core::detached_bash_runner::terminate_foreground_runner(runner_pid, rustix::process::Signal::KILL);
        return Err(DetachedJobError::Launch(message));
    }
    Ok(LaunchedDetachedBashJob {
        activation_path,
        artifacts,
        foreground_completion_path,
        job_id: reserved.job_id,
        manifest_path,
        process_identity,
        runner_pid,
    })
}

fn reserve_detached_job(
    options: &DetachedJobLifecycleControllerOptions,
    agent_type: DetachedJobAgentType,
) -> Result<ReservedDetachedJob, DetachedJobError> {
    let artifact_root = detached_job_artifact_root(&options.artifact_root, &options.session_path)?;
    loop {
        let job_id = options
            .store
            .allocate_agent_id_for_lifecycle_coordinator(agent_type.as_str())
            .map_err(|error| DetachedJobError::ControlDb(error.message))?;
        match reserve_detached_job_artifacts(&artifact_root, &job_id) {
            Ok(artifacts) => return Ok(ReservedDetachedJob { artifacts, job_id }),
            Err(DetachedJobError::ArtifactExists(_)) => continue,
            Err(error) => return Err(error),
        }
    }
}

fn observe_detached_job(
    options: &DetachedJobLifecycleControllerOptions,
    job_id: &JsString,
) -> Result<Option<AgentSnapshot>, DetachedJobError> {
    let Some(mut agent) = read_multi_agent_agent(&options.control_db_path, &options.session_path, job_id)? else {
        return Ok(None);
    };
    let ownership = read_multi_agent_runtime_ownership(&options.control_db_path, &options.session_path, job_id)?;
    if should_recover_dead_child(options, job_id, &agent, ownership.as_ref()) {
        let ownership = ownership.expect("ownership checked by should_recover_dead_child");
        let recovered = options.coordinator.recover_dead_child(&RecoverDeadChildCommandInput {
            agent: agent.clone(),
            ownership,
            owner_session_id: options.owner_session_id.clone(),
            supervisor_agent_id: options.owner_agent_id.clone(),
        })?;
        if let RecoverDeadChildCommandResult::Ok { agent: recovered_agent } = recovered {
            agent = *recovered_agent;
        }
    }
    options.store.publish_lifecycle_coordinator_snapshot(&agent)?;
    Ok(Some(agent))
}

fn should_recover_dead_child(
    options: &DetachedJobLifecycleControllerOptions,
    job_id: &JsString,
    agent: &AgentSnapshot,
    ownership: Option<&MultiAgentRuntimeOwnership>,
) -> bool {
    if !is_active_lifecycle(agent.lifecycle) {
        return false;
    }
    let Some(ownership) = ownership else {
        return false;
    };
    let Some(process_identity) = ownership.process_identity.as_ref() else {
        return false;
    };
    ownership.session_path == options.session_path
        && &ownership.agent_id == job_id
        && ownership.owner.agent_id == options.owner_agent_id
        && ownership.owner.session_id.as_ref() == Some(&options.owner_session_id)
        && !is_process_identity_alive(process_identity)
}

fn register_detached_job(
    options: &DetachedJobLifecycleControllerOptions,
    input: &RegisterDetachedJobInput,
) -> Result<DetachedJobOwnership, DetachedJobError> {
    let artifacts = create_detached_job_artifacts(
        &detached_job_artifact_root(&options.artifact_root, &options.session_path)?,
        &input.job_id,
    )?;
    let output_label = input.agent_type.output_label();
    let mut file_refs = vec![AgentFileReference {
        label: Some(output_label.into()),
        path: artifacts.output_path.clone(),
    }];
    if let Some(extra_refs) = &input.file_refs {
        file_refs.extend(extra_refs.iter().cloned());
    }
    let prepared = options.coordinator.prepare_child(&PrepareChildCommandInput {
        spawn: SpawnAgent {
            parent_id: options.owner_agent_id.clone(),
            display_name: input.display_name.clone(),
            agent_type: "background".into(),
            origin: None,
            cwd: input.cwd.clone(),
            permission: AgentPermission {
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
            worker: Some(AgentWorkerAdapter {
                adapter: AgentWorkerAdapterKind::Runtime,
                handle_id: input.worker_handle_id.clone(),
                cwd: Some(input.cwd.clone()),
                tool_call_id: input.tool_call_id.clone(),
            }),
        },
        agent_id: Some(input.job_id.clone()),
        detached: input.detached,
        result: Some(AgentResult {
            summary: None,
            file_refs: Some(file_refs),
            duration_ms: None,
            tool_call_id: None,
        }),
    })?;
    let created = options.coordinator.commit_running_child(
        &prepared,
        &options.owner_session_id,
        &input.process_identity,
        options.owner_agent_id.clone(),
    )?;
    let (agent, control_ownership) = match created {
        CreateMultiAgentChildWithRuntimeOwnershipResult::Ok { agent, ownership } => (*agent, *ownership),
        CreateMultiAgentChildWithRuntimeOwnershipResult::Err { error } => {
            return Err(DetachedJobError::Launch(js_format!(
                "Could not own detached {} job: {}",
                input.agent_type.as_str(),
                error.as_str()
            )));
        }
    };
    options.store.publish_lifecycle_coordinator_snapshot(&agent)?;
    let Some(process_identity) = control_ownership.process_identity.clone() else {
        return Err(DetachedJobError::Launch(
            "Detached job ownership identity is incomplete".into(),
        ));
    };
    let identity = DetachedJobOwnershipIdentity {
        job_id: input.job_id.clone(),
        owner: MultiAgentRuntimeOwnerIdentity {
            agent_id: control_ownership.owner.agent_id.clone(),
            session_id: control_ownership
                .owner
                .session_id
                .clone()
                .unwrap_or_else(|| options.owner_session_id.clone()),
        },
        process_identity,
        output_label: output_label.into(),
    };
    Ok(DetachedJobOwnership {
        agent,
        artifacts,
        control_ownership,
        identity,
    })
}
