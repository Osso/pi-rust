//! Port of `packages/coding-agent/extensions/pyrun/src/detached-runner.ts`.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pi_js_compat::js_json::{self, JsObject as Map, JsString, JsValue as Value};
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

use crate::core::detached_bash_runner::{
    append_artifact_record, finalize_detached_job_with_retry, hash_json, is_process_identity_alive, read_json_lines,
    retain_control_db_connection,
};
use crate::core::detached_job_control::{
    DetachedJobCancelCommand, DetachedJobResponseCommand, DetachedJobRuntimeCommand,
    claim_detached_job_runtime_commands, enqueue_detached_job_status_response,
};
use crate::core::detached_job_runner::{DetachedJobArtifacts, DetachedJobOutcome, create_detached_job_terminal_input};
use crate::core::extensions::types::BoxFuture;
use crate::core::session_control_db::{
    DetachedJobOwnershipIdentity, FinalizeDetachedJobInput, FinalizeDetachedJobResult, RuntimeMailboxAddress,
    finalize_detached_job,
};

use super::detached_bridge::{EnqueueDetachedPyrunBridgeRequestInput, enqueue_detached_pyrun_bridge_request};
use super::runner::{
    CanonicalPyrunEvalParams, CanonicalPyrunEvalResult, CanonicalPyrunProgressUpdate, PyrunRunnerClient,
    PyrunRunnerOptions,
};

pub const DETACHED_PYRUN_RUNNER_MODE: &str = "--internal-detached-pyrun-runner";
const DETACHED_PYRUN_LAUNCH_VERSION: f64 = 3.0;
const CONTROL_POLL_MS: u64 = 25;
const LAUNCH_MANIFEST_POLL_MS: u64 = 10;
const LAUNCH_MANIFEST_TIMEOUT_MS: u64 = 30_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetachedPyrunLaunchManifestData {
    pub activation_path: String,
    pub artifacts: DetachedJobArtifacts,
    pub bridge_request_path: String,
    pub bridge_response_path: String,
    pub control_db_path: String,
    pub foreground_completion_path: String,
    pub params: CanonicalPyrunEvalParams,
    pub runner_address: RuntimeMailboxAddress,
    pub runner_options: PyrunRunnerOptions,
    pub runner_process_identity: crate::core::runtime_process::ProcessIdentity,
    pub session_path: String,
    #[serde(serialize_with = "serialize_js_number")]
    pub started_at: f64,
    pub supervisor_process_identity: crate::core::runtime_process::ProcessIdentity,
    /// Absent in legacy manifests written before tool-call correlation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

/// `JSON.stringify(number)`: integral finite numbers use integer notation.
fn serialize_js_number<S>(value: &f64, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serde::Serialize::serialize(&pi_js_compat::number::js_number_value(*value), serializer)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetachedPyrunLaunchManifest {
    #[serde(flatten)]
    pub data: DetachedPyrunLaunchManifestData,
    #[serde(serialize_with = "serialize_js_number")]
    pub version: f64,
    pub checksum: String,
}

fn fsync_file(path: &str) -> Result<(), String> {
    let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())
}

fn fsync_directory(path: &str) -> Result<(), String> {
    let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())
}

/// `writeDetachedPyrunLaunchManifest`.
pub fn write_detached_pyrun_launch_manifest(path: &str, data: &DetachedPyrunLaunchManifestData) -> Result<(), String> {
    let mut unsigned = js_json::to_value(data).map_err(|error| error.to_string())?;
    let Some(object) = unsigned.as_object_mut() else {
        return Err("detached Pyrun manifest must serialize to an object".to_owned());
    };
    object.insert(
        "version".into(),
        pi_js_compat::number::js_number_value(DETACHED_PYRUN_LAUNCH_VERSION).into(),
    );
    let checksum = hash_json(&Value::Object(object.clone()));
    object.insert("checksum".into(), Value::String(checksum.into()));
    let temporary_path = format!("{path}.tmp");
    let serialized = format!("{}\n", js_json::stringify_lossless(&unsigned));
    write_private(&temporary_path, &serialized)?;
    fsync_file(&temporary_path)?;
    std::fs::rename(&temporary_path, path).map_err(|error| error.to_string())?;
    let directory = Path::new(path)
        .parent()
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    fsync_directory(&directory)
}

