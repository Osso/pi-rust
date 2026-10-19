//! Detached Bash runner (`core/detached-bash-runner.ts`).
//!
//! The runner owns a gated payload process, adopts cancel commands from the
//! runtime mailbox, and commits exactly one terminal job revision.

use std::collections::BTreeMap;
use std::fmt;
use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use rustix::process::{Pid, Signal, kill_process, kill_process_group};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::core::detached_job_bootstrap::{
    DetachedPayloadError, GatedDetachedPayloadInput, spawn_gated_detached_payload,
};
use crate::core::detached_job_control::{
    DetachedJobRuntimeCommand, EnqueueDetachedJobStatusResponseInput, claim_detached_job_runtime_commands,
    enqueue_detached_job_status_response,
};
pub use crate::core::detached_job_runner::DetachedJobArtifacts;
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetachedBashLaunchManifestData {
    pub activation_path: String,
    pub args: Vec<String>,
    pub artifacts: DetachedJobArtifacts,
    pub command: String,
    pub control_db_path: String,
    pub cwd: String,
    pub env: BTreeMap<String, String>,
    pub foreground_completion_path: String,
    pub runner_address: RuntimeMailboxAddress,
    pub session_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Serialize, Deserialize)]
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
    pub cancel_reason: Option<String>,
    pub exit_code: Option<i32>,
    pub signal: Option<String>,
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
    pub args: Vec<String>,
    pub executable: String,
}

/// Invocation options; the Rust binary is always compiled, so `entry_path`
/// selects a wrapper script instead of the internal runner mode.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DetachedBashRunnerInvocationOptions {
    pub compiled: bool,
    pub entry_path: Option<String>,
    pub executable_path: Option<String>,
}

#[derive(Debug)]
pub struct DetachedBashRunnerError(pub String);

impl fmt::Display for DetachedBashRunnerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for DetachedBashRunnerError {}

impl From<std::io::Error> for DetachedBashRunnerError {
    fn from(source: std::io::Error) -> Self {
        Self(source.to_string())
    }
}

impl From<serde_json::Error> for DetachedBashRunnerError {
    fn from(source: serde_json::Error) -> Self {
        Self(source.to_string())
    }
}

impl From<DetachedPayloadError> for DetachedBashRunnerError {
    fn from(source: DetachedPayloadError) -> Self {
        Self(source.to_string())
    }
}

impl From<ControlDbError> for DetachedBashRunnerError {
    fn from(source: ControlDbError) -> Self {
        Self(source.message().to_owned())
    }
}

/// `writeDetachedBashLaunchManifest`.
pub fn write_detached_bash_launch_manifest(
    path: &str,
    data: &DetachedBashLaunchManifestData,
) -> Result<(), DetachedBashRunnerError> {
    let unsigned = manifest_unsigned_value(data)?;
    let checksum = hash_json(&Value::Object(unsigned.clone()));
    let mut manifest = unsigned;
    manifest.insert("checksum".to_owned(), Value::String(checksum));
    write_atomic_json(path, &Value::Object(manifest))
}

fn manifest_unsigned_value(
    data: &DetachedBashLaunchManifestData,
) -> Result<Map<String, Value>, DetachedBashRunnerError> {
    let manifest = DetachedBashLaunchManifest {
        data: data.clone(),
        version: DETACHED_BASH_LAUNCH_VERSION,
        checksum: String::new(),
    };
    let Value::Object(mut unsigned) = serde_json::to_value(&manifest)? else {
        return Err(DetachedBashRunnerError(
            "Detached Bash launch manifest must serialize to an object".to_owned(),
        ));
    };
    unsigned.remove("checksum");
    Ok(unsigned)
}

/// `writeDetachedBashActivation`.
pub fn write_detached_bash_activation(
    path: &str,
    identity: &DetachedJobOwnershipIdentity,
) -> Result<(), DetachedBashRunnerError> {
    write_atomic_json(path, &serde_json::to_value(identity)?)
}

/// `readDetachedBashActivation`; `None` when the file does not exist.
pub fn read_detached_bash_activation(
    path: &str,
) -> Result<Option<DetachedJobOwnershipIdentity>, DetachedBashRunnerError> {
    if !Path::new(path).exists() {
        return Ok(None);
    }
    let data = std::fs::read_to_string(path)?;
    Ok(Some(serde_json::from_str(&data)?))
}

