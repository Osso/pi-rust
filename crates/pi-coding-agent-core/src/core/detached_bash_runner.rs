//! Detached Bash runner (`core/detached-bash-runner.ts`).
//!
//! The runner owns a gated payload process, adopts cancel commands from the
//! runtime mailbox, and commits exactly one terminal job revision.

use std::fmt;
use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use pi_js_compat::js_json::{self, JsObject, JsString, JsValue, encode_node_utf8};
use rustix::process::{Pid, Signal, kill_process, kill_process_group};
use serde::{Deserialize, Serialize};

use crate::core::detached_job_bootstrap::{
    DetachedPayloadError, GatedDetachedPayloadInput, spawn_gated_detached_payload,
};
use crate::core::detached_job_control::{
    DetachedJobRuntimeCommand, EnqueueDetachedJobStatusResponseInput, claim_detached_job_runtime_commands,
    enqueue_detached_job_status_response,
};
pub use crate::core::detached_job_runner::DetachedJobArtifacts;
use crate::utils::paths::{node_dirname, node_join};
use pi_js_compat::js_format;

use crate::core::detached_job_runner::{
    DetachedJobOutcome, DetachedJobOwnershipIdentity, DetachedJobTerminalInput, create_detached_job_terminal_input,
};
use crate::core::multi_agent_store::AgentError;
pub use crate::core::session_control_db::retain_control_db_connection;
use crate::core::session_control_db::{
    FinalizeDetachedJobInput, FinalizeDetachedJobResult, RuntimeMailboxAddress, finalize_detached_job,
};
use crate::core::sqlite::ControlDbError;
use crate::utils::iso_time;

pub use crate::core::runtime_process::is_process_identity_alive;
pub use crate::internal_runner::DETACHED_BASH_RUNNER_MODE;

/// Launch manifest schema version.
pub const DETACHED_BASH_LAUNCH_VERSION: i64 = 3;
const LAUNCH_MANIFEST_POLL_MS: u64 = 10;
const LAUNCH_MANIFEST_TIMEOUT_MS: u64 = 30_000;
const EXIT_POLL_MS: u64 = 25;

/// `DetachedBashLaunchManifestData`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetachedBashLaunchManifestData {
    pub activation_path: JsString,
    pub args: Vec<JsString>,
    pub artifacts: DetachedJobArtifacts,
    pub command: JsString,
    pub control_db_path: JsString,
    pub cwd: JsString,
    #[serde(
        serialize_with = "js_json::serialize_object",
        deserialize_with = "js_json::deserialize_object"
    )]
    pub env: JsObject,
    pub foreground_completion_path: JsString,
    pub runner_address: RuntimeMailboxAddress,
    pub session_path: JsString,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "pi_js_compat::number::serialize_option"
    )]
    pub timeout_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<JsString>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DetachedBashLaunchManifest {
    #[serde(flatten)]
    data: DetachedBashLaunchManifestData,
    version: i64,
    checksum: String,
}

/// `DetachedBashForegroundCompletion`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetachedBashForegroundCompletion {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cancel_reason: Option<JsString>,
    pub exit_code: Option<i32>,
    pub signal: Option<JsString>,
    pub timed_out: bool,
}

/// `DetachedBashRunnerResult`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DetachedBashRunnerResult {
    pub exit_code: Option<i32>,
    pub terminal_revision: i64,
}

/// Runner invocation (`getDetachedBashRunnerInvocation`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetachedBashRunnerInvocation {
    pub args: Vec<JsString>,
    pub executable: JsString,
}

/// Invocation options; the Rust binary is always compiled, so `entry_path`
/// selects a wrapper script instead of the internal runner mode.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DetachedBashRunnerInvocationOptions {
    pub compiled: bool,
    pub entry_path: Option<JsString>,
    pub executable_path: Option<JsString>,
}

#[derive(Debug)]
pub struct DetachedBashRunnerError(pub JsString);
impl DetachedBashRunnerError {
    pub fn new(message: impl Into<JsString>) -> Self {
        Self(message.into())
    }
    pub fn message(&self) -> &JsString {
        &self.0
    }
}

impl fmt::Display for DetachedBashRunnerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&encode_node_utf8(&self.0))
    }
}

impl std::error::Error for DetachedBashRunnerError {}

impl From<std::io::Error> for DetachedBashRunnerError {
    fn from(source: std::io::Error) -> Self {
        Self::new(crate::core::tools::node_fs::operation_error_message(&source))
    }
}