fn write_private(path: &str, content: &str) -> Result<(), String> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    use std::io::Write;
    let mut file = options.open(path).map_err(|error| error.to_string())?;
    file.write_all(content.as_bytes()).map_err(|error| error.to_string())
}

/// `writeDetachedPyrunActivation`.
pub fn write_detached_pyrun_activation(path: &str, identity: &DetachedJobOwnershipIdentity) -> Result<(), String> {
    let temporary_path = format!("{path}.tmp");
    let serialized = format!(
        "{}\n",
        serde_json::to_string(identity).map_err(|error| error.to_string())?
    );
    write_private(&temporary_path, &serialized)?;
    fsync_file(&temporary_path)?;
    std::fs::rename(&temporary_path, path).map_err(|error| error.to_string())?;
    let directory = Path::new(path)
        .parent()
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    fsync_directory(&directory)
}

/// `readDetachedPyrunActivation`.
pub fn read_detached_pyrun_activation(path: &str) -> Result<Option<DetachedJobOwnershipIdentity>, String> {
    if !Path::new(path).exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    let value = js_json::parse_lossless(&content).map_err(|error| error.to_string())?;
    js_json::from_value(&value).map(Some).map_err(|error| error.to_string())
}

/// `readDetachedPyrunLaunchManifest`.
pub fn read_detached_pyrun_launch_manifest(path: &str) -> Result<DetachedPyrunLaunchManifest, String> {
    let content = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    let parsed: Value = js_json::parse_lossless(&content).map_err(|error| error.to_string())?;
    let original = parsed.clone();
    let mut unsigned = parsed;
    let Some(object) = unsigned.as_object_mut() else {
        return Err(format!("Invalid detached Pyrun launch manifest: {path}"));
    };
    let checksum = object
        .shift_remove(&JsString::from("checksum"))
        .and_then(|checksum| checksum.as_string().and_then(|value| value.to_utf8().ok()));
    let version = object.get("version").and_then(Value::as_f64);
    let started_at = object.get("startedAt").and_then(Value::as_f64);
    let valid = version == Some(DETACHED_PYRUN_LAUNCH_VERSION)
        && started_at.is_some_and(f64::is_finite)
        && checksum.as_deref() == Some(hash_json(&unsigned).as_str());
    if !valid {
        return Err(format!("Invalid detached Pyrun launch manifest: {path}"));
    }
    js_json::from_value(&original).map_err(|error| error.to_string())
}

/// Rust replacement for the TypeScript entry-script invocation. The compiled
/// binary dispatches [`DETACHED_PYRUN_RUNNER_MODE`] itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetachedPyrunRunnerInvocation {
    pub args: Vec<String>,
    pub executable: String,
}

/// `getDetachedPyrunRunnerInvocation` options.
#[derive(Debug, Clone, Default)]
pub struct DetachedPyrunRunnerInvocationOptions {
    pub compiled: bool,
    pub entry_path: Option<String>,
    pub executable_path: Option<String>,
}

/// `getDetachedPyrunRunnerInvocation`.
pub fn get_detached_pyrun_runner_invocation(
    manifest_path: &str,
    options: &DetachedPyrunRunnerInvocationOptions,
) -> Result<DetachedPyrunRunnerInvocation, String> {
    let executable = match &options.executable_path {
        Some(executable) => executable.clone(),
        None => std::env::current_exe()
            .map_err(|error| error.to_string())?
            .to_string_lossy()
            .into_owned(),
    };
    if options.compiled && options.entry_path.is_none() {
        return Ok(DetachedPyrunRunnerInvocation {
            args: vec![DETACHED_PYRUN_RUNNER_MODE.to_owned(), manifest_path.to_owned()],
            executable,
        });
    }
    let Some(entry_path) = &options.entry_path else {
        return Err("Detached Pyrun runner entry path is required".to_owned());
    };
    let args = if Path::new(entry_path)
        .extension()
        .is_some_and(|extension| extension == "ts")
    {
        vec![
            "--experimental-strip-types".to_owned(),
            entry_path.clone(),
            manifest_path.to_owned(),
        ]
    } else {
        vec![entry_path.clone(), manifest_path.to_owned()]
    };
    Ok(DetachedPyrunRunnerInvocation { args, executable })
}

