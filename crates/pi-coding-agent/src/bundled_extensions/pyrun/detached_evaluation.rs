//! Port of `packages/coding-agent/extensions/pyrun/src/detached-evaluation.ts`.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pi_agent_core::AgentToolResult;
use pi_ai::types::{TextContent, TextOrImageContent};
use serde_json::{Map, Value, json};

use crate::core::detached_bash_runner::{
    append_artifact_record, is_process_identity_alive, terminate_foreground_runner,
};
use crate::core::detached_job_runner::{
    DetachedJobAgentType, DetachedJobLifecycleController, DetachedJobLifecycleControllerOptions, DetachedJobOwnership,
    RegisterDetachedJobInput,
};
use crate::core::extensions::types::{ExtensionContext, HandlerError};
use crate::core::lifecycle_coordinator::{LifecycleCoordinator, LifecycleCoordinatorOptions};
use crate::core::multi_agent_store::{MultiAgentStore, PersistenceTarget};
use crate::core::runtime_process::{ProcessIdentity, read_process_identity};
use crate::core::tool_detach_registry::{ToolDetachHandle, ToolDetachRegistry};

use super::detached_runner::{
    DetachedPyrunLaunchManifest, DetachedPyrunLaunchManifestData, launch_detached_pyrun_runner,
    read_detached_pyrun_activation, read_detached_pyrun_launch_manifest, write_detached_pyrun_activation,
    write_detached_pyrun_launch_manifest,
};
use super::eval_tool::{
    PyrunEvalParams, PyrunPiRequestDispatcher, PyrunProgressUpdateCallback, create_canonical_pyrun_eval_params,
    create_pyrun_progress_reporter, eval_context, format_canonical_pyrun_eval_result,
};
use super::runner::{CanonicalPyrunEvalResult, CanonicalPyrunProgressUpdate, PyrunRunnerOptions};

const ARTIFACT_POLL_MS: u64 = 25;
const ARTIFACT_READ_BYTE_LIMIT: usize = 1_048_576;
const DENSE_CONSOLE_RECORD_THRESHOLD: usize = 100;
const FOREGROUND_RUNNER_LIVENESS_POLL_MS: u64 = 3_000;

pub struct DurablePyrunEvaluationInput {
    pub agent_id: Option<String>,
    pub ctx: ExtensionContext,
    pub detach_registry: Arc<ToolDetachRegistry>,
    pub dispatch_pi_request: PyrunPiRequestDispatcher,
    pub on_update: Option<PyrunProgressUpdateCallback>,
    pub params: PyrunEvalParams,
    pub pi_bridge_enabled: bool,
    pub tool_call_id: Option<String>,
    pub runner_options: PyrunRunnerOptions,
    pub signal: Option<tokio_util::sync::CancellationToken>,
    pub store: Arc<MultiAgentStore>,
}