/// `getDetachedBashRunnerInvocation`.
pub fn get_detached_bash_runner_invocation(
    manifest_path: &str,
    options: &DetachedBashRunnerInvocationOptions,
) -> Result<DetachedBashRunnerInvocation, DetachedBashRunnerError> {
    let executable = match &options.executable_path {
        Some(executable) => executable.clone(),
        None => std::env::current_exe()?.to_string_lossy().into_owned(),
    };
    let args = match &options.entry_path {
        None if options.compiled => vec![DETACHED_BASH_RUNNER_MODE.to_owned(), manifest_path.to_owned()],
        None => {
            return Err(DetachedBashRunnerError(
                "Detached Bash runner entry path is required".to_owned(),
            ));
        }
        Some(entry_path) => vec![entry_path.clone(), manifest_path.to_owned()],
    };
    Ok(DetachedBashRunnerInvocation { args, executable })
}

/// `launchDetachedBashRunner`. `setsid(1)` replaces Node's `detached: true`.
pub fn launch_detached_bash_runner(manifest_path: &str) -> Result<u32, DetachedBashRunnerError> {
    let invocation = get_detached_bash_runner_invocation(
        manifest_path,
        &DetachedBashRunnerInvocationOptions {
            compiled: true,
            entry_path: None,
            executable_path: None,
        },
    )?;
    let directory = Path::new(manifest_path)
        .parent()
        .map(|directory| directory.to_path_buf())
        .unwrap_or_else(|| Path::new(".").to_path_buf());
    let mut command = Command::new("setsid");
    command
        .arg(&invocation.executable)
        .args(&invocation.args)
        .current_dir(directory)
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
    manifest_path: &str,
    now: Option<&(dyn Fn() -> String + Sync)>,
) -> Result<DetachedBashRunnerResult, DetachedBashRunnerError> {
    let manifest = wait_for_detached_bash_launch_manifest(manifest_path).await?;
    let env: Vec<(String, String)> = manifest
        .data
        .env
        .iter()
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect();
    let mut payload = spawn_gated_detached_payload(&GatedDetachedPayloadInput {
        args: &manifest.data.args,
        command: &manifest.data.command,
        cwd: Path::new(&manifest.data.cwd),
        env: Some(&env),
        identity_path: &Path::new(&manifest.data.artifacts.directory).join("payload.json"),
        stderr_path: Path::new(&manifest.data.artifacts.output_path),
        stdout_path: Path::new(&manifest.data.artifacts.output_path),
    })?;
    payload.persist_identity()?;
    payload.release()?;
    let controlled = wait_for_detached_bash_exit(payload, &manifest).await?;
    let Some(ref identity) = controlled.identity else {
        write_atomic_json(
            &manifest.data.foreground_completion_path,
            &serde_json::to_value(&DetachedBashForegroundCompletion {
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
        None => iso_time::now_iso_string(),
    };
    let terminal = create_detached_job_terminal_input(
        &manifest.data.artifacts,
        identity,
        outcome,
        &terminal_at,
        None,
        manifest.data.tool_call_id.as_deref(),
    )
    .map_err(|error| DetachedBashRunnerError(error.to_string()))?;
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
        FinalizeDetachedJobResult::Err { error } => Err(DetachedBashRunnerError(format!(
            "Could not finalize detached Bash job: {}",
            error.as_str()
        ))),
    }
}

struct ControlledExit {
    cancel_reason: Option<String>,
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
    let mut cancel_reason: Option<String> = None;
    let mut identity = read_detached_bash_activation(&manifest.data.activation_path)?;
    let mut timed_out = false;
    let timeout_at = manifest
        .data
        .timeout_ms
        .map(|timeout_ms| Instant::now() + Duration::from_millis(timeout_ms.unsigned_abs()));
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
        if !timed_out && timeout_at.is_some_and(|timeout_at| Instant::now() >= timeout_at) {
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
                            status: serde_json::json!({
                                "outputPath": manifest.data.artifacts.output_path,
                                "payloadPid": payload_pid,
                                "state": "running",
                            }),
                        })?;
                    }
                    DetachedJobRuntimeCommand::Cancel(cancel) => {
                        identity = Some(cancel.identity.clone());
                        cancel_reason = Some(cancel.reason.unwrap_or_else(|| "cancelled".to_owned()));
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
        Err(error) => Err(DetachedBashRunnerError(error.to_string())),
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
                message: "Detached Bash command timed out".to_owned(),
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
            summary: Some("Process exited successfully.".to_owned()),
        };
    }
    let detail = match controlled.exit.signal.and_then(signal_name) {
        Some(signal) => format!("signal {signal}"),
        None => format!(
            "exit code {}",
            exit_code.map_or_else(|| "null".to_owned(), |code| code.to_string())
        ),
    };
    DetachedJobOutcome::Failed {
        error: AgentError {
            message: format!("Process exited with {detail}."),
            code: None,
        },
        exit_code: exit_code.map(i64::from),
    }
}