impl From<serde_json::Error> for DetachedBashRunnerError {
    fn from(source: serde_json::Error) -> Self {
        Self::new(source.to_string())
    }
}

impl From<DetachedPayloadError> for DetachedBashRunnerError {
    fn from(source: DetachedPayloadError) -> Self {
        Self::new(source.message())
    }
}

impl From<ControlDbError> for DetachedBashRunnerError {
    fn from(source: ControlDbError) -> Self {
        Self::new(source.message().clone())
    }
}

/// `writeDetachedBashLaunchManifest`.
pub fn write_detached_bash_launch_manifest(
    path: &JsString,
    data: &DetachedBashLaunchManifestData,
) -> Result<(), DetachedBashRunnerError> {
    let unsigned = manifest_unsigned_value(data)?;
    let checksum = hash_json(&JsValue::Object(unsigned.clone()));
    let mut manifest = unsigned;
    manifest.insert("checksum".into(), JsValue::String(checksum.into()));
    write_atomic_json(path, &JsValue::Object(manifest))
}

fn manifest_unsigned_value(data: &DetachedBashLaunchManifestData) -> Result<JsObject, DetachedBashRunnerError> {
    let manifest = DetachedBashLaunchManifest {
        data: data.clone(),
        version: DETACHED_BASH_LAUNCH_VERSION,
        checksum: String::new(),
    };
    let JsValue::Object(mut unsigned) = js_json::to_value(&manifest)? else {
        return Err(DetachedBashRunnerError::new(
            "Detached Bash launch manifest must serialize to an object".to_owned(),
        ));
    };
    unsigned.shift_remove("checksum");
    Ok(unsigned)
}

/// `writeDetachedBashActivation`.
pub fn write_detached_bash_activation(
    path: &JsString,
    identity: &DetachedJobOwnershipIdentity,
) -> Result<(), DetachedBashRunnerError> {
    write_atomic_json(path, &js_json::to_value(identity)?)
}

/// `readDetachedBashActivation`; `None` when the file does not exist.
pub fn read_detached_bash_activation(
    path: &JsString,
) -> Result<Option<DetachedJobOwnershipIdentity>, DetachedBashRunnerError> {
    if !Path::new(&encode_node_utf8(path)).exists() {
        return Ok(None);
    }
    let data = std::fs::read_to_string(encode_node_utf8(path))?;
    let value = js_json::parse_lossless(&data).map_err(|error| DetachedBashRunnerError::new(error.into_message()))?;
    Ok(Some(
        js_json::from_value(&value).map_err(DetachedBashRunnerError::from)?,
    ))
}

/// `getDetachedBashRunnerInvocation`.
pub fn get_detached_bash_runner_invocation(
    manifest_path: &JsString,
    options: &DetachedBashRunnerInvocationOptions,
) -> Result<DetachedBashRunnerInvocation, DetachedBashRunnerError> {
    let executable = match &options.executable_path {
        Some(executable) => executable.clone(),
        None => std::env::current_exe()?.to_string_lossy().into_owned().into(),
    };
    let args = match &options.entry_path {
        None if options.compiled => vec![DETACHED_BASH_RUNNER_MODE.into(), manifest_path.clone()],
        None => {
            return Err(DetachedBashRunnerError::new(
                "Detached Bash runner entry path is required".to_owned(),
            ));
        }
        Some(entry_path) => vec![entry_path.clone(), manifest_path.to_owned()],
    };
    Ok(DetachedBashRunnerInvocation { args, executable })
}

/// `launchDetachedBashRunner`. `setsid(1)` replaces Node's `detached: true`.
pub fn launch_detached_bash_runner(manifest_path: &JsString) -> Result<u32, DetachedBashRunnerError> {
    let invocation = get_detached_bash_runner_invocation(
        manifest_path,
        &DetachedBashRunnerInvocationOptions {
            compiled: true,
            entry_path: None,
            executable_path: None,
        },
    )?;
    let directory = node_dirname(manifest_path);
    let mut command = Command::new("setsid");
    command
        .arg(encode_node_utf8(&invocation.executable))
        .args(invocation.args.iter().map(encode_node_utf8))
        .current_dir(encode_node_utf8(&directory))
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(home) = pi_js_compat::process_env::get("HOME") {
        command.env("HOME", home);
    }
    if let Some(path) = pi_js_compat::process_env::get("PATH") {
        command.env("PATH", path);
    }
    let child = command.spawn()?;
    Ok(child.id())
}