fn pyrun_artifact_root(session_path: &str) -> Result<String, String> {
    let session_file_name = Path::new(session_path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let session_name = Path::new(&session_file_name)
        .file_stem()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    if session_name.is_empty() {
        return Err("Pyrun session path must have a file name".to_owned());
    }
    Ok(Path::new(session_path)
        .parent()
        .unwrap_or(Path::new("."))
        .join("detached-jobs")
        .join(session_name)
        .display()
        .to_string())
}

fn create_lifecycle_controller(
    input: &DurablePyrunEvaluationInput,
    persistence: &PersistenceTarget,
) -> Result<DetachedJobLifecycleController, String> {
    let process_identity = read_process_identity(std::process::id()).map_err(|error| error.to_string())?;
    let coordinator = LifecycleCoordinator::new(LifecycleCoordinatorOptions {
        control_db_path: persistence.control_db_path.clone(),
        create_agent_id: {
            let store = Arc::clone(&input.store);
            Arc::new(move || store.allocate_agent_id_for_lifecycle_coordinator("pyrun"))
        },
        now: Arc::new(crate::utils::iso_time::now_iso_string),
        process_identity,
        session_path: persistence.session_path.clone(),
    });
    Ok(DetachedJobLifecycleController::new(
        DetachedJobLifecycleControllerOptions {
            artifact_root: Path::new(&persistence.session_path)
                .parent()
                .unwrap_or(Path::new("."))
                .display()
                .to_string(),
            control_db_path: persistence.control_db_path.clone(),
            coordinator: Arc::new(coordinator),
            owner_agent_id: input.ctx.multi_agent_agent_id().ok().flatten(),
            owner_session_id: input
                .ctx
                .session_manager()
                .map_err(|error| error.to_string())?
                .get_session_id()
                .to_owned(),
            session_path: persistence.session_path.clone(),
            store: Arc::clone(&input.store),
            write_bash_launch_manifest: None,
        },
    ))
}

struct LaunchedRunner {
    activation_path: String,
    artifacts: crate::core::detached_job_runner::DetachedJobArtifacts,
    bridge_request_path: String,
    bridge_response_path: String,
    foreground_completion_path: String,
    job_id: String,
    process_identity: ProcessIdentity,
    runner_pid: u32,
    script_path: String,
}

fn write_durable_pyrun_script(path: &str, code: &str) -> Result<(), String> {
    let temporary_path = format!("{path}.tmp-{}", std::process::id());
    {
        use std::io::Write;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary_path).map_err(|error| error.to_string())?;
        file.write_all(code.as_bytes()).map_err(|error| error.to_string())?;
    }
    let file = std::fs::File::open(&temporary_path).map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    std::fs::rename(&temporary_path, path).map_err(|error| error.to_string())?;
    for directory in [
        Path::new(path).parent(),
        Path::new(path).parent().and_then(Path::parent),
    ]
    .into_iter()
    .flatten()
    {
        let file = std::fs::File::open(directory).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn launch_foreground_pyrun_runner(
    input: &DurablePyrunEvaluationInput,
    persistence: &PersistenceTarget,
    controller: &DetachedJobLifecycleController,
    started_at: f64,
) -> Result<LaunchedRunner, String> {
    let job_id = input
        .agent_id
        .clone()
        .ok_or_else(|| "Detached Pyrun requires a persisted tool execution agent ID".to_owned())?;
    let artifacts = controller
        .create_artifacts(&job_id)
        .map_err(|error| error.to_string())?;
    let directory = artifacts.directory.clone();
    let activation_path = Path::new(&directory).join("activation.json").display().to_string();
    let bridge_request_path = Path::new(&directory)
        .join("foreground-bridge-requests.jsonl")
        .display()
        .to_string();
    let bridge_response_path = Path::new(&directory)
        .join("foreground-bridge-responses.jsonl")
        .display()
        .to_string();
    let foreground_completion_path = Path::new(&directory).join("foreground-completed").display().to_string();
    let manifest_path = Path::new(&directory).join("launch.json").display().to_string();
    let script_path = Path::new(&directory).join("script.py").display().to_string();
    write_durable_pyrun_script(&script_path, &input.params.code)?;
    let runner_pid = launch_detached_pyrun_runner(&manifest_path)?;
    let process_identity = read_process_identity(runner_pid).map_err(|error| error.to_string())?;
    let session = input.ctx.session_manager().map_err(|error| error.to_string())?;
    let eval_context = eval_context(&input.ctx).map_err(|error| error.to_string())?;
    let supervisor_identity = read_process_identity(std::process::id()).map_err(|error| error.to_string())?;
    write_detached_pyrun_launch_manifest(
        &manifest_path,
        &DetachedPyrunLaunchManifestData {
            activation_path: activation_path.clone(),
            artifacts: artifacts.clone(),
            bridge_request_path: bridge_request_path.clone(),
            bridge_response_path: bridge_response_path.clone(),
            control_db_path: persistence.control_db_path.clone(),
            foreground_completion_path: foreground_completion_path.clone(),
            params: create_canonical_pyrun_eval_params(&input.params, &eval_context, input.pi_bridge_enabled),
            runner_address: crate::core::session_control_db::RuntimeMailboxAddress {
                agent_id: Some(job_id.clone()),
                session_id: session.get_session_id().to_owned(),
            },
            runner_options: input.runner_options.clone(),
            runner_process_identity: process_identity.clone(),
            session_path: persistence.session_path.clone(),
            started_at,
            supervisor_process_identity: supervisor_identity,
            tool_call_id: input.tool_call_id.clone().unwrap_or_default(),
        },
    )?;
    Ok(LaunchedRunner {
        activation_path,
        artifacts,
        bridge_request_path,
        bridge_response_path,
        foreground_completion_path,
        job_id,
        process_identity,
        runner_pid,
        script_path,
    })
}

struct JsonLineCursor {
    fragments: Vec<u8>,
    offset: u64,
}

impl JsonLineCursor {
    fn new() -> Self {
        Self {
            fragments: Vec::new(),
            offset: 0,
        }
    }
}

fn read_new_json_lines(path: &str, cursor: &mut JsonLineCursor, byte_limit: Option<usize>) -> Vec<Value> {
    let Ok(data) = std::fs::read(path) else {
        return Vec::new();
    };
    let size = data.len() as u64;
    if size <= cursor.offset {
        return Vec::new();
    }
    let start = cursor.offset as usize;
    let end = match byte_limit {
        Some(limit) => (start + limit).min(data.len()),
        None => data.len(),
    };
    cursor.offset = end as u64;
    let chunk = &data[start..end];
    let mut values = Vec::new();
    let mut line_start = 0;
    for (index, byte) in chunk.iter().enumerate() {
        if *byte != b'\n' {
            continue;
        }
        let mut line = std::mem::take(&mut cursor.fragments);
        line.extend_from_slice(&chunk[line_start..index]);
        line_start = index + 1;
        if !line.is_empty()
            && let Ok(value) = serde_json::from_slice(&line)
        {
            values.push(value);
        }
    }
    if line_start < chunk.len() {
        cursor.fragments.extend_from_slice(&chunk[line_start..]);
    }
    values
}

struct DetachState {
    cwd: String,
    job_id: String,
    process_identity: ProcessIdentity,
    runner_pid: u32,
    script_path: String,
    activation_path: String,
    tool_call_id: Option<String>,
    controller: DetachedJobLifecycleController,
    ownership: Mutex<Option<DetachedJobOwnership>>,
    activated: AtomicBool,
}

fn detach_state_fn(state: &DetachState) -> bool {
    if state
        .ownership
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .is_some()
        || !is_process_identity_alive(&state.process_identity)
    {
        return false;
    }
    let ownership = state.controller.register(&RegisterDetachedJobInput {
        agent_type: DetachedJobAgentType::Pyrun,
        cwd: state.cwd.clone(),
        display_name: "Pyrun evaluation".to_owned(),
        job_id: state.job_id.clone(),
        process_identity: state.process_identity.clone(),
        worker_handle_id: state.runner_pid.to_string(),
        tool_call_id: state.tool_call_id.clone(),
        detached: Some(true),
        file_refs: Some(vec![crate::core::multi_agent_store::AgentFileReference {
            label: Some("Pyrun script".to_owned()),
            path: state.script_path.clone(),
        }]),
    });
    let Ok(ownership) = ownership else {
        return false;
    };
    if write_detached_pyrun_activation(&state.activation_path, &ownership.identity).is_err() {
        let _ = terminate_foreground_runner(state.runner_pid, rustix::process::Signal::KILL);
        return false;
    }
    state.activated.store(true, Ordering::SeqCst);
    *state
        .ownership
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(ownership);
    true
}

fn settle_foreground_evaluation(
    input: &DurablePyrunEvaluationInput,
    runner: &LaunchedRunner,
    records: &[Value],
    result: Option<&CanonicalPyrunEvalResult>,
    detached: bool,
) -> Result<Option<AgentToolResult>, HandlerError> {
    if detached {
        return Ok(None);
    }
    if let Some(error) = records
        .iter()
        .find(|record| record.get("kind").and_then(Value::as_str) == Some("error"))
        .and_then(|record| record.get("error").and_then(Value::as_str))
    {
        let _ = std::fs::write(&runner.foreground_completion_path, "failed\n");
        return Err(HandlerError::new(error));
    }
    let Some(result) = result else {
        return Ok(None);
    };
    let _ = std::fs::write(&runner.foreground_completion_path, "completed\n");
    Ok(Some(format_canonical_pyrun_eval_result(&input.params, result.clone())))
}

fn check_foreground_runner_liveness(
    process_identity: &ProcessIdentity,
    next_check_at: &mut std::time::Instant,
) -> Result<(), HandlerError> {
    let now = std::time::Instant::now();
    if now < *next_check_at {
        return Ok(());
    }
    if !is_process_identity_alive(process_identity) {
        return Err(HandlerError::new(
            "Foreground Pyrun runner exited without producing a result",
        ));
    }
    *next_check_at = now + Duration::from_millis(FOREGROUND_RUNNER_LIVENESS_POLL_MS);
    Ok(())
}

fn consume_artifact_records(
    records: &[Value],
    report_progress: &(dyn Fn(CanonicalPyrunProgressUpdate) + Send + Sync),
) -> Option<CanonicalPyrunEvalResult> {
    if records.len() > DENSE_CONSOLE_RECORD_THRESHOLD {
        return consume_dense_artifact_records(records, report_progress);
    }
    consume_individual_artifact_records(records, report_progress)
}

fn consume_individual_artifact_records(
    records: &[Value],
    report_progress: &(dyn Fn(CanonicalPyrunProgressUpdate) + Send + Sync),
) -> Option<CanonicalPyrunEvalResult> {
    let mut result = None;
    for record in records {
        match record.get("kind").and_then(Value::as_str) {
            Some("progress") => {
                if let Some(update) = record.get("update") {
                    report_progress(CanonicalPyrunProgressUpdate { raw: update.clone() });
                }
            }
            Some("result") => {
                if let Some(value) = record.get("result") {
                    result = serde_json::from_value(value.clone()).ok();
                }
            }
            _ => {}
        }
    }
    result
}

fn consume_dense_artifact_records(
    records: &[Value],
    report_progress: &(dyn Fn(CanonicalPyrunProgressUpdate) + Send + Sync),
) -> Option<CanonicalPyrunEvalResult> {
    struct Batch {
        update: Option<CanonicalPyrunProgressUpdate>,
        text: Vec<String>,
    }
    fn flush(batch: &mut Batch, report_progress: &(dyn Fn(CanonicalPyrunProgressUpdate) + Send + Sync)) {
        if let Some(update) = batch.update.take() {
            report_progress(update.with_text(batch.text.join("")));
        }
        batch.text.clear();
    }
    let mut batch = Batch {
        update: None,
        text: Vec::new(),
    };
    let mut result = None;
    for record in records {
        if record.get("kind").and_then(Value::as_str) == Some("progress") {
            let Some(update) = record.get("update") else {
                continue;
            };
            let update = CanonicalPyrunProgressUpdate { raw: update.clone() };
            if update.message_type() != "console" {
                flush(&mut batch, report_progress);
                report_progress(update);
                continue;
            }
            let Some(text) = update.text() else {
                flush(&mut batch, report_progress);
                report_progress(update);
                continue;
            };
            let stream = update.raw.get("stream").and_then(Value::as_str).map(str::to_owned);
            if batch
                .update
                .as_ref()
                .and_then(|existing| existing.raw.get("stream").and_then(Value::as_str))
                .is_some_and(|existing| Some(existing.to_owned()) != stream)
            {
                flush(&mut batch, report_progress);
            }
            let text = text.to_owned();
            batch.update = Some(update);
            batch.text.push(text);
            continue;
        }
        flush(&mut batch, report_progress);
        if record.get("kind").and_then(Value::as_str) == Some("result")
            && let Some(value) = record.get("result")
        {
            result = serde_json::from_value(value.clone()).ok();
        }
    }
    flush(&mut batch, report_progress);
    result
}

fn detached_result(params: &PyrunEvalParams, job_id: &str, log_path: &str) -> AgentToolResult {
    AgentToolResult {
        content: vec![TextOrImageContent::Text(TextContent::new(format!(
            "{}\n\nPyrun evaluation moved to background as job {job_id}. Output will be written to {log_path}.",
            params.code
        )))],
        details: Some(json!({
            "backgroundJobId": job_id,
            "executed": params.code,
            "type": "detached"
        })),
        usage: None,
        added_tool_names: None,
        is_error: None,
        terminate: None,
    }
}

fn lifecycle_name(lifecycle: crate::core::multi_agent_store::AgentLifecycle) -> &'static str {
    use crate::core::multi_agent_store::AgentLifecycle;
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

fn format_terminal_agent_error(
    params: &PyrunEvalParams,
    agent: &crate::core::multi_agent_store::AgentNode,
) -> AgentToolResult {
    let error = agent
        .error
        .as_ref()
        .map(|error| error.message.clone())
        .or_else(|| agent.result.as_ref().and_then(|result| result.summary.clone()))
        .unwrap_or_else(|| format!("Pyrun evaluation {}", lifecycle_name(agent.lifecycle)));
    AgentToolResult {
        content: vec![TextOrImageContent::Text(TextContent::new(format!(
            "{}\n\nError: {error}",
            params.code
        )))],
        details: Some(json!({"error": error, "executed": params.code, "type": "error"})),
        usage: None,
        added_tool_names: None,
        is_error: Some(true),
        terminate: None,
    }
}

fn format_lost_pyrun_runtime(params: &PyrunEvalParams, records: &[Value], output_path: &str) -> AgentToolResult {
    let error = "Pyrun runner exited before producing a result.";
    let console: Vec<Value> = records
        .iter()
        .filter(|record| {
            record.get("kind").and_then(Value::as_str) == Some("progress")
                && record["update"]["type"] == "console"
                && record["update"]["text"].is_string()
        })
        .map(|record| record["update"]["text"].clone())
        .collect();
    let result = CanonicalPyrunEvalResult {
        approval: None,
        console: Some(
            console
                .into_iter()
                .filter_map(|value| serde_json::from_value(value).ok())
                .collect(),
        ),
        error: Some(error.to_owned()),
        executed: Some(params.code.clone()),
        result_type: "error".to_owned(),
        value: None,
    };
    let mut formatted = format_canonical_pyrun_eval_result(params, result);
    for item in &mut formatted.content {
        if let TextOrImageContent::Text(text) = item {
            text.text = format!("{}\nPreserved output: {output_path}", text.text);
        }
    }
    if let Some(Value::Object(details)) = &mut formatted.details {
        details.insert("outputPath".to_owned(), Value::String(output_path.to_owned()));
        details.insert("type".to_owned(), Value::String("lost_runtime".to_owned()));
    }
    formatted
}

struct PyrunManifestCandidate {
    directory: String,
    manifest: DetachedPyrunLaunchManifest,
}

fn read_pyrun_manifest_candidate(directory: &str) -> Option<PyrunManifestCandidate> {
    let manifest_path = Path::new(directory).join("launch.json");
    if !manifest_path.exists() {
        return None;
    }
    let manifest = read_detached_pyrun_launch_manifest(&manifest_path.display().to_string()).ok()?;
    Some(PyrunManifestCandidate {
        directory: directory.to_owned(),
        manifest,
    })
}

fn read_complete_artifact_records(path: &str) -> Vec<Value> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Some(last_newline) = text.rfind('\n') else {
        return Vec::new();
    };
    text[..last_newline]
        .split('\n')
        .filter(|line| !line.is_empty())
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

fn restore_terminal_pyrun_result(records: &[Value]) -> Result<Option<CanonicalPyrunEvalResult>, String> {
    let mut terminal = None;
    for record in records {
        if record.get("kind").and_then(Value::as_str) == Some("progress") {
            continue;
        }
        if terminal.is_some() {
            return Err("Pyrun output artifact contains multiple terminal records".to_owned());
        }
        terminal = Some(match record.get("kind").and_then(Value::as_str) {
            Some("result") => serde_json::from_value(record.get("result").cloned().unwrap_or(Value::Null))
                .map_err(|error| error.to_string())?,
            _ => {
                let console = records
                    .iter()
                    .filter(|record| {
                        record.get("kind").and_then(Value::as_str) == Some("progress")
                            && record["update"]["type"] == "console"
                            && record["update"]["text"].is_string()
                    })
                    .map(|record| record["update"]["text"].clone())
                    .filter_map(|value| serde_json::from_value(value).ok())
                    .collect();
                CanonicalPyrunEvalResult {
                    approval: None,
                    console: Some(console),
                    error: record.get("error").and_then(Value::as_str).map(str::to_owned),
                    executed: None,
                    result_type: "error".to_owned(),
                    value: None,
                }
            }
        });
    }
    Ok(terminal)
}

enum RestoredRunner {
    Resume(Box<LaunchedRunner>, Box<Option<CanonicalPyrunEvalResult>>),
    Lost(Vec<Value>, String),
}

fn restore_foreground_pyrun_runner(
    input: &DurablePyrunEvaluationInput,
    persistence: &PersistenceTarget,
    controller: &DetachedJobLifecycleController,
) -> Result<Option<RestoredRunner>, HandlerError> {
    let agent_id = input
        .agent_id
        .clone()
        .ok_or_else(|| HandlerError::new("Detached Pyrun requires a persisted tool execution agent ID"))?;
    let root = pyrun_artifact_root(&persistence.session_path).map_err(HandlerError::new)?;
    let directory = Path::new(&root).join(&agent_id).display().to_string();
    let Some(candidate) = read_pyrun_manifest_candidate(&directory) else {
        return Ok(None);
    };
    if candidate.manifest.data.runner_address.agent_id.as_deref() != Some(agent_id.as_str()) {
        return Err(HandlerError::new(format!(
            "Pyrun launch manifest agent ID mismatch: {directory}/launch.json"
        )));
    }
    if candidate.manifest.data.tool_call_id != input.tool_call_id.clone().unwrap_or_default() {
        return Err(HandlerError::new(format!(
            "Pyrun tool-call artifact collision for {}",
            input.tool_call_id.clone().unwrap_or_default()
        )));
    }
    let records = read_complete_artifact_records(&candidate.manifest.data.artifacts.output_path);
    if let Some(terminal) = restore_terminal_pyrun_result(&records).map_err(HandlerError::new)? {
        return Ok(Some(RestoredRunner::Resume(
            Box::new(LaunchedRunner {
                activation_path: candidate.manifest.data.activation_path.clone(),
                artifacts: candidate.manifest.data.artifacts.clone(),
                bridge_request_path: candidate.manifest.data.bridge_request_path.clone(),
                bridge_response_path: candidate.manifest.data.bridge_response_path.clone(),
                foreground_completion_path: candidate.manifest.data.foreground_completion_path.clone(),
                job_id: agent_id.clone(),
                process_identity: candidate.manifest.data.runner_process_identity.clone(),
                runner_pid: candidate.manifest.data.runner_process_identity.pid,
                script_path: Path::new(&candidate.directory).join("script.py").display().to_string(),
            }),
            Box::new(Some(terminal)),
        )));
    }
    if !is_process_identity_alive(&candidate.manifest.data.runner_process_identity) {
        return Ok(Some(RestoredRunner::Lost(
            records,
            candidate.manifest.data.artifacts.output_path.clone(),
        )));
    }
    let script_path = Path::new(&candidate.directory).join("script.py");
    if !script_path.exists() {
        return Err(HandlerError::new(format!(
            "Pyrun script artifact is missing: {}",
            script_path.display()
        )));
    }
    let _ = controller;
    Ok(Some(RestoredRunner::Resume(
        Box::new(LaunchedRunner {
            activation_path: candidate.manifest.data.activation_path.clone(),
            artifacts: candidate.manifest.data.artifacts.clone(),
            bridge_request_path: candidate.manifest.data.bridge_request_path.clone(),
            bridge_response_path: candidate.manifest.data.bridge_response_path.clone(),
            foreground_completion_path: candidate.manifest.data.foreground_completion_path.clone(),
            job_id: agent_id,
            process_identity: candidate.manifest.data.runner_process_identity.clone(),
            runner_pid: candidate.manifest.data.runner_process_identity.pid,
            script_path: script_path.display().to_string(),
        }),
        Box::new(None),
    )))
}

async fn observe_detachable_pyrun_evaluation(
    input: DurablePyrunEvaluationInput,
    controller: DetachedJobLifecycleController,
    runner: LaunchedRunner,
    restored_result: Option<CanonicalPyrunEvalResult>,
) -> Result<AgentToolResult, HandlerError> {
    let mut bridge_cursor = JsonLineCursor::new();
    let mut output_cursor = JsonLineCursor::new();
    let mut result = restored_result;
    let mut terminal_agent: Option<crate::core::multi_agent_store::AgentNode> = None;
    let report_progress = create_pyrun_progress_reporter(input.on_update.clone());
    let detach_state = Arc::new(DetachState {
        cwd: input.ctx.cwd().unwrap_or_default(),
        job_id: runner.job_id.clone(),
        process_identity: runner.process_identity.clone(),
        runner_pid: runner.runner_pid,
        script_path: runner.script_path.clone(),
        activation_path: runner.activation_path.clone(),
        tool_call_id: input.tool_call_id.clone(),
        controller: controller.clone(),
        ownership: Mutex::new(None),
        activated: AtomicBool::new(false),
    });
    let detach_for_handle = Arc::clone(&detach_state);
    let detach_handle: Arc<dyn ToolDetachHandle> = Arc::new(DetachFn {
        detach: Box::new(move || detach_state_fn(&detach_for_handle)),
    });
    let subscription = input.detach_registry.register(detach_handle);
    let mut next_liveness = std::time::Instant::now();
    let loop_result = async {
        loop {
            let requests = read_new_json_lines(&runner.bridge_request_path, &mut bridge_cursor, None);
            let detached = read_detached_pyrun_activation(&runner.activation_path).is_some();
            if !detached {
                for request in requests {
                    respond_to_foreground_bridge_request(&input, &runner, &request).await?;
                }
            }
            let records = read_new_json_lines(
                &runner.artifacts.output_path,
                &mut output_cursor,
                Some(ARTIFACT_READ_BYTE_LIMIT),
            );
            if let Some(consumed) = consume_artifact_records(&records, report_progress.as_ref()) {
                result = Some(consumed);
            }
            if let Some(formatted) = settle_foreground_evaluation(&input, &runner, &records, result.as_ref(), detached)?
            {
                return Ok(formatted);
            }
            if !detached {
                check_foreground_runner_liveness(&runner.process_identity, &mut next_liveness)?;
            }
            if detached {
                if let Some(ownership) = read_detached_pyrun_activation(&runner.activation_path) {
                    if let Ok(Some(agent)) = controller.observe(&ownership.job_id) {
                        terminal_agent = Some(agent);
                    }
                    if terminal_agent
                        .as_ref()
                        .is_some_and(|agent| !crate::core::multi_agent_store::is_active_lifecycle(agent.lifecycle))
                    {
                        break;
                    }
                }
                return Ok(detached_result(
                    &input.params,
                    &runner.job_id,
                    &runner.artifacts.output_path,
                ));
            }
            tokio::time::sleep(Duration::from_millis(ARTIFACT_POLL_MS)).await;
        }
        if let Some(result) = result {
            return Ok(format_canonical_pyrun_eval_result(&input.params, result));
        }
        let Some(agent) = terminal_agent else {
            return Err(HandlerError::new("Detached Pyrun job terminal state is unavailable"));
        };
        Ok(format_terminal_agent_error(&input.params, &agent))
    }
    .await;
    subscription.unregister();
    loop_result
}

struct DetachFn {
    detach: Box<dyn Fn() -> bool + Send + Sync>,
}

impl ToolDetachHandle for DetachFn {
    fn detach(&self) -> bool {
        (self.detach)()
    }
}

async fn respond_to_foreground_bridge_request(
    input: &DurablePyrunEvaluationInput,
    runner: &LaunchedRunner,
    request: &Value,
) -> Result<(), HandlerError> {
    let Some(request_id) = request.get("requestId").and_then(Value::as_str) else {
        return Ok(());
    };
    if !claim_foreground_bridge_request(&runner.artifacts.directory, request_id) {
        return Ok(());
    }
    let mut claimed = Map::new();
    claimed.insert("claimed".to_owned(), Value::Bool(true));
    claimed.insert("requestId".to_owned(), Value::String(request_id.to_owned()));
    append_artifact_record(&runner.bridge_response_path, &Value::Object(claimed))
        .map_err(|error| HandlerError::new(error.to_string()))?;
    let method = request
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let params = request.get("params").cloned().unwrap_or(Value::Null);
    let response = (input.dispatch_pi_request)(
        super::runner::PiRequest { method, params },
        input.ctx.clone(),
        input.signal.clone(),
        input.tool_call_id.clone(),
    )
    .await;
    let mut body = Map::new();
    body.insert("requestId".to_owned(), Value::String(request_id.to_owned()));
    match response {
        Ok(result) => {
            body.insert("result".to_owned(), result);
        }
        Err(error) => {
            body.insert("error".to_owned(), Value::String(error.to_string()));
        }
    }
    append_artifact_record(&runner.bridge_response_path, &Value::Object(body))
        .map_err(|error| HandlerError::new(error.to_string()))
}

fn claim_foreground_bridge_request(directory: &str, request_id: &str) -> bool {
    let path = Path::new(directory).join(format!("bridge-claim-{request_id}"));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).is_ok()
}

/// `runDurableDetachablePyrunEvaluation`.
pub async fn run_durable_detachable_pyrun_evaluation(
    input: DurablePyrunEvaluationInput,
) -> Result<AgentToolResult, HandlerError> {
    let Some(started_at) = input.ctx.tool_execution_started_at else {
        return Err(HandlerError::new(
            "Detached Pyrun requires the tool lifecycle start timestamp",
        ));
    };
    let Some(persistence) = input.store.get_persistence_target() else {
        return Err(HandlerError::new(
            "Detached Pyrun requires a persisted supervisor session",
        ));
    };
    let controller = create_lifecycle_controller(&input, &persistence).map_err(HandlerError::new)?;
    let restored = restore_foreground_pyrun_runner(&input, &persistence, &controller)?;
    let (runner, restored_result) = match restored {
        Some(RestoredRunner::Lost(records, output_path)) => {
            return Ok(format_lost_pyrun_runtime(&input.params, &records, &output_path));
        }
        Some(RestoredRunner::Resume(runner, result)) => (*runner, *result),
        None => (
            launch_foreground_pyrun_runner(&input, &persistence, &controller, started_at).map_err(HandlerError::new)?,
            None,
        ),
    };
    observe_detachable_pyrun_evaluation(input, controller, runner, restored_result).await
}
