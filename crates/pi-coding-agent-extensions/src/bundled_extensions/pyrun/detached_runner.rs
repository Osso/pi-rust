//! Port of `packages/coding-agent/extensions/pyrun/src/detached-runner.ts`.

use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::utils::paths::{node_dirname, node_extname, node_join};
use pi_js_compat::js_format;
use pi_js_compat::js_json::{self, JsObject as Map, JsString, JsValue as Value, encode_node_utf8};
use serde::{Deserialize, Serialize};
use tokio::process::Command;

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
    pub activation_path: JsString,
    pub artifacts: DetachedJobArtifacts,
    pub bridge_request_path: JsString,
    pub bridge_response_path: JsString,
    pub control_db_path: JsString,
    pub foreground_completion_path: JsString,
    pub params: CanonicalPyrunEvalParams,
    pub runner_address: RuntimeMailboxAddress,
    pub runner_options: PyrunRunnerOptions,
    pub runner_process_identity: crate::core::runtime_process::ProcessIdentity,
    pub session_path: JsString,
    #[serde(serialize_with = "serialize_js_number")]
    pub started_at: f64,
    pub supervisor_process_identity: crate::core::runtime_process::ProcessIdentity,
    /// Absent in legacy manifests written before tool-call correlation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<JsString>,
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
    pub checksum: JsString,
}

fn fsync_file(path: &JsString) -> Result<(), JsString> {
    let file = std::fs::File::open(&encode_node_utf8(path)).map_err(|error| JsString::from(error.to_string()))?;
    file.sync_all().map_err(|error| JsString::from(error.to_string()))
}

fn fsync_directory(path: &JsString) -> Result<(), JsString> {
    let file = std::fs::File::open(&encode_node_utf8(path)).map_err(|error| JsString::from(error.to_string()))?;
    file.sync_all().map_err(|error| JsString::from(error.to_string()))
}

/// `writeDetachedPyrunLaunchManifest`.
pub fn write_detached_pyrun_launch_manifest(
    path: &JsString,
    data: &DetachedPyrunLaunchManifestData,
) -> Result<(), JsString> {
    let mut unsigned = js_json::to_value(data).map_err(|error| JsString::from(error.to_string()))?;
    let Some(object) = unsigned.as_object_mut() else {
        return Err("detached Pyrun manifest must serialize to an object".into());
    };
    object.insert(
        "version".into(),
        pi_js_compat::number::js_number_value(DETACHED_PYRUN_LAUNCH_VERSION).into(),
    );
    let checksum = hash_json(&Value::Object(object.clone()));
    object.insert("checksum".into(), Value::String(checksum.into()));
    let temporary_path = js_format!("{}.tmp", path);
    let serialized = format!("{}\n", js_json::stringify_lossless(&unsigned));
    write_private(&temporary_path, &serialized)?;
    fsync_file(&temporary_path)?;
    std::fs::rename(encode_node_utf8(&temporary_path), encode_node_utf8(path))
        .map_err(|error| JsString::from(error.to_string()))?;
    let directory = node_dirname(path);
    fsync_directory(&directory)
}

fn write_private(path: &JsString, content: &str) -> Result<(), JsString> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    use std::io::Write;
    let mut file = options
        .open(&encode_node_utf8(path))
        .map_err(|error| JsString::from(error.to_string()))?;
    file.write_all(content.as_bytes())
        .map_err(|error| JsString::from(error.to_string()))
}

/// `writeDetachedPyrunActivation`.
pub fn write_detached_pyrun_activation(
    path: &JsString,
    identity: &DetachedJobOwnershipIdentity,
) -> Result<(), JsString> {
    let temporary_path = js_format!("{}.tmp", path);
    let serialized = format!(
        "{}\n",
        js_json::stringify_lossless(&js_json::to_value(identity).map_err(|error| JsString::from(error.to_string()))?)
    );
    write_private(&temporary_path, &serialized)?;
    fsync_file(&temporary_path)?;
    std::fs::rename(encode_node_utf8(&temporary_path), encode_node_utf8(path))
        .map_err(|error| JsString::from(error.to_string()))?;
    let directory = node_dirname(path);
    fsync_directory(&directory)
}

/// `readDetachedPyrunActivation`.
pub fn read_detached_pyrun_activation(path: &JsString) -> Result<Option<DetachedJobOwnershipIdentity>, JsString> {
    if !Path::new(&encode_node_utf8(path)).exists() {
        return Ok(None);
    }
    let content =
        std::fs::read_to_string(&encode_node_utf8(path)).map_err(|error| JsString::from(error.to_string()))?;
    let value = js_json::parse_lossless(&content).map_err(|error| error.into_message())?;
    js_json::from_value(&value)
        .map(Some)
        .map_err(|error| JsString::from(error.to_string()))
}