/// `runDetachedBashRunner`.
pub async fn run_detached_bash_runner(
    manifest_path: &JsString,
    now: Option<&(dyn Fn() -> JsString + Sync)>,
) -> Result<DetachedBashRunnerResult, DetachedBashRunnerError> {
    let manifest = wait_for_detached_bash_launch_manifest(manifest_path).await?;
    let env = manifest
        .data
        .env
        .iter()
        .map(|(name, value)| {
            let value = value.as_string().ok_or_else(|| {
                DetachedBashRunnerError::new(js_format!("Detached Bash environment value must be a string: {}", name))
            })?;
            Ok((name.clone(), value.clone()))
        })
        .collect::<Result<Vec<_>, DetachedBashRunnerError>>()?;
    let mut payload = spawn_gated_detached_payload(&GatedDetachedPayloadInput {
        args: &manifest.data.args,
        command: &manifest.data.command,
        cwd: &manifest.data.cwd,
        env: Some(&env),
        identity_path: &node_join(&manifest.data.artifacts.directory, "payload.json"),
        stderr_path: &manifest.data.artifacts.output_path,
        stdout_path: &manifest.data.artifacts.output_path,
    })?;
    payload.persist_identity()?;
    payload.release()?;
    let controlled = wait_for_detached_bash_exit(payload, &manifest).await?;
    let Some(ref identity) = controlled.identity else {
        write_atomic_json(
            &manifest.data.foreground_completion_path,
            &js_json::to_value(&DetachedBashForegroundCompletion {
                cancel_reason: controlled.cancel_reason.clone(),
                exit_code: controlled.exit.exit_code,
                signal: controlled.exit.signal.and_then(signal_name),
                timed_out: controlled.timed_out,
            })?,
        )?;
        return Ok(DetachedBashRunnerResult {
            exit_code: controlled.exit.exit_code,
            terminal_revision: 0,
        });
    };
    let outcome = detached_bash_outcome(&controlled);
    let terminal_at = match now {
        Some(now) => now(),
        None => iso_time::now_iso_string().into(),
    };
    let terminal = create_detached_job_terminal_input(
        &manifest.data.artifacts,
        identity,
        outcome,
        &terminal_at,
        None,
        manifest.data.tool_call_id.as_ref(),
    )
    .map_err(|error| DetachedBashRunnerError::new(error.message()))?;
    let finalized = finalize_detached_job_with_retry(&terminal, |terminal| {
        finalize_detached_job(
            &manifest.data.control_db_path,
            &FinalizeDetachedJobInput {
                session_path: manifest.data.session_path.clone(),
                terminal: terminal.clone(),
            },
        )
    })
    .await?;
    match finalized {
        FinalizeDetachedJobResult::Ok { terminal_revision, .. } => Ok(DetachedBashRunnerResult {
            exit_code: controlled.exit.exit_code,
            terminal_revision,
        }),
        FinalizeDetachedJobResult::Err { error } => Err(DetachedBashRunnerError::new(format!(
            "Could not finalize detached Bash job: {}",
            error.as_str()
        ))),
    }
}

struct ControlledExit {
    cancel_reason: Option<JsString>,
    exit: crate::core::detached_job_bootstrap::GatedDetachedPayloadExit,
    identity: Option<DetachedJobOwnershipIdentity>,
    timed_out: bool,
}