/// `launchDetachedPyrunRunner`.
pub fn launch_detached_pyrun_runner(manifest_path: &str) -> Result<u32, String> {
    let invocation = get_detached_pyrun_runner_invocation(
        manifest_path,
        &DetachedPyrunRunnerInvocationOptions {
            compiled: true,
            ..Default::default()
        },
    )?;
    let mut command = Command::new(&invocation.executable);
    command
        .args(&invocation.args)
        .current_dir(
            Path::new(manifest_path)
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from(".")),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if !cfg!(windows) {
        command.process_group(0);
    }
    let mut child = command
        .spawn()
        .map_err(|_| "Could not launch detached Pyrun runner".to_owned())?;
    let pid = child
        .id()
        .ok_or_else(|| "Could not launch detached Pyrun runner".to_owned())?;
    tokio::spawn(async move {
        let _ = child.wait().await;
    });
    Ok(pid)
}

struct PendingBridgeRequest {
    resolve: tokio::sync::oneshot::Sender<Result<Option<Value>, String>>,
}

type PendingRequests = Arc<Mutex<std::collections::HashMap<String, PendingBridgeRequest>>>;

struct ControlState {
    cancel: Option<DetachedJobCancelCommand>,
    identity: Option<DetachedJobOwnershipIdentity>,
}

struct PyrunSettlement {
    cancel: Option<DetachedJobCancelCommand>,
    identity: Option<DetachedJobOwnershipIdentity>,
    result: Option<CanonicalPyrunEvalResult>,
    error: Option<JsString>,
}

/// `runDetachedPyrunRunner`.
pub async fn run_detached_pyrun_runner(manifest_path: &str) -> Result<Option<i64>, String> {
    let manifest = wait_for_detached_pyrun_launch_manifest(manifest_path).await?;
    let release_control_db =
        retain_control_db_connection(&manifest.data.control_db_path).map_err(|error| error.to_string())?;
    let runner_options = PyrunRunnerOptions {
        detached: Some(false),
        ..manifest.data.runner_options.clone()
    };
    let runner = PyrunRunnerClient::new(runner_options);
    let outcome = run_detached_pyrun_settlement(&manifest, &runner).await;
    runner.dispose();
    release_control_db.release();
    outcome
}

async fn run_detached_pyrun_settlement(
    manifest: &DetachedPyrunLaunchManifest,
    runner: &PyrunRunnerClient,
) -> Result<Option<i64>, String> {
    let settlement = wait_for_detached_pyrun_settlement(manifest, runner).await?;
    append_pyrun_settlement_record(manifest, &settlement)?;
    let identity = match settlement.identity.clone() {
        Some(identity) => Some(identity),
        None => wait_for_pyrun_ownership_decision(manifest).await?,
    };
    let Some(identity) = identity else {
        return Ok(None);
    };
    finalize_detached_pyrun_settlement(manifest, settlement, identity)
        .await
        .map(Some)
}

async fn wait_for_detached_pyrun_launch_manifest(path: &str) -> Result<DetachedPyrunLaunchManifest, String> {
    let deadline = std::time::Instant::now() + Duration::from_millis(LAUNCH_MANIFEST_TIMEOUT_MS);
    while !Path::new(path).exists() {
        if std::time::Instant::now() >= deadline {
            return Err(format!("Timed out waiting for detached Pyrun launch manifest: {path}"));
        }
        tokio::time::sleep(Duration::from_millis(LAUNCH_MANIFEST_POLL_MS)).await;
    }
    read_detached_pyrun_launch_manifest(path)
}

