//! Port of `packages/coding-agent/extensions/pyrun/src/detached-evaluation.ts`.

use crate::utils::paths::{node_basename, node_dirname, node_extname, node_join};
use pi_js_compat::js_format;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pi_agent_core::AgentToolResult;
use pi_ai::request::CallbackError;
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_js_compat::{
    js_json as json,
    js_json::{self, JsObject as Map, JsString, JsValue as Value},
};

use crate::core::compaction::now_millis;
use crate::core::detached_bash_runner::{
    append_artifact_record, finalize_detached_job_with_retry, is_process_identity_alive, terminate_foreground_runner,
};
use crate::core::detached_job_runner::{
    DetachedJobAgentType, DetachedJobLifecycleController, DetachedJobLifecycleControllerOptions, DetachedJobOwnership,
    RegisterDetachedJobInput, create_detached_job_terminal_input,
};
use crate::core::extensions::types::{ExtensionContext, HandlerError};
use crate::core::lifecycle_coordinator::{LifecycleCoordinator, LifecycleCoordinatorOptions};
use crate::core::multi_agent_store::{AgentLifecycle, AgentNode, MultiAgentStore, PersistenceTarget};
use crate::core::runtime_process::{ProcessIdentity, read_process_identity};
use crate::core::session_control_db::{
    FinalizeDetachedJobInput, FinalizeDetachedJobResult, finalize_detached_job, read_multi_agent_agent,
};
use crate::core::tool_detach_registry::{ToolDetachHandle, ToolDetachRegistry};

use super::detached_runner::{
    DetachedPyrunLaunchManifest, DetachedPyrunLaunchManifestData, claim_foreground_bridge_request,
    launch_detached_pyrun_runner, read_detached_pyrun_activation, read_detached_pyrun_launch_manifest,
    write_detached_pyrun_activation, write_detached_pyrun_launch_manifest,
};
use super::eval_tool::{
    PyrunEvalParams, PyrunPiRequestDispatcher, PyrunProgressUpdateCallback, create_canonical_pyrun_eval_params,
    create_pyrun_progress_reporter, eval_context, format_canonical_pyrun_eval_result,
};
use super::runner::{CanonicalPyrunEvalResult, PyrunRunnerOptions};

const ARTIFACT_POLL_MS: u64 = 25;
const ARTIFACT_READ_BYTE_LIMIT: usize = 1_048_576;
const FOREGROUND_RUNNER_LIVENESS_POLL_MS: u64 = 3_000;

pub struct DurablePyrunEvaluationInput {
    pub agent_id: Option<JsString>,
    pub ctx: ExtensionContext,
    pub detach_registry: Arc<ToolDetachRegistry>,
    pub dispatch_pi_request: PyrunPiRequestDispatcher,
    pub on_update: Option<PyrunProgressUpdateCallback>,
    pub params: PyrunEvalParams,
    pub pi_bridge_enabled: bool,
    pub tool_call_id: Option<JsString>,
    pub runner_options: PyrunRunnerOptions,
    pub signal: Option<pi_js_compat::abort::AbortSignal>,
    pub store: Arc<MultiAgentStore>,
}

fn pyrun_artifact_root(session_path: &JsString) -> Result<JsString, JsString> {
    let name = node_basename(session_path);
    let extension = node_extname(&name);
    let session_name = name.slice(0, name.len() - extension.len());
    if session_name.is_empty() {
        return Err("Pyrun session path must have a file name".into());
    }
    Ok(node_join(
        node_join(node_dirname(session_path), "detached-jobs"),
        session_name,
    ))
}