async fn wait_for_detached_bash_exit(
    payload: crate::core::detached_job_bootstrap::GatedDetachedPayload,
    manifest: &DetachedBashLaunchManifest,
) -> Result<ControlledExit, DetachedBashRunnerError> {
    let payload_pid = payload.pid();
    let exit_future = payload.wait_for_exit();
    tokio::pin!(exit_future);
    let mut exit = None;
    let mut cancel_reason: Option<JsString> = None;
    let mut identity = read_detached_bash_activation(&manifest.data.activation_path)?;
    let mut timed_out = false;
    let timeout_at = manifest
        .data
        .timeout_ms
        .map(|timeout_ms| pi_js_compat::date::js_now_millis() + timeout_ms);
    loop {
        tokio::select! {
            result = &mut exit_future, if exit.is_none() => {
                exit = Some(result?);
            }
            () = tokio::time::sleep(Duration::from_millis(EXIT_POLL_MS)) => {}
        }
        if exit.is_some() {
            break;
        }
        if identity.is_none() {
            identity = read_detached_bash_activation(&manifest.data.activation_path)?;
        }
        if !timed_out && timeout_at.is_some_and(|timeout_at| pi_js_compat::date::js_now_millis() >= timeout_at) {
            timed_out = true;
            signal_payload_group(payload_pid, Signal::TERM)?;
        }
        if let Some(current_identity) = identity.clone() {
            for command in claim_detached_job_runtime_commands(
                &manifest.data.control_db_path,
                &manifest.data.runner_address,
                &current_identity,
            )? {
                match command {
                    DetachedJobRuntimeCommand::Status(status) => {
                        enqueue_detached_job_status_response(&EnqueueDetachedJobStatusResponseInput {
                            control_db_path: manifest.data.control_db_path.clone(),
                            identity: current_identity.clone(),
                            reply_to: status.reply_to,
                            request_id: status.request_id,
                            runner_address: manifest.data.runner_address.clone(),
                            session_path: manifest.data.session_path.clone(),
                            status: pi_js_compat::js_json!({
                                "outputPath": manifest.data.artifacts.output_path,
                                "payloadPid": payload_pid,
                                "state": "running",
                            }),
                        })?;
                    }
                    DetachedJobRuntimeCommand::Cancel(cancel) => {
                        identity = Some(cancel.identity.clone());
                        cancel_reason = Some(cancel.reason.unwrap_or_else(|| "cancelled".into()));
                        signal_payload_group(payload_pid, Signal::TERM)?;
                    }
                    DetachedJobRuntimeCommand::Response(_) => {}
                }
            }
        }
    }
    if identity.is_none() {
        identity = read_detached_bash_activation(&manifest.data.activation_path)?;
    }
    Ok(ControlledExit {
        cancel_reason,
        exit: exit.expect("the loop exits only after the payload exit is recorded"),
        identity,
        timed_out,
    })
}

fn signal_payload_group(pid: u32, signal: Signal) -> Result<(), DetachedBashRunnerError> {
    let Ok(pid) = i32::try_from(pid).ok().and_then(Pid::from_raw).ok_or(()) else {
        return Ok(());
    };
    match kill_process_group(pid, signal) {
        Ok(()) => Ok(()),
        Err(rustix::io::Errno::SRCH) => Ok(()),
        Err(error) => Err(DetachedBashRunnerError::new(error.to_string())),
    }
}

/// Error classification for terminal finalization retries.
pub trait DetachedFinalizeError {
    fn is_contention(&self) -> bool;
}

impl DetachedFinalizeError for ControlDbError {
    fn is_contention(&self) -> bool {
        ControlDbError::is_contention(self)
    }
}

/// `finalizeDetachedJobWithRetry`.
pub async fn finalize_detached_job_with_retry<T, E, F>(terminal: &DetachedJobTerminalInput, finalize: F) -> Result<T, E>
where
    E: DetachedFinalizeError,
    F: Fn(&DetachedJobTerminalInput) -> Result<T, E>,
{
    finalize_detached_job_with_retry_delay(terminal, finalize, Duration::from_millis(250)).await
}

/// Retry variant with an explicit delay for tests.
pub async fn finalize_detached_job_with_retry_delay<T, E, F>(
    terminal: &DetachedJobTerminalInput,
    finalize: F,
    retry_delay: Duration,
) -> Result<T, E>
where
    E: DetachedFinalizeError,
    F: Fn(&DetachedJobTerminalInput) -> Result<T, E>,
{
    loop {
        match finalize(terminal) {
            Ok(result) => return Ok(result),
            Err(error) if error.is_contention() => tokio::time::sleep(retry_delay).await,
            Err(error) => return Err(error),
        }
    }
}

fn detached_bash_outcome(controlled: &ControlledExit) -> DetachedJobOutcome {
    if controlled.timed_out {
        return DetachedJobOutcome::Failed {
            error: AgentError {
                message: "Detached Bash command timed out".into(),
                code: None,
            },
            exit_code: None,
        };
    }
    if let Some(reason) = &controlled.cancel_reason {
        return DetachedJobOutcome::Aborted {
            reason: Some(reason.clone()),
        };
    }
    let exit_code = controlled.exit.exit_code;
    if exit_code == Some(0) {
        return DetachedJobOutcome::Completed {
            exit_code: Some(0),
            summary: Some("Process exited successfully.".into()),
        };
    }
    let detail = match controlled.exit.signal.and_then(signal_name) {
        Some(signal) => js_format!("signal {}", signal),
        None => format!(
            "exit code {}",
            exit_code.map_or_else(|| "null".to_owned(), |code| code.to_string())
        )
        .into(),
    };
    DetachedJobOutcome::Failed {
        error: AgentError {
            message: js_format!("Process exited with {}.", detail),
            code: None,
        },
        exit_code: exit_code.map(i64::from),
    }
}