/// `readDetachedPyrunLaunchManifest`.
pub fn read_detached_pyrun_launch_manifest(path: &JsString) -> Result<DetachedPyrunLaunchManifest, JsString> {
    let content =
        std::fs::read_to_string(&encode_node_utf8(path)).map_err(|error| JsString::from(error.to_string()))?;
    let parsed: Value = js_json::parse_lossless(&content).map_err(|error| error.into_message())?;
    let original = parsed.clone();
    let mut unsigned = parsed;
    let Some(object) = unsigned.as_object_mut() else {
        return Err(js_format!("Invalid detached Pyrun launch manifest: {}", path).into());
    };
    let checksum = object
        .shift_remove(&JsString::from("checksum"))
        .and_then(|checksum| checksum.as_string().cloned());
    let version = object.get("version").and_then(Value::as_f64);
    let started_at = object.get("startedAt").and_then(Value::as_f64);
    let valid = version == Some(DETACHED_PYRUN_LAUNCH_VERSION)
        && started_at.is_some_and(f64::is_finite)
        && checksum.as_ref() == Some(&JsString::from(hash_json(&unsigned)));
    if !valid {
        return Err(js_format!("Invalid detached Pyrun launch manifest: {}", path).into());
    }
    js_json::from_value(&original).map_err(|error| JsString::from(error.to_string()))
}

/// Rust replacement for the TypeScript entry-script invocation. The compiled
/// binary dispatches [`DETACHED_PYRUN_RUNNER_MODE`] itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetachedPyrunRunnerInvocation {
    pub args: Vec<JsString>,
    pub executable: JsString,
}

/// `getDetachedPyrunRunnerInvocation` options.
#[derive(Debug, Clone, Default)]
pub struct DetachedPyrunRunnerInvocationOptions {
    pub compiled: bool,
    pub entry_path: Option<JsString>,
    pub executable_path: Option<JsString>,
}

/// `getDetachedPyrunRunnerInvocation`.
pub fn get_detached_pyrun_runner_invocation(
    manifest_path: &JsString,
    options: &DetachedPyrunRunnerInvocationOptions,
) -> Result<DetachedPyrunRunnerInvocation, JsString> {
    let executable = match &options.executable_path {
        Some(executable) => executable.clone(),
        None => JsString::from(
            std::env::current_exe()
                .map_err(|error| JsString::from(error.to_string()))?
                .to_string_lossy()
                .into_owned(),
        ),
    };
    if options.compiled && options.entry_path.is_none() {
        return Ok(DetachedPyrunRunnerInvocation {
            args: vec![DETACHED_PYRUN_RUNNER_MODE.into(), manifest_path.to_owned()],
            executable,
        });
    }
    let Some(entry_path) = &options.entry_path else {
        return Err("Detached Pyrun runner entry path is required".into());
    };
    let args = if node_extname(entry_path) == ".ts" {
        vec![
            "--experimental-strip-types".into(),
            entry_path.clone(),
            manifest_path.to_owned(),
        ]
    } else {
        vec![entry_path.clone(), manifest_path.to_owned()]
    };
    Ok(DetachedPyrunRunnerInvocation { args, executable })
}

/// `launchDetachedPyrunRunner`.
pub fn launch_detached_pyrun_runner(manifest_path: &JsString) -> Result<u32, JsString> {
    let invocation = get_detached_pyrun_runner_invocation(
        manifest_path,
        &DetachedPyrunRunnerInvocationOptions {
            compiled: true,
            ..Default::default()
        },
    )?;
    let mut command = Command::new(encode_node_utf8(&invocation.executable));
    command
        .args(invocation.args.iter().map(encode_node_utf8))
        .current_dir(encode_node_utf8(&node_dirname(manifest_path)))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if !cfg!(windows) {
        command.process_group(0);
    }
    let mut child = command
        .spawn()
        .map_err(|_| JsString::from("Could not launch detached Pyrun runner"))?;
    let pid = child
        .id()
        .ok_or_else(|| "Could not launch detached Pyrun runner".to_owned())?;
    tokio::spawn(async move {
        let _ = child.wait().await;
    });
    Ok(pid)
}

struct PendingBridgeRequest {
    resolve: tokio::sync::oneshot::Sender<Result<Option<Value>, JsString>>,
}