fn create_lifecycle_controller(
    input: &DurablePyrunEvaluationInput,
    persistence: &PersistenceTarget,
) -> Result<DetachedJobLifecycleController, JsString> {
    let process_identity = read_process_identity(std::process::id()).map_err(|error| error.message)?;
    let coordinator = LifecycleCoordinator::new(LifecycleCoordinatorOptions {
        control_db_path: persistence.control_db_path.clone(),
        create_agent_id: {
            let store = Arc::clone(&input.store);
            Arc::new(move || {
                store
                    .allocate_agent_id_for_lifecycle_coordinator("pyrun")
                    .map_err(|error| crate::core::sqlite::ControlDbError::new(error.message))
            })
        },
        now: Arc::new(crate::utils::iso_time::now_iso_string),
        process_identity,
        session_path: persistence.session_path.clone(),
    });
    Ok(DetachedJobLifecycleController::new(
        DetachedJobLifecycleControllerOptions {
            artifact_root: node_dirname(&persistence.session_path),
            control_db_path: persistence.control_db_path.clone(),
            coordinator: Arc::new(coordinator),
            owner_agent_id: input.ctx.multi_agent_agent_id().ok().flatten(),
            owner_session_id: {
                let session = input.ctx.session_manager().map_err(|error| error.message)?;
                session.get_session_id().to_owned()
            },
            session_path: persistence.session_path.clone(),
            store: Arc::clone(&input.store),
            write_bash_launch_manifest: None,
        },
    ))
}

struct LaunchedRunner {
    activation_path: JsString,
    artifacts: crate::core::detached_job_runner::DetachedJobArtifacts,
    bridge_request_path: JsString,
    bridge_response_path: JsString,
    foreground_completion_path: JsString,
    job_id: JsString,
    process_identity: ProcessIdentity,
    runner_pid: u32,
    script_path: JsString,
}