async fn wait_for_detached_pyrun_settlement(
    manifest: &DetachedPyrunLaunchManifest,
    runner: &PyrunRunnerClient,
) -> Result<PyrunSettlement, String> {
    let abort = CancellationToken::new();
    let control = Arc::new(Mutex::new(ControlState {
        cancel: None,
        identity: None,
    }));
    let pending: PendingRequests = Arc::new(Mutex::new(std::collections::HashMap::new()));
    let dispatch_pi_request = {
        let manifest = manifest.clone();
        let control = Arc::clone(&control);
        let pending = Arc::clone(&pending);
        Arc::new(move |request: super::runner::PiRequest| {
            let manifest = manifest.clone();
            let control = Arc::clone(&control);
            let pending = Arc::clone(&pending);
            let identity = control
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .identity
                .clone();
            let future: BoxFuture<'static, Result<Option<Value>, crate::core::extensions::types::HandlerError>> =
                Box::pin(async move {
                    match identity {
                        Some(identity) => {
                            dispatch_detached_pyrun_bridge_request(&manifest, &identity, pending, request)
                                .await
                                .map_err(crate::core::extensions::types::HandlerError::new)
                        }
                        None => dispatch_foreground_pyrun_bridge_request(&manifest, pending, request)
                            .await
                            .map_err(crate::core::extensions::types::HandlerError::new),
                    }
                });
            future
        })
    };
    let manifest_for_progress = manifest.clone();
    let settled = runner.evaluate(
        manifest.data.params.clone(),
        Some(Arc::new(move |update: CanonicalPyrunProgressUpdate| {
            let record = pi_js_compat::js_json!({"kind": "progress", "update": update.raw});
            append_artifact_record(&manifest_for_progress.data.artifacts.output_path, &record)
                .map_err(|error| super::runner::PyrunRunnerError::new(error.to_string()))
        })),
        Some(abort.clone()),
        Some(dispatch_pi_request),
    );
    tokio::pin!(settled);
    loop {
        let result = tokio::select! {
            result = &mut settled => Some(result),
            () = tokio::time::sleep(Duration::from_millis(CONTROL_POLL_MS)) => None,
        };
        let identity = {
            let mut control = control.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            if control.identity.is_none() {
                control.identity = read_detached_pyrun_activation(&manifest.data.activation_path)?;
            }
            control.identity.clone()
        };
        if let Some(result) = result {
            let control = control.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            return Ok(match result {
                Ok(result) => PyrunSettlement {
                    cancel: control.cancel.clone(),
                    identity: control.identity.clone(),
                    result: Some(result),
                    error: None,
                },
                Err(error) => PyrunSettlement {
                    cancel: control.cancel.clone(),
                    identity: control.identity.clone(),
                    result: None,
                    error: Some(error.to_string().into()),
                },
            });
        }
        if let Some(identity) = identity {
            apply_detached_pyrun_runtime_commands(manifest, &control, &pending, &abort, &identity);
        }
    }
}

fn apply_detached_pyrun_runtime_commands(
    manifest: &DetachedPyrunLaunchManifest,
    control: &Arc<Mutex<ControlState>>,
    pending: &PendingRequests,
    abort: &CancellationToken,
    identity: &DetachedJobOwnershipIdentity,
) {
    let Ok(commands) =
        claim_detached_job_runtime_commands(&manifest.data.control_db_path, &manifest.data.runner_address, identity)
    else {
        return;
    };
    for command in commands {
        match command {
            DetachedJobRuntimeCommand::Response(response) => settle_bridge_response(pending, &response),
            DetachedJobRuntimeCommand::Status(status) => {
                let _ = enqueue_detached_job_status_response(
                    &crate::core::detached_job_control::EnqueueDetachedJobStatusResponseInput {
                        control_db_path: manifest.data.control_db_path.clone(),
                        identity: identity.clone(),
                        reply_to: status.reply_to.clone(),
                        request_id: status.request_id.clone(),
                        runner_address: manifest.data.runner_address.clone(),
                        session_path: manifest.data.session_path.clone(),
                        status: pi_js_compat::js_json!({
                            "outputPath": manifest.data.artifacts.output_path,
                            "pendingRequestCount": pending.lock().unwrap().len(),
                            "state": "running"
                        }),
                    },
                );
            }
            DetachedJobRuntimeCommand::Cancel(cancel) => {
                let mut control = control.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                if control.cancel.is_none() {
                    control.cancel = Some(cancel.clone());
                    control.identity = Some(cancel.identity.clone());
                    abort.cancel();
                }
            }
        }
    }
}