type PendingRequests = Arc<Mutex<std::collections::HashMap<JsString, PendingBridgeRequest>>>;

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
pub async fn run_detached_pyrun_runner(manifest_path: &JsString) -> Result<Option<i64>, JsString> {
    let manifest = wait_for_detached_pyrun_launch_manifest(manifest_path).await?;
    let release_control_db =
        retain_control_db_connection(&manifest.data.control_db_path).map_err(|error| error.message().clone())?;
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
) -> Result<Option<i64>, JsString> {
    let output = Arc::new(Mutex::new(super::detached_output::PyrunArtifactWriter::new(
        manifest.data.artifacts.output_path.clone(),
    )));
    let settlement = wait_for_detached_pyrun_settlement(manifest, runner, output.clone()).await?;
    append_pyrun_settlement_record(&output, &settlement)?;
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

async fn wait_for_detached_pyrun_launch_manifest(path: &JsString) -> Result<DetachedPyrunLaunchManifest, JsString> {
    let deadline = std::time::Instant::now() + Duration::from_millis(LAUNCH_MANIFEST_TIMEOUT_MS);
    while !Path::new(&encode_node_utf8(path)).exists() {
        if std::time::Instant::now() >= deadline {
            return Err(js_format!("Timed out waiting for detached Pyrun launch manifest: {}", path).into());
        }
        tokio::time::sleep(Duration::from_millis(LAUNCH_MANIFEST_POLL_MS)).await;
    }
    read_detached_pyrun_launch_manifest(path)
}

async fn wait_for_detached_pyrun_settlement(
    manifest: &DetachedPyrunLaunchManifest,
    runner: &PyrunRunnerClient,
    output: Arc<Mutex<super::detached_output::PyrunArtifactWriter>>,
) -> Result<PyrunSettlement, JsString> {
    let abort = pi_js_compat::abort::AbortController::new();
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
                                .map_err(crate::core::extensions::types::HandlerError::from)
                        }
                        None => dispatch_foreground_pyrun_bridge_request(&manifest, pending, request)
                            .await
                            .map_err(crate::core::extensions::types::HandlerError::from),
                    }
                });
            future
        })
    };
    let settled = runner.evaluate(
        manifest.data.params.clone(),
        Some(Arc::new(move |update: CanonicalPyrunProgressUpdate| {
            output
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .append_progress(&update)
                .map_err(super::runner::PyrunRunnerError::new)
        })),
        Some(abort.signal()),
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
                    error: Some(error.message().clone()),
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
    abort: &pi_js_compat::abort::AbortController,
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
                    abort.abort(None);
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
) -> Result<Option<Value>, pi_ai::request::CallbackError> {
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
    receiver
        .await
        .map_err(|error| JsString::from(error.to_string()))?
        .map_err(Into::into)
}

async fn dispatch_foreground_pyrun_bridge_request(
    manifest: &DetachedPyrunLaunchManifest,
    pending: PendingRequests,
    request: super::runner::PiRequest,
) -> Result<Option<Value>, pi_ai::request::CallbackError> {
    let request_id = JsString::from(pi_ai::utils::random_uuid::random_uuid()?);
    let mut record = Map::new();
    record.insert("method".into(), Value::String(request.method.clone().into()));
    record.insert("params".into(), request.params.clone());
    record.insert("requestId".into(), Value::String(request_id.clone().into()));
    append_artifact_record(&manifest.data.bridge_request_path, &Value::Object(record))
        .map_err(|error| JsString::from(error.to_string()))?;
    let mut offset = 0;
    loop {
        let records = read_json_lines(&manifest.data.bridge_response_path, offset)
            .map_err(|error| JsString::from(error.to_string()))?;
        offset = records.offset;
        for response in &records.values {
            if !response
                .get("requestId")
                .and_then(Value::as_string)
                .is_some_and(|id| id == &request_id)
            {
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
                return Err(error.to_owned().into());
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
pub(crate) fn claim_foreground_bridge_request(directory: &JsString, request_id: &JsString) -> Result<bool, JsString> {
    let path = node_join(directory, js_format!("bridge-claim-{}", request_id));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(encode_node_utf8(&path)) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(error) => Err(JsString::from(error.to_string())),
    }
}

fn append_pyrun_settlement_record(
    output: &Mutex<super::detached_output::PyrunArtifactWriter>,
    settlement: &PyrunSettlement,
) -> Result<(), JsString> {
    let record = match (&settlement.result, &settlement.error) {
        (Some(result), _) => pi_js_compat::js_json!({"kind": "result", "result": result}),
        (None, error) => pi_js_compat::js_json!({
            "error": error.clone().unwrap_or_default(),
            "kind": "error"
        }),
    };
    output
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .finish(&record)
}

async fn wait_for_pyrun_ownership_decision(
    manifest: &DetachedPyrunLaunchManifest,
) -> Result<Option<DetachedJobOwnershipIdentity>, JsString> {
    loop {
        if let Some(identity) = read_detached_pyrun_activation(&manifest.data.activation_path)? {
            return Ok(Some(identity));
        }
        if Path::new(&encode_node_utf8(&manifest.data.foreground_completion_path)).exists() {
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
) -> Result<i64, JsString> {
    let outcome = pyrun_settlement_outcome(&settlement);
    let terminal_at = crate::core::compaction::now_millis();
    let terminal_at_iso = JsString::from(crate::utils::iso_time::format_iso_millis(terminal_at));
    let terminal = create_detached_job_terminal_input(
        &manifest.data.artifacts,
        &identity,
        outcome,
        &terminal_at_iso,
        Some((terminal_at - manifest.data.started_at as i64).max(0)),
        manifest.data.tool_call_id.as_ref(),
    )
    .map_err(|error| error.message())?;
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
        Ok(other) => Err(format!("Could not finalize detached Pyrun job: {other:?}").into()),
        Err(error) => Err(js_format!("Could not finalize detached Pyrun job: {}", error.message())),
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
            summary: Some("Pyrun evaluation completed.".to_owned().into()),
        };
    }
    if let Some(cancel) = &settlement.cancel {
        return DetachedJobOutcome::Aborted {
            reason: Some(cancel.reason.clone().unwrap_or_else(|| "cancelled".to_owned().into())),
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