/// Node signal names for the signals a detached payload can report.
pub fn signal_name(signal: i32) -> Option<String> {
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
    Some(name.to_owned())
}

async fn wait_for_detached_bash_launch_manifest(
    path: &str,
) -> Result<DetachedBashLaunchManifest, DetachedBashRunnerError> {
    let deadline = Instant::now() + Duration::from_millis(LAUNCH_MANIFEST_TIMEOUT_MS);
    while !Path::new(path).exists() {
        if Instant::now() >= deadline {
            return Err(DetachedBashRunnerError(format!(
                "Timed out waiting for detached Bash launch manifest: {path}"
            )));
        }
        tokio::time::sleep(Duration::from_millis(LAUNCH_MANIFEST_POLL_MS)).await;
    }
    read_detached_bash_launch_manifest(path)
}

fn read_detached_bash_launch_manifest(path: &str) -> Result<DetachedBashLaunchManifest, DetachedBashRunnerError> {
    let data = std::fs::read_to_string(path)?;
    let parsed: DetachedBashLaunchManifest = serde_json::from_str(&data)?;
    let checksum = hash_json(&Value::Object(manifest_unsigned_value(&parsed.data)?));
    if parsed.version != DETACHED_BASH_LAUNCH_VERSION || parsed.checksum != checksum {
        return Err(DetachedBashRunnerError(format!(
            "Invalid detached Bash launch manifest: {path}"
        )));
    }
    Ok(parsed)
}

/// `hashJson`.
pub fn hash_json(value: &Value) -> String {
    let serialized = serde_json::to_string(value).expect("JSON values always serialize");
    crate::core::detached_job_runner::sha256_hex(serialized.as_bytes())
}

/// `fsyncFile`.
pub fn fsync_file(path: &str) -> Result<(), std::io::Error> {
    std::fs::File::open(path)?.sync_all()
}

/// `fsyncDirectory`.
pub fn fsync_directory(path: &str) -> Result<(), std::io::Error> {
    std::fs::File::open(path)?.sync_all()
}

fn write_atomic_json(path: &str, value: &Value) -> Result<(), DetachedBashRunnerError> {
    let temporary_path = format!("{path}.tmp");
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .mode(0o600)
        .open(&temporary_path)?;
    file.write_all(format!("{}\n", serde_json::to_string(value)?).as_bytes())?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&temporary_path, path)?;
    fsync_directory(
        Path::new(path)
            .parent()
            .map(|directory| directory.to_string_lossy().into_owned())
            .unwrap_or_else(|| ".".to_owned())
            .as_str(),
    )?;
    Ok(())
}

/// Appends one JSON line (`appendArtifactRecord`).
pub fn append_artifact_record(path: &str, record: &Value) -> Result<(), std::io::Error> {
    let mut file = OpenOptions::new().create(true).append(true).mode(0o600).open(path)?;
    file.write_all(format!("{}\n", serde_json::to_string(record).map_err(std::io::Error::other)?).as_bytes())
}

/// One `readJsonLines` chunk.
#[derive(Debug, Clone, PartialEq)]
pub struct JsonLinesChunk {
    pub offset: u64,
    pub values: Vec<Value>,
}

/// Reads newline-delimited JSON records after `offset` (`readJsonLines`).
pub fn read_json_lines(path: &str, offset: u64) -> Result<JsonLinesChunk, std::io::Error> {
    if !Path::new(path).exists() {
        return Ok(JsonLinesChunk {
            offset,
            values: Vec::new(),
        });
    }
    let data = std::fs::read(path)?;
    if data.len() as u64 <= offset {
        return Ok(JsonLinesChunk {
            offset,
            values: Vec::new(),
        });
    }
    let text = String::from_utf8_lossy(&data[offset as usize..]);
    let mut values = Vec::new();
    for line in text.split('\n').filter(|line| !line.is_empty()) {
        values.push(serde_json::from_str(line).map_err(std::io::Error::other)?);
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