fn settle_bridge_response(pending: &PendingRequests, response: &DetachedJobResponseCommand) {
    let request = pending
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .remove(&response.request_id);
    if let Some(request) = request {
        let _ = request.resolve.send(match &response.error {
            Some(error) => Err(error.clone()),
            None => Ok(response.result.clone()),
        });
    }
}

async fn dispatch_detached_pyrun_bridge_request(
    manifest: &DetachedPyrunLaunchManifest,
    identity: &DetachedJobOwnershipIdentity,
    pending: PendingRequests,
    request: super::runner::PiRequest,
) -> Result<Option<Value>, String> {
    let request_id = enqueue_detached_pyrun_bridge_request(EnqueueDetachedPyrunBridgeRequestInput {
        control_db_path: manifest.data.control_db_path.clone(),
        identity: identity.clone(),
        method: request.method,
        params: request.params,
        runner_address: manifest.data.runner_address.clone(),
        session_path: manifest.data.session_path.clone(),
        supervisor_address: RuntimeMailboxAddress {
            agent_id: None,
            session_id: manifest.data.runner_address.session_id.clone(),
        },
        tool_call_id: manifest.data.tool_call_id.clone(),
    })?;
    let (sender, receiver) = tokio::sync::oneshot::channel();
    pending
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(request_id, PendingBridgeRequest { resolve: sender });
    receiver.await.map_err(|error| error.to_string())?
}

async fn dispatch_foreground_pyrun_bridge_request(
    manifest: &DetachedPyrunLaunchManifest,
    pending: PendingRequests,
    request: super::runner::PiRequest,
) -> Result<Option<Value>, String> {
    let request_id = pi_ai::utils::uuid::random_uuid();
    let mut record = Map::new();
    record.insert("method".into(), Value::String(request.method.clone().into()));
    record.insert("params".into(), request.params.clone());
    record.insert("requestId".into(), Value::String(request_id.clone().into()));
    append_artifact_record(&manifest.data.bridge_request_path, &Value::Object(record))
        .map_err(|error| error.to_string())?;
    let mut offset = 0;
    loop {
        let records =
            read_json_lines(&manifest.data.bridge_response_path, offset).map_err(|error| error.to_string())?;
        offset = records.offset;
        for response in &records.values {
            if response.get("requestId").and_then(Value::as_string) != Some(request_id.as_str()) {
                continue;
            }
            if response.get("claimed").and_then(Value::as_bool) == Some(true) {
                continue;
            }
            if let Some(error) = response
                .get("error")
                .and_then(Value::as_string)
                .filter(|error| !error.is_empty())
            {
                return Err(error.to_owned());
            }
            return Ok(response.get("result").cloned());
        }
        let identity = read_detached_pyrun_activation(&manifest.data.activation_path)?;
        if let Some(identity) = identity
            && claim_foreground_bridge_request(&manifest.data.artifacts.directory, &request_id)?
        {
            return dispatch_detached_pyrun_bridge_request(manifest, &identity, pending, request).await;
        }
        tokio::time::sleep(Duration::from_millis(CONTROL_POLL_MS)).await;
    }
}

