//! Port of `packages/coding-agent/extensions/pyrun/src/runner.ts`.
//!
//! The canonical Pyrun runner is an external process speaking one JSON object
//! per line. This client mirrors the TypeScript `PyrunRunnerClient`: one
//! long-lived child per client, FIFO request/response correlation, progress
//! updates, `pi_request` round-trips, and process-group termination.

use std::collections::VecDeque;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_format;
use pi_js_compat::js_json::{self, JsObject, JsString, JsValue as Value};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, Command};
use tokio::sync::{Mutex as AsyncMutex, oneshot};

use crate::core::extensions::types::{BoxFuture, HandlerError};
use crate::utils::child_process::node_spawn_error_message;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CanonicalPyrunEvalParams {
    pub code: JsString,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<JsString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pi: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pi_bridge: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_console: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum CanonicalPyrunConsoleEntry {
    Text(JsString),
    Leveled { level: JsString, message: JsString },
}

impl<'de> Deserialize<'de> for CanonicalPyrunConsoleEntry {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match Value::deserialize(deserializer)? {
            Value::String(text) => Ok(Self::Text(text)),
            Value::Object(entries) => {
                let level = entries
                    .get("level")
                    .and_then(Value::as_string)
                    .cloned()
                    .ok_or_else(|| serde::de::Error::custom("console level must be a string"))?;
                let message = entries
                    .get("message")
                    .and_then(Value::as_string)
                    .cloned()
                    .ok_or_else(|| serde::de::Error::custom("console message must be a string"))?;
                Ok(Self::Leveled { level, message })
            }
            _ => Err(serde::de::Error::custom(
                "console entry must be text or a leveled message",
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalPyrunApprovalRequest {
    pub args: Value,
    pub id: JsString,
    pub summary: JsString,
    pub tool: JsString,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CanonicalPyrunEvalResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval: Option<CanonicalPyrunApprovalRequest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub console: Option<Vec<CanonicalPyrunConsoleEntry>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<JsString>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executed: Option<JsString>,
    #[serde(rename = "type")]
    pub result_type: JsString,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
}

/// Progress messages keep their source key order because the extension forwards
/// them verbatim as tool-result details.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CanonicalPyrunProgressUpdate {
    pub raw: Value,
}

impl CanonicalPyrunProgressUpdate {
    pub fn message_type(&self) -> JsString {
        self.raw
            .get("type")
            .and_then(Value::as_string)
            .cloned()
            .unwrap_or_default()
    }

    pub fn method(&self) -> Option<&JsString> {
        self.raw.get("method").and_then(Value::as_string)
    }

    pub fn message(&self) -> Option<&JsString> {
        self.raw.get("message").and_then(Value::as_string)
    }

    pub fn text(&self) -> Option<&JsString> {
        self.raw.get("text").and_then(Value::as_string)
    }

    pub fn output(&self) -> Option<&JsString> {
        self.raw.get("output").and_then(Value::as_string)
    }

    pub fn status(&self) -> Option<&JsString> {
        self.raw.get("status").and_then(Value::as_string)
    }

    pub fn params(&self) -> Option<&Value> {
        self.raw.get("params")
    }

    pub fn value(&self) -> Option<&Value> {
        self.raw.get("value")
    }

    pub fn with_text(&self, text: JsString) -> Self {
        let mut raw = self.raw.clone();
        if let Some(object) = raw.as_object_mut() {
            object.insert("text".into(), Value::String(text));
        }
        Self { raw }
    }
}

/// `undefined` (no handler result) is `None`; a JSON `null` result is `Some(Value::Null)`.
pub type PyrunPiRequestHandler =
    Arc<dyn Fn(PiRequest) -> BoxFuture<'static, Result<Option<Value>, HandlerError>> + Send + Sync>;

#[derive(Debug, Clone, PartialEq)]
pub struct PiRequest {
    pub method: JsString,
    pub params: Value,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PyrunRunnerOptions {
    pub args: Option<Vec<JsString>>,
    pub command: Option<JsString>,
    pub detached: Option<bool>,
    #[serde(default, with = "runner_environment")]
    pub env: Option<indexmap::IndexMap<JsString, JsString>>,
    pub inherit_env: Option<bool>,
}

mod runner_environment {
    use super::*;

    pub fn serialize<S: serde::Serializer>(
        env: &Option<indexmap::IndexMap<JsString, JsString>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let value = env.as_ref().map(|entries| {
            Value::Object(
                entries
                    .iter()
                    .map(|(key, value)| (key.clone(), Value::String(value.clone())))
                    .collect(),
            )
        });
        value.serialize(serializer)
    }

    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<indexmap::IndexMap<JsString, JsString>>, D::Error> {
        let Some(value) = Option::<Value>::deserialize(deserializer)? else {
            return Ok(None);
        };
        let Value::Object(entries) = value else {
            return Err(serde::de::Error::custom("runner environment must be an object"));
        };
        entries
            .into_iter()
            .map(|(key, value)| {
                let Value::String(value) = value else {
                    return Err(serde::de::Error::custom("runner environment values must be strings"));
                };
                Ok((key, value))
            })
            .collect::<Result<_, _>>()
            .map(Some)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPyrunRunnerOptions {
    pub args: Vec<JsString>,
    pub command: JsString,
    pub env: Vec<(JsString, JsString)>,
}

fn parse_runner_args(value: Option<&JsString>) -> Result<Option<Vec<JsString>>, PyrunRunnerError> {
    let Some(value) = value.filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let parsed = js_json::parse_lossless(value).map_err(|error| PyrunRunnerError::new(error.into_message()))?;
    let Some(items) = parsed.as_array() else {
        return Err(PyrunRunnerError::new(
            "PI_PYRUN_RUNNER_ARGS must be a JSON string array",
        ));
    };
    let mut args = Vec::with_capacity(items.len());
    for item in items {
        let Some(item) = item.as_string() else {
            return Err(PyrunRunnerError::new(
                "PI_PYRUN_RUNNER_ARGS must be a JSON string array",
            ));
        };
        args.push(item.to_owned());
    }
    Ok(Some(args))
}

fn lookup_runner_environment(env: Option<&[(JsString, JsString)]>, key: &str) -> Option<JsString> {
    match env {
        Some(env) => env
            .iter()
            .rev()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.clone()),
        None => pi_js_compat::process_env::get_native(key),
    }
}

/// Command availability does not parse the runner's arguments.
pub fn resolve_pyrun_runner_command(env: Option<&[(JsString, JsString)]>) -> JsString {
    lookup_runner_environment(env, "PI_PYRUN_RUNNER_COMMAND")
        .or_else(|| lookup_runner_environment(env, "PI_PYRUN_RUNNER"))
        .unwrap_or_else(|| "pyrun-jsonl".into())
}

/// `resolvePyrunRunnerOptions`.
pub fn resolve_pyrun_runner_options(
    env: Option<&[(JsString, JsString)]>,
) -> Result<ResolvedPyrunRunnerOptions, PyrunRunnerError> {
    let args = parse_runner_args(lookup_runner_environment(env, "PI_PYRUN_RUNNER_ARGS").as_ref())?.unwrap_or_default();
    Ok(ResolvedPyrunRunnerOptions {
        args,
        command: resolve_pyrun_runner_command(env),
        env: Vec::new(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PyrunRunnerError {
    message: JsString,
}

impl PyrunRunnerError {
    pub fn message(&self) -> &JsString {
        &self.message
    }
    pub fn new(message: impl Into<JsString>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for PyrunRunnerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&js_json::encode_node_utf8(&self.message))
    }
}

impl std::error::Error for PyrunRunnerError {}

/// `onProgress`: a callback failure rejects the pending evaluation, like the
/// TypeScript `try`/`catch` around `pending.onProgress?.(message)`.
pub type PyrunProgressCallback =
    Arc<dyn Fn(CanonicalPyrunProgressUpdate) -> Result<(), PyrunRunnerError> + Send + Sync>;

struct PendingRequest {
    generation: u64,
    on_pi_request: Option<PyrunPiRequestHandler>,
    on_progress: Option<PyrunProgressCallback>,
    resolve: oneshot::Sender<Result<CanonicalPyrunEvalResult, PyrunRunnerError>>,
    abort_task: Option<tokio::task::JoinHandle<()>>,
}

struct ProcessHandle {
    generation: u64,
    pid: Option<u32>,
    stdin: Arc<AsyncMutex<ChildStdin>>,
    stderr: Arc<Mutex<JsString>>,
}

struct RunnerState {
    process: Option<ProcessHandle>,
    pending: VecDeque<PendingRequest>,
}

#[derive(Clone)]
pub struct PyrunRunnerClient {
    options: PyrunRunnerOptions,
    state: Arc<Mutex<RunnerState>>,
    next_generation: Arc<AtomicU64>,
}

fn now_generation(counter: &AtomicU64) -> u64 {
    counter.fetch_add(1, Ordering::SeqCst) + 1
}

fn should_detach(options: &PyrunRunnerOptions) -> bool {
    options.detached.unwrap_or(!cfg!(windows))
}

fn spawn_error(command: &JsString, error: &std::io::Error) -> PyrunRunnerError {
    PyrunRunnerError::new(node_spawn_error_message("spawn", command, error))
}

impl PyrunRunnerClient {
    pub fn new(options: PyrunRunnerOptions) -> Self {
        Self {
            options,
            state: Arc::new(Mutex::new(RunnerState {
                process: None,
                pending: VecDeque::new(),
            })),
            next_generation: Arc::new(AtomicU64::new(0)),
        }
    }

    /// `evaluate`.
    pub async fn evaluate(
        &self,
        params: CanonicalPyrunEvalParams,
        on_progress: Option<PyrunProgressCallback>,
        signal: Option<AbortSignal>,
        on_pi_request: Option<PyrunPiRequestHandler>,
    ) -> Result<CanonicalPyrunEvalResult, PyrunRunnerError> {
        if signal.as_ref().is_some_and(AbortSignal::aborted) {
            return Err(PyrunRunnerError::new("Pyrun evaluation aborted"));
        }
        let process = self.ensure_process().await?;
        let value = js_json::to_value(&params).map_err(|error| PyrunRunnerError::new(error.to_string()))?;
        let payload = js_json::stringify_lossless(&value);
        let (sender, receiver) = oneshot::channel();
        let generation = process.generation;
        let abort_task = signal.map(|signal| {
            let state = Arc::clone(&self.state);
            let pid = process.pid;
            let detached = should_detach(&self.options);
            tokio::spawn(async move {
                signal.wait().await;
                terminate_generation(&state, generation, pid, detached);
                let mut state = state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                reject_generation_locked(
                    &mut state,
                    generation,
                    PyrunRunnerError::new("Pyrun evaluation aborted"),
                );
            })
        });
        {
            let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            state.pending.push_back(PendingRequest {
                generation,
                on_pi_request,
                on_progress,
                resolve: sender,
                abort_task,
            });
        }
        {
            let mut stdin = process.stdin.lock().await;
            if let Err(error) = stdin.write_all(format!("{payload}\n").as_bytes()).await {
                drop(stdin);
                self.reject_generation(generation, PyrunRunnerError::new(error.to_string()));
            } else if let Err(error) = stdin.flush().await {
                drop(stdin);
                self.reject_generation(generation, PyrunRunnerError::new(error.to_string()));
            }
        }
        match receiver.await {
            Ok(result) => result,
            Err(_) => Err(PyrunRunnerError::new("Pyrun evaluation aborted")),
        }
    }

    /// `dispose`.
    pub fn dispose(&self) {
        let process = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .process
            .take();
        if let Some(process) = process {
            terminate_process(process.pid, should_detach(&self.options));
        }
    }

    fn reject_generation(&self, generation: u64, error: PyrunRunnerError) {
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        reject_generation_locked(&mut state, generation, error);
    }

    async fn ensure_process(&self) -> Result<ProcessHandle, PyrunRunnerError> {
        if let Some(process) = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .process
            .as_ref()
        {
            return Ok(ProcessHandle {
                generation: process.generation,
                pid: process.pid,
                stdin: Arc::clone(&process.stdin),
                stderr: Arc::clone(&process.stderr),
            });
        }

        let resolved = resolve_pyrun_runner_options(None)?;
        let command = self.options.command.clone().unwrap_or(resolved.command);
        let args = self.options.args.clone().unwrap_or(resolved.args);
        let detached = should_detach(&self.options);
        let mut process_command = Command::new(js_json::encode_node_utf8(&command));
        process_command
            .args(args.iter().map(js_json::encode_node_utf8))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if detached {
            process_command.process_group(0);
        }
        if self.options.inherit_env == Some(false) {
            process_command.env_clear();
        }
        if let Some(env) = &self.options.env {
            for (name, value) in env {
                process_command.env(js_json::encode_node_utf8(name), js_json::encode_node_utf8(value));
            }
        }
        let mut child = process_command.spawn().map_err(|error| spawn_error(&command, &error))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| PyrunRunnerError::new("Pyrun runner stdin is unavailable"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| PyrunRunnerError::new("Pyrun runner stdout is unavailable"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| PyrunRunnerError::new("Pyrun runner stderr is unavailable"))?;
        let generation = now_generation(&self.next_generation);
        let pid = child.id();
        let stdin = Arc::new(AsyncMutex::new(stdin));
        let stderr_buffer = Arc::new(Mutex::new(JsString::default()));
        let process = ProcessHandle {
            generation,
            pid,
            stdin: Arc::clone(&stdin),
            stderr: Arc::clone(&stderr_buffer),
        };
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .process = Some(ProcessHandle {
            generation,
            pid,
            stdin: Arc::clone(&stdin),
            stderr: Arc::clone(&stderr_buffer),
        });

        let state = Arc::clone(&self.state);
        let stderr_for_reader = Arc::clone(&stderr_buffer);
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
                let trimmed = line.trim_end_matches(['\n', '\r']);
                if trimmed.is_empty() || trimmed.chars().all(char::is_whitespace) {
                    continue;
                }
                handle_stdout_line(&state, &stdin, generation, trimmed).await;
            }
        });
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
                let mut buffer = stderr_for_reader
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                buffer.push_str(&line);
            }
        });
        let state = Arc::clone(&self.state);
        let stderr_for_exit = Arc::clone(&stderr_buffer);
        tokio::spawn(async move {
            let status = child.wait().await;
            if detached && let Some(pid) = pid {
                terminate_process(Some(pid), true);
            }
            let mut state = state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            if state
                .process
                .as_ref()
                .is_some_and(|process| process.generation == generation)
            {
                state.process = None;
            }
            if !state.pending.iter().any(|request| request.generation == generation) {
                return;
            }
            let stderr = stderr_for_exit
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .trim()
                .to_owned();
            let reason = match status {
                Ok(status) => match exit_signal(&status) {
                    Some(signal) => format!("signal {}", signal_name(signal)),
                    None => format!("exit code {}", status.code().unwrap_or(-1)),
                },
                Err(error) => format!("exit code unknown ({error})"),
            };
            let suffix = if stderr.is_empty() {
                JsString::default()
            } else {
                js_format!("\n{}", stderr)
            };
            reject_generation_locked(
                &mut state,
                generation,
                PyrunRunnerError::new(js_format!("Pyrun runner exited with {}{}", reason, suffix)),
            );
        });
        Ok(process)
    }
}

fn exit_signal(status: &std::process::ExitStatus) -> Option<i32> {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        status.signal()
    }
    #[cfg(not(unix))]
    {
        let _ = status;
        None
    }
}

fn signal_name(signal: i32) -> String {
    match signal {
        1 => "SIGHUP".to_owned(),
        2 => "SIGINT".to_owned(),
        3 => "SIGQUIT".to_owned(),
        6 => "SIGABRT".to_owned(),
        9 => "SIGKILL".to_owned(),
        13 => "SIGPIPE".to_owned(),
        15 => "SIGTERM".to_owned(),
        other => other.to_string(),
    }
}

fn terminate_process(pid: Option<u32>, detached: bool) {
    #[cfg(unix)]
    {
        use rustix::process::{Pid, Signal, kill_process, kill_process_group};
        let Some(pid) = pid.and_then(|pid| Pid::from_raw(i32::try_from(pid).ok()?)) else {
            return;
        };
        if detached {
            let _ = kill_process_group(pid, Signal::TERM);
        } else {
            let _ = kill_process(pid, Signal::TERM);
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (pid, detached);
    }
}

fn terminate_generation(state: &Arc<Mutex<RunnerState>>, generation: u64, pid: Option<u32>, detached: bool) {
    let mut state = state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if state
        .process
        .as_ref()
        .is_some_and(|process| process.generation == generation)
    {
        state.process = None;
    }
    drop(state);
    terminate_process(pid, detached);
}

fn reject_generation_locked(state: &mut RunnerState, generation: u64, error: PyrunRunnerError) {
    let mut rejected = Vec::new();
    let mut index = 0;
    while index < state.pending.len() {
        if state.pending[index].generation == generation {
            if let Some(request) = state.pending.remove(index) {
                rejected.push(request);
            }
        } else {
            index += 1;
        }
    }
    for mut request in rejected {
        if let Some(task) = request.abort_task.take() {
            task.abort();
        }
        let _ = request.resolve.send(Err(error.clone()));
    }
}

fn is_final_result(value: &Value) -> bool {
    matches!(
        value.get("type").and_then(Value::as_string),
        Some(kind) if kind == "completed" || kind == "error" || kind == "needs_approval"
    )
}

async fn handle_stdout_line(
    state: &Arc<Mutex<RunnerState>>,
    stdin: &Arc<AsyncMutex<ChildStdin>>,
    generation: u64,
    line: &str,
) {
    let parsed: Value = match js_json::parse_lossless(line) {
        Ok(parsed) => parsed,
        Err(error) => {
            let mut state = state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            reject_generation_locked(&mut state, generation, PyrunRunnerError::new(error.into_message()));
            return;
        }
    };
    let message_type = parsed
        .get("type")
        .and_then(Value::as_string)
        .cloned()
        .unwrap_or_default();
    enum LineAction {
        Pi(Option<PyrunPiRequestHandler>, JsString, Value),
        Progress(Option<PyrunProgressCallback>),
    }
    let action = {
        let mut state = state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(index) = state
            .pending
            .iter()
            .position(|request| request.generation == generation)
        else {
            return;
        };
        if is_final_result(&parsed) {
            let Some(mut request) = state.pending.remove(index) else {
                return;
            };
            if let Some(task) = request.abort_task.take() {
                task.abort();
            }
            match js_json::from_value::<CanonicalPyrunEvalResult>(&parsed) {
                Ok(result) => {
                    let _ = request.resolve.send(Ok(result));
                }
                Err(error) => {
                    let _ = request.resolve.send(Err(PyrunRunnerError::new(error.to_string())));
                }
            }
            return;
        }
        if message_type == "pi_request" {
            let handler = state.pending[index].on_pi_request.clone();
            let method = parsed
                .get("method")
                .and_then(Value::as_string)
                .cloned()
                .unwrap_or_default();
            let params = parsed.get("params").cloned().unwrap_or(Value::Null);
            LineAction::Pi(handler, method, params)
        } else {
            LineAction::Progress(state.pending[index].on_progress.clone())
        }
    };
    match action {
        LineAction::Pi(handler, method, params) => {
            let response = match handler {
                Some(handler) => handler(PiRequest { method, params }).await,
                None => Ok(None),
            };
            let payload = match response {
                Ok(Some(result)) => {
                    let mut object = JsObject::new();
                    object.insert("result".to_owned().into(), result);
                    Value::Object(object)
                }
                Ok(None) => Value::Object(JsObject::new()),
                Err(error) => {
                    let mut object = JsObject::new();
                    object.insert("error".into(), Value::String(error.message));
                    Value::Object(object)
                }
            };
            let mut stdin = stdin.lock().await;
            let payload = js_json::stringify_lossless(&payload);
            let _ = stdin.write_all(format!("{payload}\n").as_bytes()).await;
            let _ = stdin.flush().await;
        }
        LineAction::Progress(callback) => {
            if let Some(callback) = callback
                && let Err(error) = callback(CanonicalPyrunProgressUpdate { raw: parsed })
            {
                let mut state = state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                reject_generation_locked(&mut state, generation, error);
            }
        }
    }
}

/// Small delay used by tests and callers that need to observe background writes.
pub async fn flush_delay() {
    tokio::time::sleep(Duration::from_millis(1)).await;
}