fn write_durable_pyrun_script(path: &JsString, code: &JsString) -> Result<(), JsString> {
    let temporary_path = js_format!("{}.tmp-{}", path, std::process::id());
    {
        use std::io::Write;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(js_json::encode_node_utf8(&temporary_path))
            .map_err(|error| error.to_string())?;
        file.write_all(js_json::encode_node_utf8(code).as_bytes())
            .map_err(|error| error.to_string())?;
    }
    let file = std::fs::File::open(js_json::encode_node_utf8(&temporary_path)).map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    std::fs::rename(
        js_json::encode_node_utf8(&temporary_path),
        js_json::encode_node_utf8(path),
    )
    .map_err(|error| error.to_string())?;
    for directory in [node_dirname(path), node_dirname(&node_dirname(path))] {
        let file = std::fs::File::open(js_json::encode_node_utf8(&directory)).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn launch_foreground_pyrun_runner(
    input: &DurablePyrunEvaluationInput,
    persistence: &PersistenceTarget,
    controller: &DetachedJobLifecycleController,
    started_at: f64,
) -> Result<LaunchedRunner, CallbackError> {
    let job_id = input
        .agent_id
        .clone()
        .ok_or_else(|| "Detached Pyrun requires a persisted tool execution agent ID".to_owned())?;
    let artifacts = controller.create_artifacts(&job_id).map_err(|error| error.message())?;
    let directory = artifacts.directory.clone();
    let activation_path = node_join(&directory, "activation.json");
    let bridge_request_path = node_join(&directory, "foreground-bridge-requests.jsonl");
    let bridge_response_path = node_join(&directory, "foreground-bridge-responses.jsonl");
    let foreground_completion_path = node_join(&directory, "foreground-completed");
    let manifest_path = node_join(&directory, "launch.json");
    let script_path = node_join(&directory, "script.py");
    write_durable_pyrun_script(&script_path, &input.params.code)?;
    let runner_pid = launch_detached_pyrun_runner(&manifest_path)?;
    let process_identity = read_process_identity(runner_pid).map_err(|error| error.message)?;
    let session = input.ctx.session_manager().map_err(|error| error.message)?;
    let eval_context = eval_context(&input.ctx).map_err(|error| error.message)?;
    let supervisor_identity = read_process_identity(std::process::id()).map_err(|error| error.message)?;
    write_detached_pyrun_launch_manifest(
        &manifest_path,
        &DetachedPyrunLaunchManifestData {
            activation_path: activation_path.clone(),
            artifacts: artifacts.clone(),
            bridge_request_path: bridge_request_path.clone(),
            bridge_response_path: bridge_response_path.clone(),
            control_db_path: persistence.control_db_path.clone(),
            foreground_completion_path: foreground_completion_path.clone(),
            params: create_canonical_pyrun_eval_params(&input.params, &eval_context, input.pi_bridge_enabled)?,
            runner_address: crate::core::session_control_db::RuntimeMailboxAddress {
                agent_id: Some(job_id.clone().into()),
                session_id: session.get_session_id().to_owned().into(),
            },
            runner_options: input.runner_options.clone(),
            runner_process_identity: process_identity.clone(),
            session_path: persistence.session_path.clone(),
            started_at,
            supervisor_process_identity: supervisor_identity,
            tool_call_id: input.tool_call_id.clone(),
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

/// `readNewJsonLines`: complete lines only; a malformed complete line throws
/// like `JSON.parse`.
fn read_new_json_lines(
    path: &JsString,
    cursor: &mut JsonLineCursor,
    byte_limit: Option<usize>,
) -> Result<Vec<Value>, JsString> {
    let data = match std::fs::read(js_json::encode_node_utf8(path)) {
        Ok(data) => data,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.to_string().into()),
    };
    let size = data.len() as u64;
    if size <= cursor.offset {
        return Ok(Vec::new());
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
        if !line.is_empty() {
            let text = String::from_utf8_lossy(&line);
            values.push(js_json::parse_lossless(text.as_ref()).map_err(|error| error.into_message())?);
        }
    }
    if line_start < chunk.len() {
        cursor.fragments.extend_from_slice(&chunk[line_start..]);
    }
    Ok(values)
}

struct DetachState {
    cwd: JsString,
    job_id: JsString,
    process_identity: ProcessIdentity,
    runner_pid: u32,
    script_path: JsString,
    activation_path: JsString,
    tool_call_id: Option<JsString>,
    controller: DetachedJobLifecycleController,
    ownership: Mutex<Option<DetachedJobOwnership>>,
    activated: AtomicBool,
}

impl DetachState {
    fn ownership(&self) -> Option<DetachedJobOwnership> {
        self.ownership
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    fn is_activated(&self) -> bool {
        self.activated.load(Ordering::SeqCst)
    }
}

fn detach_state_fn(state: &DetachState) -> Result<bool, pi_js_compat::callback_error::CallbackError> {
    if state.ownership().is_some() || !is_process_identity_alive(&state.process_identity) {
        return Ok(false);
    }
    let ownership = state.controller.register(&RegisterDetachedJobInput {
        agent_type: DetachedJobAgentType::Pyrun,
        cwd: state.cwd.clone(),
        display_name: "Pyrun evaluation".into(),
        job_id: state.job_id.clone(),
        process_identity: state.process_identity.clone(),
        worker_handle_id: state.runner_pid.to_string().into(),
        tool_call_id: state.tool_call_id.clone(),
        detached: Some(true),
        file_refs: Some(vec![crate::core::multi_agent_store::AgentFileReference {
            label: Some("Pyrun script".into()),
            path: state.script_path.clone().into(),
        }]),
    });
    // Frozen detached-evaluation.ts:465-478 registers outside the activation catch.
    let ownership =
        ownership.map_err(|error| pi_js_compat::callback_error::CallbackError::from_error(error.message(), error))?;
    if write_detached_pyrun_activation(&state.activation_path, &ownership.identity).is_err() {
        let _ = terminate_foreground_runner(state.runner_pid, rustix::process::Signal::KILL);
        return Ok(false);
    }
    state.activated.store(true, Ordering::SeqCst);
    *state
        .ownership
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(ownership);
    Ok(true)
}

fn cancel_pyrun_evaluation(state: &DetachState) -> Result<(), HandlerError> {
    match state.ownership() {
        Some(ownership) => state
            .controller
            .cancel(&ownership, Some(&"Pyrun tool call aborted".into()))
            .map(|_| ())
            .map_err(|error| HandlerError::new(error.message())),
        None => terminate_foreground_runner(state.runner_pid, rustix::process::Signal::TERM).map_err(HandlerError::new),
    }
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
        .find(|record| record.get("kind").and_then(Value::as_string) == Some(&JsString::from("error")))
        .and_then(|record| record.get("error").and_then(Value::as_string))
    {
        std::fs::write(
            js_json::encode_node_utf8(&runner.foreground_completion_path),
            "failed\n",
        )
        .map_err(|error| HandlerError::new(error.to_string()))?;
        return Err(HandlerError::new(error));
    }
    let Some(result) = result else {
        return Ok(None);
    };
    std::fs::write(
        js_json::encode_node_utf8(&runner.foreground_completion_path),
        "completed\n",
    )
    .map_err(|error| HandlerError::new(error.to_string()))?;
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

fn detached_result(params: &PyrunEvalParams, job_id: &JsString, log_path: &JsString) -> AgentToolResult {
    AgentToolResult {
        content: vec![TextOrImageContent::Text(TextContent::new(
            params.code.clone()
                + js_format!(
                    "\n\nPyrun evaluation moved to background as job {}. Output will be written to {}.",
                    job_id,
                    log_path
                ),
        ))],
        details: Some(
            json!({
                "backgroundJobId": job_id,
                "executed": params.code,
                "type": "detached"
            })
            .into(),
        ),
        is_error: None,
        terminate: None,
    }
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

fn format_terminal_agent_error(params: &PyrunEvalParams, agent: &AgentNode) -> AgentToolResult {
    let error = agent
        .error
        .as_ref()
        .map(|error| JsString::from(&error.message))
        .or_else(|| agent.result.as_ref().and_then(|result| result.summary.clone()))
        .unwrap_or_else(|| js_format!("Pyrun evaluation {}", lifecycle_name(agent.lifecycle)).into());
    AgentToolResult {
        content: vec![TextOrImageContent::Text(TextContent::new(
            params.code.clone() + "\n\nError: " + &error,
        ))],
        details: Some(json!({"error": error, "executed": params.code, "type": "error"}).into()),
        is_error: Some(true),
        terminate: None,
    }
}

fn pyrun_console_records(records: &[Value]) -> Vec<Value> {
    records
        .iter()
        .filter(|record| {
            record.get("kind").and_then(Value::as_string) == Some(&JsString::from("progress"))
                && record["update"]["type"] == Value::String("console".into())
                && record["update"]["text"].is_string()
        })
        .map(|record| record["update"]["text"].clone())
        .collect()
}

fn format_lost_pyrun_runtime(params: &PyrunEvalParams, records: &[Value], output_path: &JsString) -> AgentToolResult {
    let error = "Pyrun runner exited before producing a result.";
    let result = CanonicalPyrunEvalResult {
        approval: None,
        console: Some(
            pyrun_console_records(records)
                .into_iter()
                .filter_map(|value| js_json::from_value(&value).ok())
                .collect(),
        ),
        error: Some(error.to_owned().into()),
        executed: Some(params.code.clone()),
        result_type: "error".into(),
        value: None,
    };
    let mut formatted = format_canonical_pyrun_eval_result(params, result);
    for item in &mut formatted.content {
        if let TextOrImageContent::Text(text) = item {
            text.text.push_str(js_format!("\nPreserved output: {}", output_path));
        }
    }
    if let Some(Value::Object(details)) = &mut formatted.details {
        details.insert("outputPath".into(), Value::String(output_path.to_owned().into()));
        details.insert("type".into(), Value::String("lost_runtime".to_owned().into()));
    }
    formatted
}

struct PyrunManifestCandidate {
    directory: JsString,
    manifest: DetachedPyrunLaunchManifest,
    manifest_path: JsString,
}

fn read_pyrun_manifest_candidate(directory: &JsString) -> Result<Option<PyrunManifestCandidate>, HandlerError> {
    let manifest_path = node_join(directory, "launch.json");
    if !Path::new(&js_json::encode_node_utf8(&manifest_path)).exists() {
        return Ok(None);
    }
    let manifest = read_detached_pyrun_launch_manifest(&manifest_path).map_err(HandlerError::new)?;
    Ok(Some(PyrunManifestCandidate {
        directory: directory.to_owned(),
        manifest,
        manifest_path,
    }))
}

fn read_complete_artifact_records(path: &JsString) -> Result<Vec<Value>, JsString> {
    if !Path::new(&js_json::encode_node_utf8(path)).exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(js_json::encode_node_utf8(path)).map_err(|error| error.to_string())?;
    let Some(last_newline) = text.rfind('\n') else {
        return Ok(Vec::new());
    };
    let mut records = Vec::new();
    for line in text[..last_newline].split('\n').filter(|line| !line.is_empty()) {
        records.push(js_json::parse_lossless(line).map_err(|error| error.into_message())?);
    }
    Ok(records)
}

fn restore_terminal_pyrun_result(records: &[Value]) -> Result<Option<CanonicalPyrunEvalResult>, JsString> {
    let mut terminal = None;
    for record in records {
        if record.get("kind").and_then(Value::as_string) == Some(&JsString::from("progress")) {
            continue;
        }
        if terminal.is_some() {
            return Err("Pyrun output artifact contains multiple terminal records".into());
        }
        terminal = Some(match record.get("kind").and_then(Value::as_string) {
            Some(kind) if kind == "result" => {
                js_json::from_value(&record.get("result").cloned().unwrap_or(Value::Null))
                    .map_err(|error| error.to_string())?
            }
            _ => CanonicalPyrunEvalResult {
                approval: None,
                console: Some(
                    pyrun_console_records(records)
                        .into_iter()
                        .filter_map(|value| js_json::from_value(&value).ok())
                        .collect(),
                ),
                error: record.get("error").and_then(Value::as_string).cloned(),
                executed: None,
                result_type: "error".into(),
                value: None,
            },
        });
    }
    Ok(terminal)
}

enum RestoredRunner {
    Aborted(Box<AgentNode>),
    Terminal(Box<CanonicalPyrunEvalResult>),
    Lost(Vec<Value>, JsString),
    Resume(LaunchedRunner),
}

async fn settle_cancelling_pyrun_job(
    persistence: &PersistenceTarget,
    manifest: &DetachedPyrunLaunchManifest,
    persisted: AgentNode,
) -> Result<AgentNode, HandlerError> {
    if is_process_identity_alive(&manifest.data.runner_process_identity) {
        terminate_foreground_runner(manifest.data.runner_process_identity.pid, rustix::process::Signal::KILL)
            .map_err(HandlerError::new)?;
    }
    let Some(identity) = read_detached_pyrun_activation(&manifest.data.activation_path).map_err(HandlerError::new)?
    else {
        return Ok(persisted);
    };
    // The runner may have died before writing any output; the terminal input
    // hashes the output file, so ensure it exists before building the transition.
    if !Path::new(&js_json::encode_node_utf8(&manifest.data.artifacts.output_path)).exists() {
        let mut options = std::fs::OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options
            .open(js_json::encode_node_utf8(&manifest.data.artifacts.output_path))
            .map_err(|error| HandlerError::new(error.to_string()))?;
    }
    let terminal_at = now_millis();
    let terminal_at_iso = JsString::from(crate::utils::iso_time::format_iso_millis(terminal_at));
    let terminal = create_detached_job_terminal_input(
        &manifest.data.artifacts,
        &identity,
        crate::core::detached_job_runner::DetachedJobOutcome::Aborted {
            reason: Some("Cancelled before session resume".into()),
        },
        &terminal_at_iso,
        Some((terminal_at - manifest.data.started_at as i64).max(0)),
        manifest.data.tool_call_id.as_ref(),
    )
    .map_err(|error| HandlerError::new(error.message()))?;
    let control_db_path = persistence.control_db_path.clone();
    let session_path = persistence.session_path.clone();
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
    if let Ok(FinalizeDetachedJobResult::Ok { terminal_agent, .. }) = finalized {
        return Ok(*terminal_agent);
    }
    Ok(
        read_multi_agent_agent(&persistence.control_db_path, &persistence.session_path, &persisted.id)
            .map_err(|error| HandlerError::new(error.message()))?
            .unwrap_or(persisted),
    )
}

async fn settle_cancelled_pyrun_candidate(
    persistence: &PersistenceTarget,
    candidate: &PyrunManifestCandidate,
    tool_call_id: &JsString,
    job_id: &JsString,
) -> Result<Option<AgentNode>, HandlerError> {
    let correlated = candidate.manifest.data.tool_call_id.is_none()
        || candidate.manifest.data.tool_call_id.as_ref() == Some(tool_call_id);
    if !correlated {
        return Ok(None);
    }
    let persisted = read_multi_agent_agent(&persistence.control_db_path, &persistence.session_path, job_id)
        .map_err(|error| HandlerError::new(error.message()))?;
    let Some(persisted) = persisted else {
        return Ok(None);
    };
    if persisted.lifecycle != AgentLifecycle::Cancelling {
        return Ok(None);
    }
    Ok(Some(
        settle_cancelling_pyrun_job(persistence, &candidate.manifest, persisted).await?,
    ))
}

fn restore_pyrun_candidate(
    candidate: PyrunManifestCandidate,
    expected_params: &super::runner::CanonicalPyrunEvalParams,
    tool_call_id: &JsString,
    job_id: &JsString,
) -> Result<RestoredRunner, HandlerError> {
    let manifest = candidate.manifest;
    let manifest_params =
        js_json::to_value(&manifest.data.params).map_err(|error| HandlerError::new(error.to_string()))?;
    let expected_params = js_json::to_value(expected_params).map_err(|error| HandlerError::new(error.to_string()))?;
    let manifest_params = js_json::stringify_lossless(&manifest_params);
    let expected_params = js_json::stringify_lossless(&expected_params);
    if manifest_params != expected_params {
        return Err(HandlerError::new(js_format!(
            "Pyrun tool-call artifact collision for {}",
            tool_call_id
        )));
    }
    let records = read_complete_artifact_records(&manifest.data.artifacts.output_path).map_err(HandlerError::new)?;
    if let Some(terminal) = restore_terminal_pyrun_result(&records).map_err(HandlerError::new)? {
        return Ok(RestoredRunner::Terminal(Box::new(terminal)));
    }
    if !is_process_identity_alive(&manifest.data.runner_process_identity) {
        return Ok(RestoredRunner::Lost(records, manifest.data.artifacts.output_path));
    }
    let script_path = node_join(&candidate.directory, "script.py");
    if !Path::new(&js_json::encode_node_utf8(&script_path)).exists() {
        return Err(HandlerError::new(js_format!(
            "Pyrun script artifact is missing: {}",
            script_path
        )));
    }
    Ok(RestoredRunner::Resume(LaunchedRunner {
        activation_path: manifest.data.activation_path,
        artifacts: manifest.data.artifacts,
        bridge_request_path: manifest.data.bridge_request_path,
        bridge_response_path: manifest.data.bridge_response_path,
        foreground_completion_path: manifest.data.foreground_completion_path,
        job_id: job_id.to_owned(),
        process_identity: manifest.data.runner_process_identity.clone(),
        runner_pid: manifest.data.runner_process_identity.pid,
        script_path,
    }))
}

async fn restore_foreground_pyrun_runner(
    input: &DurablePyrunEvaluationInput,
    persistence: &PersistenceTarget,
) -> Result<Option<RestoredRunner>, HandlerError> {
    let agent_id = input
        .agent_id
        .clone()
        .ok_or_else(|| HandlerError::new("Detached Pyrun requires a persisted tool execution agent ID"))?;
    let root = pyrun_artifact_root(&persistence.session_path).map_err(HandlerError::new)?;
    let directory = node_join(&root, &agent_id);
    let Some(candidate) = read_pyrun_manifest_candidate(&directory)? else {
        return Ok(None);
    };
    if !candidate
        .manifest
        .data
        .runner_address
        .agent_id
        .as_ref()
        .is_some_and(|id| id == &agent_id)
    {
        return Err(HandlerError::new(js_format!(
            "Pyrun launch manifest agent ID mismatch: {}",
            candidate.manifest_path
        )));
    }
    // TypeScript template interpolation renders a missing tool call id as
    // `undefined` in the collision message.
    let tool_call_id = input.tool_call_id.clone().unwrap_or_else(|| "undefined".into());
    if let Some(agent) = settle_cancelled_pyrun_candidate(persistence, &candidate, &tool_call_id, &agent_id).await? {
        return Ok(Some(RestoredRunner::Aborted(Box::new(agent))));
    }
    if candidate.manifest.data.tool_call_id != input.tool_call_id {
        return Err(HandlerError::new(js_format!(
            "Pyrun tool-call artifact collision for {}",
            tool_call_id
        )));
    }
    let eval_context = eval_context(&input.ctx)?;
    let expected_params = create_canonical_pyrun_eval_params(&input.params, &eval_context, input.pi_bridge_enabled)?;
    restore_pyrun_candidate(candidate, &expected_params, &tool_call_id, &agent_id).map(Some)
}

async fn respond_to_foreground_bridge_request(
    input: &DurablePyrunEvaluationInput,
    runner: &LaunchedRunner,
    request: &Value,
) -> Result<(), HandlerError> {
    let Some(request_id) = request.get("requestId").and_then(Value::as_string) else {
        return Ok(());
    };
    if !claim_foreground_bridge_request(&runner.artifacts.directory, request_id).map_err(HandlerError::new)? {
        return Ok(());
    }
    let mut claimed = Map::new();
    claimed.insert("claimed".into(), Value::Bool(true));
    claimed.insert("requestId".into(), Value::String(request_id.to_owned().into()));
    append_artifact_record(&runner.bridge_response_path, &Value::Object(claimed))
        .map_err(|error| HandlerError::new(error.to_string()))?;
    let method = request
        .get("method")
        .and_then(Value::as_string)
        .cloned()
        .unwrap_or_default();
    let params = request.get("params").cloned().unwrap_or(Value::Null);
    let response = (input.dispatch_pi_request)(
        super::runner::PiRequest { method, params },
        input.ctx.clone(),
        input.signal.clone(),
        input
            .tool_call_id
            .as_ref()
            .map(|id| {
                id.to_utf8().map_err(|_| {
                    HandlerError::new("Persisted Pyrun provider tool-call ID contains an unpaired UTF-16 unit")
                })
            })
            .transpose()?,
    )
    .await;
    let mut body = Map::new();
    body.insert("requestId".into(), Value::String(request_id.to_owned().into()));
    match response {
        Ok(Some(result)) => {
            body.insert("result".into(), result);
        }
        Ok(None) => {}
        Err(error) => {
            body.insert("error".into(), Value::String(error.message));
        }
    }
    append_artifact_record(&runner.bridge_response_path, &Value::Object(body))
        .map_err(|error| HandlerError::new(error.to_string()))
}

struct DetachFn {
    detach: Box<dyn Fn() -> Result<bool, pi_js_compat::callback_error::CallbackError> + Send + Sync>,
}

impl ToolDetachHandle for DetachFn {
    fn detach(&self) -> Result<bool, pi_js_compat::callback_error::CallbackError> {
        (self.detach)()
    }
}

async fn observe_detachable_pyrun_evaluation(
    input: DurablePyrunEvaluationInput,
    controller: DetachedJobLifecycleController,
    runner: LaunchedRunner,
) -> Result<AgentToolResult, HandlerError> {
    let mut bridge_cursor = JsonLineCursor::new();
    let mut output_cursor = JsonLineCursor::new();
    let mut result: Option<CanonicalPyrunEvalResult> = None;
    let mut terminal_agent: Option<AgentNode> = None;
    let report_progress = create_pyrun_progress_reporter(input.on_update.clone());
    let mut progress = super::detached_progress::ArtifactProgressAccumulator::new(report_progress, ARTIFACT_POLL_MS);
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
    let mut cancel_applied = false;
    let loop_result = async {
        loop {
            let ownership = detach_state.ownership();
            if ownership.is_none() {
                let requests = read_new_json_lines(&runner.bridge_request_path, &mut bridge_cursor, None)
                    .map_err(HandlerError::new)?;
                for request in requests {
                    respond_to_foreground_bridge_request(&input, &runner, &request).await?;
                }
            }
            let records = read_new_json_lines(
                &runner.artifacts.output_path,
                &mut output_cursor,
                Some(ARTIFACT_READ_BYTE_LIMIT),
            )
            .map_err(HandlerError::new)?;
            if let Some(consumed) = progress.consume(&records)? {
                result = Some(js_json::from_value(&consumed).map_err(|error| HandlerError::new(error.to_string()))?);
            }
            let ownership = detach_state.ownership();
            if let Some(formatted) =
                settle_foreground_evaluation(&input, &runner, &records, result.as_ref(), ownership.is_some())?
            {
                return Ok(formatted);
            }
            if ownership.is_none() {
                check_foreground_runner_liveness(&runner.process_identity, &mut next_liveness)?;
            }
            if let Some(ownership) = &ownership
                && let Some(agent) = controller
                    .observe(&ownership.agent.id)
                    .map_err(|error| HandlerError::new(error.message()))?
            {
                terminal_agent = Some(agent);
            }
            if terminal_agent
                .as_ref()
                .is_some_and(|agent| !crate::core::multi_agent_store::is_active_lifecycle(agent.lifecycle))
            {
                break;
            }
            if ownership.is_some() && detach_state.is_activated() {
                return Ok(detached_result(
                    &input.params,
                    &runner.job_id,
                    &runner.artifacts.output_path,
                ));
            }
            let sleep = tokio::time::sleep(Duration::from_millis(ARTIFACT_POLL_MS));
            match &input.signal {
                Some(signal) if !cancel_applied => {
                    tokio::select! {
                        _ = signal.wait() => {
                            cancel_applied = true;
                            cancel_pyrun_evaluation(&detach_state)?;
                        }
                        () = sleep => {}
                    }
                }
                _ => sleep.await,
            }
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
    progress.close();
    subscription.unregister();
    loop_result
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
    let restored = restore_foreground_pyrun_runner(&input, &persistence).await?;
    let runner = match restored {
        Some(RestoredRunner::Aborted(agent)) => {
            return Ok(format_terminal_agent_error(&input.params, &agent));
        }
        Some(RestoredRunner::Terminal(result)) => {
            return Ok(format_canonical_pyrun_eval_result(&input.params, *result));
        }
        Some(RestoredRunner::Lost(records, output_path)) => {
            return Ok(format_lost_pyrun_runtime(&input.params, &records, &output_path));
        }
        Some(RestoredRunner::Resume(runner)) => runner,
        None => {
            launch_foreground_pyrun_runner(&input, &persistence, &controller, started_at).map_err(HandlerError::from)?
        }
    };
    observe_detachable_pyrun_evaluation(input, controller, runner).await
}

#[cfg(test)]
mod js_value_tests {
    use super::*;

    #[test]
    fn fragmented_artifact_lines_keep_unpaired_units() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("output.jsonl");
        std::fs::write(&path, b"{\"value\":\"\\ud800\"}\n").unwrap();
        let mut cursor = JsonLineCursor::new();
        assert!(
            read_new_json_lines(&path.to_str().unwrap().into(), &mut cursor, Some(12))
                .unwrap()
                .is_empty()
        );
        let records = read_new_json_lines(&path.to_str().unwrap().into(), &mut cursor, None).unwrap();
        assert_eq!(
            records,
            vec![pi_js_compat::js_json!({"value": JsString::from_utf16(vec![0xd800])})]
        );
    }

    #[test]
    fn persisted_js_equivalent_params_do_not_report_a_collision() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("output.log");
        std::fs::write(
            &output,
            "{\"kind\":\"result\",\"result\":{\"type\":\"completed\",\"value\":42}}\n",
        )
        .unwrap();
        let params: super::super::runner::CanonicalPyrunEvalParams =
            serde_json::from_str(r#"{"code":"x","pi":{"2":2.0,"1":1.0}}"#).unwrap();
        let data = js_json::from_value(&pi_js_compat::js_json!({
            "activationPath":"activation", "artifacts":{"directory":directory.path(),"outputPath":output},
            "bridgeRequestPath":"requests", "bridgeResponsePath":"responses", "controlDbPath":"db", "foregroundCompletionPath":"completion",
            "params":params, "runnerAddress":{"sessionId":"s","agentId":"job"}, "runnerOptions":{},
            "runnerProcessIdentity":{"pid":123,"startTimeTicks":1}, "sessionPath":"session", "startedAt":1,
            "supervisorProcessIdentity":{"pid":124,"startTimeTicks":1}, "toolCallId":"call"
        })).unwrap();
        let path = directory.path().join("launch.json");
        super::super::detached_runner::write_detached_pyrun_launch_manifest(&path.to_str().unwrap().into(), &data)
            .unwrap();
        let candidate = read_pyrun_manifest_candidate(&directory.path().to_str().unwrap().into())
            .unwrap()
            .unwrap();
        let restored = restore_pyrun_candidate(candidate, &params, &"call".into(), &"job".into());
        assert!(
            matches!(restored, Ok(RestoredRunner::Terminal(_))),
            "{:?}",
            restored.err().map(|error| error.message)
        );
    }
}