/// Node signal names for the signals a detached payload can report.
pub fn signal_name(signal: i32) -> Option<JsString> {
    let name = match signal {
        1 => "SIGHUP",
        2 => "SIGINT",
        3 => "SIGQUIT",
        4 => "SIGILL",
        5 => "SIGTRAP",
        6 => "SIGABRT",
        7 => "SIGBUS",
        8 => "SIGFPE",
        9 => "SIGKILL",
        10 => "SIGUSR1",
        11 => "SIGSEGV",
        12 => "SIGUSR2",
        13 => "SIGPIPE",
        14 => "SIGALRM",
        15 => "SIGTERM",
        16 => "SIGSTKFLT",
        17 => "SIGCHLD",
        18 => "SIGCONT",
        19 => "SIGSTOP",
        20 => "SIGTSTP",
        21 => "SIGTTIN",
        22 => "SIGTTOU",
        23 => "SIGURG",
        24 => "SIGXCPU",
        25 => "SIGXFSZ",
        26 => "SIGVTALRM",
        27 => "SIGPROF",
        28 => "SIGWINCH",
        29 => "SIGIO",
        30 => "SIGPWR",
        31 => "SIGSYS",
        _ => return None,
    };
    Some(name.into())
}

async fn wait_for_detached_bash_launch_manifest(
    path: &JsString,
) -> Result<DetachedBashLaunchManifest, DetachedBashRunnerError> {
    let deadline = Instant::now() + Duration::from_millis(LAUNCH_MANIFEST_TIMEOUT_MS);
    while !Path::new(&encode_node_utf8(path)).exists() {
        if Instant::now() >= deadline {
            return Err(DetachedBashRunnerError::new(js_format!(
                "Timed out waiting for detached Bash launch manifest: {}",
                path
            )));
        }
        tokio::time::sleep(Duration::from_millis(LAUNCH_MANIFEST_POLL_MS)).await;
    }
    read_detached_bash_launch_manifest(path)
}

fn read_detached_bash_launch_manifest(path: &JsString) -> Result<DetachedBashLaunchManifest, DetachedBashRunnerError> {
    let data = std::fs::read_to_string(encode_node_utf8(path))?;
    let value = js_json::parse_lossless(&data).map_err(|error| DetachedBashRunnerError::new(error.into_message()))?;
    let parsed_data = js_json::from_value::<DetachedBashLaunchManifestData>(&value)?;
    let checksum = hash_json(&JsValue::Object(manifest_unsigned_value(&parsed_data)?));
    let version_matches = value.get("version").and_then(JsValue::as_f64) == Some(DETACHED_BASH_LAUNCH_VERSION as f64);
    let checksum_matches = value
        .get("checksum")
        .and_then(JsValue::as_string)
        .is_some_and(|stored| stored == &checksum);
    if !version_matches || !checksum_matches {
        return Err(DetachedBashRunnerError::new(js_format!(
            "Invalid detached Bash launch manifest: {}",
            path
        )));
    }
    Ok(DetachedBashLaunchManifest {
        data: parsed_data,
        version: DETACHED_BASH_LAUNCH_VERSION,
        checksum,
    })
}

/// `hashJson`.
pub fn hash_json(value: &pi_js_compat::js_json::JsValue) -> String {
    let serialized = pi_js_compat::js_json::stringify_lossless(value);
    crate::core::detached_job_runner::sha256_hex(serialized.as_bytes())
}

/// `fsyncFile`.
pub fn fsync_file(path: &JsString) -> Result<(), std::io::Error> {
    std::fs::File::open(encode_node_utf8(path))?.sync_all()
}

/// `fsyncDirectory`.
pub fn fsync_directory(path: &JsString) -> Result<(), DetachedBashRunnerError> {
    fsync_artifact_path(path)
}

fn write_atomic_json(path: &JsString, value: &JsValue) -> Result<(), DetachedBashRunnerError> {
    let temporary_path = js_format!("{}.tmp", path);
    write_temporary_json(&temporary_path, value)?;
    fsync_artifact_path(&temporary_path)?;
    std::fs::rename(encode_node_utf8(&temporary_path), encode_node_utf8(path)).map_err(|error| {
        DetachedBashRunnerError::new(pi_js_compat::node_fs::node_fs_dest_error(
            &error,
            "rename",
            &temporary_path,
            path,
        ))
    })?;
    fsync_directory(&node_dirname(path))
}