/// `claimForegroundBridgeRequest` (identical in both TypeScript modules).
pub(crate) fn claim_foreground_bridge_request(directory: &str, request_id: &str) -> Result<bool, String> {
    let path = Path::new(directory).join(format!("bridge-claim-{request_id}"));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(error) => Err(error.to_string()),
    }
}

fn append_pyrun_settlement_record(
    manifest: &DetachedPyrunLaunchManifest,
    settlement: &PyrunSettlement,
) -> Result<(), String> {
    let record = match (&settlement.result, &settlement.error) {
        (Some(result), _) => pi_js_compat::js_json!({"kind": "result", "result": result}),
        (None, error) => pi_js_compat::js_json!({
            "error": error.clone().unwrap_or_default(),
            "kind": "error"
        }),
    };
    append_artifact_record(&manifest.data.artifacts.output_path, &record).map_err(|error| error.to_string())
}

async fn wait_for_pyrun_ownership_decision(
    manifest: &DetachedPyrunLaunchManifest,
) -> Result<Option<DetachedJobOwnershipIdentity>, String> {
    loop {
        if let Some(identity) = read_detached_pyrun_activation(&manifest.data.activation_path)? {
            return Ok(Some(identity));
        }
        if Path::new(&manifest.data.foreground_completion_path).exists() {
            return Ok(None);
        }
        if !is_process_identity_alive(&manifest.data.supervisor_process_identity) {
            return Ok(None);
        }
        tokio::time::sleep(Duration::from_millis(CONTROL_POLL_MS)).await;
    }
}

async fn finalize_detached_pyrun_settlement(
    manifest: &DetachedPyrunLaunchManifest,
    settlement: PyrunSettlement,
    identity: DetachedJobOwnershipIdentity,
) -> Result<i64, String> {
    let outcome = pyrun_settlement_outcome(&settlement);
    let terminal_at = crate::core::compaction::now_millis();
    let terminal_at_iso = crate::utils::iso_time::format_iso_millis(terminal_at);
    let terminal = create_detached_job_terminal_input(
        &manifest.data.artifacts,
        &identity,
        outcome,
        &terminal_at_iso,
        Some((terminal_at - manifest.data.started_at as i64).max(0)),
        manifest.data.tool_call_id.as_deref(),
    )
    .map_err(|error| error.to_string())?;
    let control_db_path = manifest.data.control_db_path.clone();
    let session_path = manifest.data.session_path.clone();
    let finalized = finalize_detached_job_with_retry(&terminal, move |terminal| {
        finalize_detached_job(
            &control_db_path,
            &FinalizeDetachedJobInput {
                session_path: session_path.clone(),
                terminal: terminal.clone(),
            },
        )
    })
    .await;
    match finalized {
        Ok(FinalizeDetachedJobResult::Ok { terminal_revision, .. }) => Ok(terminal_revision),
        Ok(other) => Err(format!("Could not finalize detached Pyrun job: {other:?}")),
        Err(error) => Err(format!("Could not finalize detached Pyrun job: {error}")),
    }
}

fn pyrun_settlement_outcome(settlement: &PyrunSettlement) -> DetachedJobOutcome {
    if let Some(result) = &settlement.result {
        if result.result_type == "error" {
            return DetachedJobOutcome::Failed {
                error: crate::core::multi_agent_store::AgentError {
                    message: result
                        .error
                        .clone()
                        .unwrap_or_else(|| "Pyrun evaluation failed".to_owned().into()),
                    code: None,
                },
                exit_code: None,
            };
        }
        return DetachedJobOutcome::Completed {
            exit_code: None,
            summary: Some("Pyrun evaluation completed.".to_owned()),
        };
    }
    if let Some(cancel) = &settlement.cancel {
        return DetachedJobOutcome::Aborted {
            reason: Some(cancel.reason.clone().unwrap_or_else(|| "cancelled".to_owned())),
        };
    }
    DetachedJobOutcome::Failed {
        error: crate::core::multi_agent_store::AgentError {
            message: settlement.error.clone().unwrap_or_default(),
            code: None,
        },
        exit_code: None,
    }
}