fn validate_artifact_path(path: &JsString) -> Result<(), DetachedBashRunnerError> {
    match pi_js_compat::node_fs::node_path_argument_error(path) {
        Some(error) => Err(DetachedBashRunnerError::new(error.message)),
        None => Ok(()),
    }
}

fn write_temporary_json(path: &JsString, value: &JsValue) -> Result<(), DetachedBashRunnerError> {
    validate_artifact_path(path)?;
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .mode(0o600)
        .open(encode_node_utf8(path))
        .map_err(|error| DetachedBashRunnerError::new(pi_js_compat::node_fs::node_fs_error(&error, "open", path)))?;
    let line = format!("{}\n", js_json::stringify_lossless(value));
    let written = file.write_all(line.as_bytes()).map_err(|error| {
        DetachedBashRunnerError::new(pi_js_compat::node_fs::node_file_write_error_record(&error).message)
    });
    // writeFileSync closes in finally: close errors override failed writes.
    close_artifact_descriptor(file)?;
    written
}

fn fsync_artifact_path(path: &JsString) -> Result<(), DetachedBashRunnerError> {
    validate_artifact_path(path)?;
    let file = std::fs::File::open(encode_node_utf8(path))
        .map_err(|error| DetachedBashRunnerError::new(pi_js_compat::node_fs::node_fs_error(&error, "open", path)))?;
    let synced = file.sync_all().map_err(|error| {
        DetachedBashRunnerError::new(pi_js_compat::node_fs::node_fs_error(
            &error,
            "fsync",
            &JsString::default(),
        ))
    });
    // Source fsyncFile/fsyncDirectory both close in finally after fsync.
    close_artifact_descriptor(file)?;
    synced
}

fn close_artifact_descriptor(file: std::fs::File) -> Result<(), DetachedBashRunnerError> {
    nix::unistd::close(file).map_err(|error| {
        DetachedBashRunnerError::new(pi_js_compat::node_fs::node_fs_error(
            &std::io::Error::from(error),
            "close",
            &JsString::default(),
        ))
    })
}

/// Appends one JSON line (`appendArtifactRecord`).
pub fn append_artifact_record(path: &JsString, record: &pi_js_compat::js_json::JsValue) -> Result<(), std::io::Error> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(encode_node_utf8(path))?;
    let line = format!("{}\n", pi_js_compat::js_json::stringify_lossless(record));
    file.write_all(line.as_bytes())
}

/// One `readJsonLines` chunk.
#[derive(Debug, Clone, PartialEq)]
pub struct JsonLinesChunk {
    pub offset: u64,
    pub values: Vec<pi_js_compat::js_json::JsValue>,
}

/// Reads newline-delimited JSON records after `offset` (`readJsonLines`).
pub fn read_json_lines(path: &JsString, offset: u64) -> Result<JsonLinesChunk, std::io::Error> {
    if !Path::new(&encode_node_utf8(path)).exists() {
        return Ok(JsonLinesChunk {
            offset,
            values: Vec::new(),
        });
    }
    let data = std::fs::read(encode_node_utf8(path))?;
    if data.len() as u64 <= offset {
        return Ok(JsonLinesChunk {
            offset,
            values: Vec::new(),
        });
    }
    let text = String::from_utf8_lossy(&data[offset as usize..]);
    let mut values = Vec::new();
    for line in text.split('\n').filter(|line| !line.is_empty()) {
        values.push(pi_js_compat::js_json::parse_lossless(line).map_err(|error| {
            std::io::Error::other(crate::core::extensions::types::HandlerError::new(error.into_message()))
        })?);
    }
    Ok(JsonLinesChunk {
        offset: data.len() as u64,
        values,
    })
}

/// `terminateForegroundRunner`: signal the process group, then the process.
pub fn terminate_foreground_runner(pid: u32, signal: Signal) -> Result<(), String> {
    let Ok(pid) = i32::try_from(pid).ok().and_then(Pid::from_raw).ok_or(()) else {
        return Err(format!("invalid pid {pid}"));
    };
    match kill_process_group(pid, signal) {
        Ok(()) => return Ok(()),
        Err(rustix::io::Errno::SRCH) => {}
        Err(error) => return Err(error.to_string()),
    }
    match kill_process(pid, signal) {
        Ok(()) => Ok(()),
        Err(rustix::io::Errno::SRCH) => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}
