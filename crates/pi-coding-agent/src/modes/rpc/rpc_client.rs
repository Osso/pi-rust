//! RPC client (`modes/rpc/rpc-client.ts`).
//!
//! Spawns a `pi --mode rpc` child, writes command lines to its stdin, and
//! correlates response lines with pending requests. Events that are not
//! responses are delivered to subscribers in wire order.

use indexmap::IndexMap;
use std::collections::HashMap;
use std::fmt;
use std::io::Write as _;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pi_agent_core::harness::ThinkingLevel;
use pi_ai::types::ImageContent;

#[path = "rpc_client_models.rs"]
mod model_records;
pub use model_records::{ModelInfo, ModelReference};
use pi_js_compat::js_json::{self, JsString, JsValue, encode_node_utf8};
use rustix::process::Signal;
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::{ChildStdin, Command};
use tokio::sync::{mpsc, oneshot, watch};

use crate::core::agent_session::SessionStats;
use crate::core::bash_executor::BashResult;
use crate::core::compaction::CompactionResult;
use crate::core::session_manager::{SessionEntry, SessionTreeNode};
use crate::modes::rpc::jsonl::{JsonlLineReader, attach_jsonl_line_reader, serialize_json_line};
use crate::modes::rpc::rpc_types::{QueueMode, RpcCommand, RpcCommandBody, RpcSessionState, RpcSlashCommand};

/// Default wait used by `waitForIdle` and `collectEvents` in TypeScript.
pub const DEFAULT_EVENT_TIMEOUT: f64 = 60_000.0;
const STARTUP_DELAY: Duration = Duration::from_millis(100);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const TERMINATE_TIMEOUT: Duration = Duration::from_secs(1);
const KILL_TIMEOUT: Duration = Duration::from_secs(1);
const STDERR_CHUNK_BYTES: usize = 8192;

/// `RpcEventListener`: receives each non-correlated event line as JSON.
///
/// `on_event` accepts any compatible closure; this alias names the shared form.
pub type RpcEventListener = Arc<dyn Fn(&JsValue) -> Result<(), pi_ai::request::CallbackError> + Send + Sync>;

/// Options for [`RpcClient::start`].
#[derive(Debug, Clone, Default)]
pub struct RpcClientOptions {
    /// Runtime executable, defaulting to node (frozen executablePath).
    pub executable_path: Option<JsString>,
    /// Path to the CLI entry point (default: `dist/cli.js`).
    pub cli_path: Option<JsString>,
    /// Working directory for the agent.
    pub cwd: Option<JsString>,
    /// Environment overrides merged over the inherited environment.
    pub env: IndexMap<JsString, JsString>,
    /// Provider passed as `--provider`.
    pub provider: Option<JsString>,
    /// Model passed as `--model`.
    pub model: Option<JsString>,
    /// Additional arguments passed to Node before the CLI entry point.
    pub node_args: Vec<JsString>,
    /// Additional CLI arguments.
    pub args: Vec<JsString>,
}

/// `{ cancelled }` returned by `new_session`, `switch_session`, and `clone`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelledResult {
    pub cancelled: bool,
}

/// `cycle_model` data.
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CycleModelResult {
    pub model: ModelReference,
    pub thinking_level: ThinkingLevel,
    pub is_scoped: bool,
}

/// `cycle_thinking_level` data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct CycleThinkingLevelResult {
    pub level: ThinkingLevel,
}

/// `export_html` data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct ExportHtmlResult {
    pub path: JsString,
}

/// `fork` data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct ForkResult {
    pub text: JsString,
    pub cancelled: bool,
}

/// One `get_fork_messages` entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForkMessage {
    pub entry_id: JsString,
    pub text: JsString,
}

/// `get_entries` data.
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntriesResult {
    pub entries: Vec<SessionEntry>,
    pub leaf_id: Option<JsString>,
}

/// `get_tree` data.
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeResult {
    pub tree: Vec<SessionTreeNode>,
    pub leaf_id: Option<JsString>,
}

/// An RPC client error. `Display` matches the TypeScript `Error.message`.
#[derive(Debug, Clone, PartialEq)]
pub enum RpcClientError {
    Native(Box<pi_js_compat::node_error::NativeNodeError>),
    AlreadyStartedOrStopping,
    StoppedDuringStartup {
        stderr: String,
    },
    ProcessExited {
        code: Option<i32>,
        signal: Option<i32>,
        stderr: String,
    },
    ProcessError {
        message: JsString,
        stderr: String,
    },
    StdinError {
        message: String,
        stderr: String,
    },
    StdinNotWritable {
        stderr: String,
    },
    NotStarted,
    Stopping {
        stderr: String,
    },
    PreviousStopFailed {
        message: JsString,
    },
    Stopped {
        stderr: String,
    },
    RequestTimeout {
        command: String,
        stderr: String,
    },
    StopFailed {
        stderr: String,
    },
    IdleTimeout {
        stderr: String,
    },
    CollectTimeout {
        stderr: String,
    },
    CommandFailed {
        message: JsString,
    },
    MissingData {
        command: String,
    },
    InvalidData {
        command: String,
        message: String,
    },
    TypeError {
        message: JsString,
    },
    Serialize {
        message: String,
    },
}

impl RpcClientError {
    pub fn name(&self) -> JsString {
        match self {
            Self::Native(error) => error.name.clone(),
            Self::TypeError { .. } => "TypeError".into(),
            _ => "Error".into(),
        }
    }

    pub fn message(&self) -> JsString {
        match self {
            Self::Native(error) => error.message.clone(),
            Self::CommandFailed { message } | Self::TypeError { message } => message.clone(),
            Self::AlreadyStartedOrStopping => "Client already started or stopping".into(),
            Self::NotStarted => "Client not started".into(),
            Self::StoppedDuringStartup { stderr } => {
                format!("RPC client stopped during startup. Stderr: {stderr}").into()
            }
            Self::ProcessExited { code, signal, stderr } => format!(
                "Agent process exited (code={} signal={}). Stderr: {stderr}",
                optional_code(*code),
                optional_signal(*signal)
            )
            .into(),
            Self::ProcessError { message, stderr } => {
                pi_js_compat::js_format!("Agent process error: {}. Stderr: {}", message, stderr)
            }
            Self::StdinError { message, stderr } => {
                format!("Agent process stdin error: {message}. Stderr: {stderr}").into()
            }
            Self::StdinNotWritable { stderr } => {
                format!("Agent process stdin is not writable. Stderr: {stderr}").into()
            }
            Self::Stopping { stderr } => format!("RPC client is stopping. Stderr: {stderr}").into(),
            Self::PreviousStopFailed { message } => pi_js_compat::js_format!("Previous RPC stop failed: {}", message),
            Self::Stopped { stderr } => format!("RPC client stopped. Stderr: {stderr}").into(),
            Self::RequestTimeout { command, stderr } => {
                format!("Timeout waiting for response to {command}. Stderr: {stderr}").into()
            }
            Self::StopFailed { stderr } => format!("RPC child did not exit after SIGKILL. Stderr: {stderr}").into(),
            Self::IdleTimeout { stderr } => {
                format!("Timeout waiting for agent to become idle. Stderr: {stderr}").into()
            }
            Self::CollectTimeout { stderr } => format!("Timeout collecting events. Stderr: {stderr}").into(),
            Self::MissingData { command } => format!("Missing response data for {command}").into(),
            Self::InvalidData { command, message } => format!("Invalid response data for {command}: {message}").into(),
            Self::Serialize { message } => format!("Failed to serialize command: {message}").into(),
        }
    }

    pub fn value(&self) -> JsValue {
        match self {
            Self::Native(error) => error.value(),
            _ => pi_js_compat::js_json!({"name":self.name(),"message":self.message()}),
        }
    }
}

impl fmt::Display for RpcClientError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&encode_node_utf8(&self.message()))
    }
}

impl std::error::Error for RpcClientError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Native(error) => Some(error.as_ref()),
            _ => None,
        }
    }
}

fn optional_code(code: Option<i32>) -> String {
    code.map_or_else(|| "null".to_owned(), |code| code.to_string())
}

fn optional_signal(signal: Option<i32>) -> String {
    signal.map_or_else(|| "null".to_owned(), crate::utils::child_process::signal_name)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ExitInfo {
    code: Option<i32>,
    signal: Option<i32>,
}

fn exit_info(status: &std::process::ExitStatus) -> ExitInfo {
    #[cfg(unix)]
    let signal = {
        use std::os::unix::process::ExitStatusExt;
        status.signal()
    };
    #[cfg(not(unix))]
    let signal = None;

    ExitInfo {
        code: status.code(),
        signal,
    }
}

type KillFn = Arc<dyn Fn(u32, Signal) -> std::io::Result<()> + Send + Sync>;

fn kill_process(pid: u32, signal: Signal) -> std::io::Result<()> {
    let Some(pid) = rustix::process::Pid::from_raw(pid as i32) else {
        return Ok(());
    };
    rustix::process::kill_process(pid, signal).map_err(std::io::Error::from)
}

#[derive(Clone)]
struct Listener {
    id: u64,
    callback: RpcEventListener,
}

struct SyncState {
    stderr: String,
    exit: Option<ExitInfo>,
    exit_error: Option<RpcClientError>,
    process_error_listener: bool,
    active_generation: Option<u64>,
    stop_run: Option<Arc<StopCompletion>>,
    stop_error: Option<RpcClientError>,
    next_request_id: u64,
    pending: HashMap<String, oneshot::Sender<Result<JsValue, RpcClientError>>>,
    stdin: Option<mpsc::UnboundedSender<String>>,
    listeners: Vec<Listener>,
    next_listener_id: u64,
}

#[derive(Default)]
struct StopCompletion {
    result: Mutex<Option<Result<(), RpcClientError>>>,
    ready: tokio::sync::Notify,
}

impl StopCompletion {
    async fn wait(&self) -> Result<(), RpcClientError> {
        loop {
            let ready = self.ready.notified();
            if let Some(result) = self.result.lock().expect("stop result poisoned").clone() {
                return result;
            }
            ready.await;
        }
    }
}

struct ProcessHandle {
    pid: u32,
    generation: u64,
    exit: watch::Receiver<Option<ExitInfo>>,
}

struct PendingRequest {
    id: String,
    receiver: oneshot::Receiver<Result<JsValue, RpcClientError>>,
}

#[derive(Default)]
struct ProcessState {
    child: Option<ProcessHandle>,
    reader: Option<JsonlLineReader>,
}

struct ClientInner {
    options: RpcClientOptions,
    sync: Mutex<SyncState>,
    process: tokio::sync::Mutex<ProcessState>,
    kill: KillFn,
    next_generation: AtomicU64,
}

impl ClientInner {
    fn handle_line(&self, line: &str) {
        let Ok(value) = js_json::parse_lossless(line) else {
            return;
        };

        if let Some(id) = response_id(&value) {
            let sender = self.sync.lock().expect("sync state poisoned").pending.remove(&id);
            if let Some(sender) = sender {
                let _ = sender.send(Ok(value));
                return;
            }
        }

        // Frozen's encompassing catch also ignores null.type and callback rejections.
        if value.is_null() {
            return;
        }
        let mut index = 0;
        loop {
            let listener = self
                .sync
                .lock()
                .expect("sync state poisoned")
                .listeners
                .get(index)
                .cloned();
            let Some(listener) = listener else {
                return;
            };
            if (listener.callback)(&value).is_err() {
                return;
            }
            index += 1;
        }
    }

    fn stderr(&self) -> String {
        self.sync.lock().expect("sync state poisoned").stderr.clone()
    }
}

fn response_id(value: &JsValue) -> Option<String> {
    if value.get("type").and_then(JsValue::as_string) != Some(&"response".into()) {
        return None;
    }
    value
        .get("id")
        .and_then(JsValue::as_string)
        .and_then(|id| id.to_utf8().ok())
        .filter(|id| !id.is_empty())
}

fn reject_pending(state: &mut SyncState, error: RpcClientError) {
    for (_, sender) in state.pending.drain() {
        let _ = sender.send(Err(error.clone()));
    }
}

/// RPC client driving one `pi --mode rpc` child process.
pub struct RpcClient {
    inner: Arc<ClientInner>,
}

impl Default for RpcClient {
    fn default() -> Self {
        Self::new(RpcClientOptions::default())
    }
}

impl RpcClient {
    pub fn new(options: RpcClientOptions) -> Self {
        Self::with_kill(options, Arc::new(kill_process))
    }

    fn with_kill(options: RpcClientOptions, kill: KillFn) -> Self {
        Self {
            inner: Arc::new(ClientInner {
                options,
                sync: Mutex::new(SyncState {
                    stderr: String::new(),
                    exit: None,
                    exit_error: None,
                    process_error_listener: false,
                    active_generation: None,
                    stop_run: None,
                    stop_error: None,
                    next_request_id: 0,
                    pending: HashMap::new(),
                    stdin: None,
                    listeners: Vec::new(),
                    next_listener_id: 0,
                }),
                process: tokio::sync::Mutex::new(ProcessState::default()),
                kill,
                next_generation: AtomicU64::new(0),
            }),
        }
    }

    /// Starts the RPC agent process.
    pub async fn start(&self) -> Result<(), RpcClientError> {
        let mut process = self.inner.process.lock().await;
        let generation = self.begin_start(&process)?;
        let spawn_error = match spawn_process(&self.inner.options, self.inner.stderr()) {
            Ok(spawned) => {
                install_process(&self.inner, generation, spawned, &mut process);
                None
            }
            Err(error @ RpcClientError::Native(_)) => return Err(error),
            Err(error) => {
                self.publish_spawn_failure(generation, &error);
                Some(error)
            }
        };
        drop(process);
        tokio::time::sleep(STARTUP_DELAY).await;
        self.verify_started(generation)?;
        spawn_error.map_or(Ok(()), Err)
    }

    fn publish_spawn_failure(&self, generation: u64, error: &RpcClientError) {
        let mut sync = self.inner.sync.lock().expect("sync state poisoned");
        sync.active_generation = Some(generation);
        sync.process_error_listener = false;
        sync.exit_error = Some(error.clone());
    }

    fn begin_start(&self, process: &ProcessState) -> Result<u64, RpcClientError> {
        let mut sync = self.inner.sync.lock().expect("sync state poisoned");
        if sync.active_generation.is_some() || process.child.is_some() || sync.stop_run.is_some() {
            return Err(RpcClientError::AlreadyStartedOrStopping);
        }
        sync.exit = None;
        sync.exit_error = None;
        Ok(self.inner.next_generation.fetch_add(1, Ordering::SeqCst) + 1)
    }

    fn verify_started(&self, generation: u64) -> Result<(), RpcClientError> {
        let sync = self.inner.sync.lock().expect("sync state poisoned");
        if sync.stop_run.is_some() || sync.active_generation != Some(generation) {
            return Err(RpcClientError::StoppedDuringStartup {
                stderr: sync.stderr.clone(),
            });
        }
        match sync.exit {
            Some(exit) => Err(sync
                .exit_error
                .clone()
                .unwrap_or_else(|| process_exit_error(exit, &sync.stderr))),
            None => Ok(()),
        }
    }

    /// Stops the RPC agent process.
    pub async fn stop(&self) -> Result<(), RpcClientError> {
        let (completion, owner) = {
            let mut sync = self.inner.sync.lock().expect("sync state poisoned");
            if let Some(completion) = &sync.stop_run {
                (Arc::clone(completion), false)
            } else {
                if sync.active_generation.is_none() {
                    return Ok(());
                }
                let completion = Arc::new(StopCompletion::default());
                sync.stop_run = Some(Arc::clone(&completion));
                (completion, true)
            }
        };
        if owner {
            let client = Self {
                inner: Arc::clone(&self.inner),
            };
            let run = Arc::clone(&completion);
            tokio::spawn(async move {
                let result = client.stop_process().await;
                client.publish_stop_result(&run, result);
            });
        }
        completion.wait().await
    }

    fn publish_stop_result(&self, completion: &Arc<StopCompletion>, result: Result<(), RpcClientError>) {
        {
            let mut sync = self.inner.sync.lock().expect("sync state poisoned");
            sync.stop_error = result.as_ref().err().cloned();
            *completion.result.lock().expect("stop result poisoned") = Some(result.clone());
            sync.stop_run = None;
        }
        completion.ready.notify_waiters();
    }

    async fn stop_process(&self) -> Result<(), RpcClientError> {
        let mut process = self.inner.process.lock().await;
        if let Some(reader) = process.reader.take() {
            reader.detach();
        }
        {
            let mut sync = self.inner.sync.lock().expect("sync state poisoned");
            let error = RpcClientError::Stopped {
                stderr: sync.stderr.clone(),
            };
            reject_pending(&mut sync, error);
        }

        let result = self.terminate(&mut process).await;
        if result.is_ok() {
            process.child = None;
            let mut sync = self.inner.sync.lock().expect("sync state poisoned");
            sync.stdin = None;
            sync.active_generation = None;
        }
        result
    }

    async fn terminate(&self, process: &mut ProcessState) -> Result<(), RpcClientError> {
        let Some(handle) = process.child.as_mut() else {
            return Ok(());
        };
        if handle.exit.borrow().is_some() {
            return Ok(());
        }

        self.signal_child(handle, Signal::TERM)
            .map_err(RpcClientError::Native)?;
        if wait_for_exit(&mut handle.exit, TERMINATE_TIMEOUT).await {
            return Ok(());
        }
        self.signal_child(handle, Signal::KILL)
            .map_err(RpcClientError::Native)?;
        if wait_for_exit(&mut handle.exit, KILL_TIMEOUT).await {
            return Ok(());
        }
        Err(RpcClientError::StopFailed {
            stderr: self.inner.stderr(),
        })
    }

    fn signal_child(
        &self,
        handle: &ProcessHandle,
        signal: Signal,
    ) -> Result<(), Box<pi_js_compat::node_error::NativeNodeError>> {
        let Err(error) = (self.inner.kill)(handle.pid, signal) else {
            return Ok(());
        };
        if error.raw_os_error() == Some(rustix::io::Errno::SRCH.raw_os_error()) {
            return Ok(());
        }
        let synchronous = error.raw_os_error().is_some_and(|code| {
            [
                rustix::io::Errno::INVAL.raw_os_error(),
                rustix::io::Errno::NOSYS.raw_os_error(),
            ]
            .contains(&code)
        });
        let native = pi_js_compat::node_fs::node_stream_error_record(&error, "kill").into();
        if synchronous {
            return Err(Box::new(native));
        }
        publish_process_error(&self.inner, handle.generation, native);
        Ok(())
    }

    /// Subscribes to agent events. Dropping the subscription unsubscribes.
    pub fn on_event<F>(&self, listener: F) -> RpcEventSubscription
    where
        F: Fn(&JsValue) -> Result<(), pi_ai::request::CallbackError> + Send + Sync + 'static,
    {
        self.register_listener(|_| Arc::new(listener))
    }

    fn register_listener(&self, callback: impl FnOnce(u64) -> RpcEventListener) -> RpcEventSubscription {
        let mut sync = self.inner.sync.lock().expect("sync state poisoned");
        sync.next_listener_id += 1;
        let id = sync.next_listener_id;
        sync.listeners.push(Listener {
            id,
            callback: callback(id),
        });
        RpcEventSubscription {
            inner: Arc::clone(&self.inner),
            id,
        }
    }

    /// Collected stderr output.
    pub fn stderr(&self) -> String {
        self.inner.stderr()
    }

    /// Sends a command and waits for its response.
    pub async fn send(&self, body: RpcCommandBody) -> Result<JsValue, RpcClientError> {
        let command_type = body.command_type();
        let request = self.begin_request(body)?;
        self.await_request_response(request, command_type).await
    }

    fn begin_request(&self, body: RpcCommandBody) -> Result<PendingRequest, RpcClientError> {
        let mut sync = self.inner.sync.lock().expect("sync state poisoned");
        if sync.stop_run.is_some() {
            return Err(RpcClientError::Stopping {
                stderr: sync.stderr.clone(),
            });
        }
        if let Some(error) = &sync.stop_error {
            return Err(RpcClientError::PreviousStopFailed {
                message: error.message(),
            });
        }
        if sync.active_generation.is_none() {
            return Err(RpcClientError::NotStarted);
        }
        if let Some(error) = &sync.exit_error {
            return Err(error.clone());
        }
        if let Some(exit) = sync.exit {
            let error = process_exit_error(exit, &sync.stderr);
            sync.exit_error = Some(error.clone());
            return Err(error);
        }

        let stdin = match sync.stdin.as_ref().filter(|stdin| !stdin.is_closed()).cloned() {
            Some(stdin) => stdin,
            None => {
                let error = RpcClientError::StdinNotWritable {
                    stderr: sync.stderr.clone(),
                };
                sync.exit_error = Some(error.clone());
                return Err(error);
            }
        };
        sync.next_request_id += 1;
        let id = format!("req_{}", sync.next_request_id);
        let command = RpcCommand::new(body, Some(id.as_str().into()));
        let line = serialize_json_line(&command).map_err(|error| RpcClientError::Serialize {
            message: error.to_string(),
        })?;
        let (sender, receiver) = oneshot::channel();
        sync.pending.insert(id.clone(), sender);
        if stdin.send(line).is_err() {
            let error = RpcClientError::StdinNotWritable {
                stderr: sync.stderr.clone(),
            };
            sync.exit_error = Some(error.clone());
            reject_pending(&mut sync, error.clone());
            return Err(error);
        }
        Ok(PendingRequest { id, receiver })
    }

    async fn await_request_response(
        &self,
        request: PendingRequest,
        command_type: &str,
    ) -> Result<JsValue, RpcClientError> {
        match tokio::time::timeout(REQUEST_TIMEOUT, request.receiver).await {
            Ok(Ok(response)) => response,
            Ok(Err(_)) => Err(RpcClientError::Stopped {
                stderr: self.inner.stderr(),
            }),
            Err(_) => {
                self.inner
                    .sync
                    .lock()
                    .expect("sync state poisoned")
                    .pending
                    .remove(&request.id);
                Err(RpcClientError::RequestTimeout {
                    command: command_type.to_owned(),
                    stderr: self.inner.stderr(),
                })
            }
        }
    }

    // =========================================================================
    // Command methods
    // =========================================================================

    pub async fn prompt(
        &self,
        message: impl Into<JsString>,
        images: Option<Vec<ImageContent>>,
    ) -> Result<(), RpcClientError> {
        self.send(RpcCommandBody::Prompt {
            message: message.into(),
            images,
            streaming_behavior: None,
        })
        .await?;
        Ok(())
    }

    pub async fn steer(
        &self,
        message: impl Into<JsString>,
        images: Option<Vec<ImageContent>>,
    ) -> Result<(), RpcClientError> {
        self.send(RpcCommandBody::Steer {
            message: message.into(),
            images,
        })
        .await?;
        Ok(())
    }

    pub async fn follow_up(
        &self,
        message: impl Into<JsString>,
        images: Option<Vec<ImageContent>>,
    ) -> Result<(), RpcClientError> {
        self.send(RpcCommandBody::FollowUp {
            message: message.into(),
            images,
        })
        .await?;
        Ok(())
    }

    pub async fn abort(&self) -> Result<(), RpcClientError> {
        self.send(RpcCommandBody::Abort).await?;
        Ok(())
    }

    pub async fn interrupt(&self) -> Result<(), RpcClientError> {
        self.send(RpcCommandBody::Interrupt).await?;
        Ok(())
    }

    pub async fn new_session(&self, parent_session: Option<JsString>) -> Result<CancelledResult, RpcClientError> {
        let response = self.send(RpcCommandBody::NewSession { parent_session }).await?;
        response_data(response, "new_session")
    }

    pub async fn get_state(&self) -> Result<RpcSessionState, RpcClientError> {
        let response = self.send(RpcCommandBody::GetState).await?;
        response_data(response, "get_state")
    }

    pub async fn set_model(
        &self,
        provider: impl Into<JsString>,
        model_id: impl Into<JsString>,
    ) -> Result<ModelReference, RpcClientError> {
        let response = self
            .send(RpcCommandBody::SetModel {
                provider: provider.into(),
                model_id: model_id.into(),
            })
            .await?;
        response_data(response, "set_model")
    }

    pub async fn cycle_model(&self) -> Result<Option<CycleModelResult>, RpcClientError> {
        let response = self.send(RpcCommandBody::CycleModel).await?;
        response_data(response, "cycle_model")
    }

    pub async fn get_available_models(&self) -> Result<Vec<ModelInfo>, RpcClientError> {
        let response = self.send(RpcCommandBody::GetAvailableModels).await?;
        response_field_data(response, "get_available_models", "models")
    }

    pub async fn set_thinking_level(&self, level: ThinkingLevel) -> Result<(), RpcClientError> {
        self.send(RpcCommandBody::SetThinkingLevel { level }).await?;
        Ok(())
    }

    pub async fn cycle_thinking_level(&self) -> Result<Option<CycleThinkingLevelResult>, RpcClientError> {
        let response = self.send(RpcCommandBody::CycleThinkingLevel).await?;
        response_data(response, "cycle_thinking_level")
    }

    pub async fn set_steering_mode(&self, mode: QueueMode) -> Result<(), RpcClientError> {
        self.send(RpcCommandBody::SetSteeringMode { mode }).await?;
        Ok(())
    }

    pub async fn set_follow_up_mode(&self, mode: QueueMode) -> Result<(), RpcClientError> {
        self.send(RpcCommandBody::SetFollowUpMode { mode }).await?;
        Ok(())
    }

    pub async fn set_auto_compaction(&self, enabled: bool) -> Result<(), RpcClientError> {
        self.send(RpcCommandBody::SetAutoCompaction { enabled }).await?;
        Ok(())
    }

    pub async fn compact(&self, custom_instructions: Option<JsString>) -> Result<CompactionResult, RpcClientError> {
        let response = self.send(RpcCommandBody::Compact { custom_instructions }).await?;
        response_data(response, "compact")
    }

    pub async fn set_auto_retry(&self, enabled: bool) -> Result<(), RpcClientError> {
        self.send(RpcCommandBody::SetAutoRetry { enabled }).await?;
        Ok(())
    }

    pub async fn abort_retry(&self) -> Result<(), RpcClientError> {
        self.send(RpcCommandBody::AbortRetry).await?;
        Ok(())
    }

    pub async fn bash(&self, command: impl Into<JsString>) -> Result<BashResult, RpcClientError> {
        let response = self
            .send(RpcCommandBody::Bash {
                command: command.into(),
                exclude_from_context: None,
            })
            .await?;
        response_data(response, "bash")
    }

    pub async fn abort_bash(&self) -> Result<(), RpcClientError> {
        self.send(RpcCommandBody::AbortBash).await?;
        Ok(())
    }

    pub async fn get_session_stats(&self) -> Result<SessionStats, RpcClientError> {
        let response = self.send(RpcCommandBody::GetSessionStats).await?;
        response_data(response, "get_session_stats")
    }

    pub async fn export_html(&self, output_path: Option<JsString>) -> Result<ExportHtmlResult, RpcClientError> {
        let response = self.send(RpcCommandBody::ExportHtml { output_path }).await?;
        response_data(response, "export_html")
    }

    pub async fn switch_session(&self, session_path: impl Into<JsString>) -> Result<CancelledResult, RpcClientError> {
        let response = self
            .send(RpcCommandBody::SwitchSession {
                session_path: session_path.into(),
            })
            .await?;
        response_data(response, "switch_session")
    }

    pub async fn fork(&self, entry_id: impl Into<JsString>) -> Result<ForkResult, RpcClientError> {
        let response = self
            .send(RpcCommandBody::Fork {
                entry_id: entry_id.into(),
            })
            .await?;
        response_data(response, "fork")
    }

    pub async fn clone_session(&self) -> Result<CancelledResult, RpcClientError> {
        let response = self.send(RpcCommandBody::Clone).await?;
        response_data(response, "clone")
    }

    pub async fn get_fork_messages(&self) -> Result<Vec<ForkMessage>, RpcClientError> {
        let response = self.send(RpcCommandBody::GetForkMessages).await?;
        response_field_data(response, "get_fork_messages", "messages")
    }

    pub async fn get_entries(&self, since: Option<JsString>) -> Result<EntriesResult, RpcClientError> {
        let response = self.send(RpcCommandBody::GetEntries { since }).await?;
        response_data(response, "get_entries")
    }

    pub async fn get_tree(&self) -> Result<TreeResult, RpcClientError> {
        let response = self.send(RpcCommandBody::GetTree).await?;
        response_data(response, "get_tree")
    }

    pub async fn get_last_assistant_text(&self) -> Result<Option<JsString>, RpcClientError> {
        let response = self.send(RpcCommandBody::GetLastAssistantText).await?;
        response_field_data(response, "get_last_assistant_text", "text")
    }

    pub async fn set_session_name(&self, name: impl Into<JsString>) -> Result<(), RpcClientError> {
        self.send(RpcCommandBody::SetSessionName { name: name.into() }).await?;
        Ok(())
    }

    pub async fn get_messages(&self) -> Result<Vec<pi_agent_core::agent::AgentMessage>, RpcClientError> {
        let response = self.send(RpcCommandBody::GetMessages).await?;
        response_field_data(response, "get_messages", "messages")
    }

    pub async fn get_commands(&self) -> Result<Vec<RpcSlashCommand>, RpcClientError> {
        let response = self.send(RpcCommandBody::GetCommands).await?;
        response_field_data(response, "get_commands", "commands")
    }

    // =========================================================================
    // Helpers
    // =========================================================================

    /// Waits for the first agent_end event, including a retrying end.
    pub async fn wait_for_idle(&self, timeout_ms: f64) -> Result<(), RpcClientError> {
        let (sender, mut receiver) = mpsc::unbounded_channel();
        let _subscription = self.subscribe_sender(sender);
        let outcome = tokio::time::timeout(event_timeout(timeout_ms), async {
            while let Some(event) = receiver.recv().await {
                if is_final_agent_end(&event) {
                    return;
                }
            }
        })
        .await;
        outcome.map_err(|_| RpcClientError::IdleTimeout {
            stderr: self.inner.stderr(),
        })
    }

    /// Collects events until the agent becomes idle.
    pub async fn collect_events(&self, timeout_ms: f64) -> Result<Vec<JsValue>, RpcClientError> {
        self.start_collector(timeout_ms, true).finish().await
    }

    /// Sends a prompt and waits for completion, returning all events.
    pub async fn prompt_and_wait(
        &self,
        message: impl Into<JsString>,
        images: Option<Vec<ImageContent>>,
        timeout_ms: f64,
    ) -> Result<Vec<JsValue>, RpcClientError> {
        let collector = self.start_collector(timeout_ms, false);
        if let Err(error) = self.prompt(message, images).await {
            collector.retain_unobserved();
            return Err(error);
        }
        collector.finish().await
    }

    fn start_collector(&self, timeout_ms: f64, observed: bool) -> EventCollector {
        let (sender, receiver) = mpsc::unbounded_channel();
        let subscription = self.subscribe_sender(sender);
        let deadline = tokio::time::Instant::now() + event_timeout(timeout_ms);
        let inner = Arc::clone(&self.inner);
        let (result_sender, result) = oneshot::channel();
        let observed = Arc::new(AtomicBool::new(observed));
        let task_observed = Arc::clone(&observed);
        let task = tokio::spawn(async move {
            let result = collect_event_stream(receiver, deadline, &inner).await;
            drop(subscription);
            let unobserved = result
                .as_ref()
                .err()
                .filter(|_| !task_observed.load(Ordering::SeqCst))
                .map(RpcClientError::message);
            let _ = result_sender.send(result);
            if let Some(message) = unobserved {
                crate::utils::uncaught::throw_uncaught(crate::utils::uncaught::ERROR, message);
            }
        });
        EventCollector {
            receiver: result,
            observed,
            task: Some(task),
        }
    }

    fn subscribe_sender(&self, sender: mpsc::UnboundedSender<JsValue>) -> RpcEventSubscription {
        let inner = Arc::downgrade(&self.inner);
        self.register_listener(move |id| {
            Arc::new(move |event| {
                let _ = sender.send(event.clone());
                if is_final_agent_end(event)
                    && let Some(inner) = inner.upgrade()
                {
                    inner
                        .sync
                        .lock()
                        .expect("sync state poisoned")
                        .listeners
                        .retain(|listener| listener.id != id);
                }
                Ok(())
            })
        })
    }
}

fn event_timeout(timeout_ms: f64) -> Duration {
    Duration::from_secs_f64(pi_js_compat::node_timers::normalize_timeout_delay(timeout_ms) / 1000.0)
}

struct EventCollector {
    receiver: oneshot::Receiver<Result<Vec<JsValue>, RpcClientError>>,
    observed: Arc<AtomicBool>,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl EventCollector {
    async fn finish(mut self) -> Result<Vec<JsValue>, RpcClientError> {
        self.observed.store(true, Ordering::SeqCst);
        (&mut self.receiver)
            .await
            .expect("collector publishes before an uncaught rejection")
    }

    // Frozen promptAndWait leaves its local promise running when prompt rejects.
    fn retain_unobserved(mut self) {
        drop(self.task.take());
    }
}

impl Drop for EventCollector {
    fn drop(&mut self) {
        // Explicit caller cancellation keeps the approved native SDK boundary.
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

async fn collect_event_stream(
    mut receiver: mpsc::UnboundedReceiver<JsValue>,
    deadline: tokio::time::Instant,
    inner: &ClientInner,
) -> Result<Vec<JsValue>, RpcClientError> {
    let mut events = Vec::new();
    tokio::time::timeout_at(deadline, async {
        while let Some(event) = receiver.recv().await {
            let done = is_final_agent_end(&event);
            events.push(event);
            if done {
                return;
            }
        }
    })
    .await
    .map_err(|_| RpcClientError::CollectTimeout { stderr: inner.stderr() })?;
    Ok(events)
}

/// Handle for one [`RpcClient::on_event`] subscription.
pub struct RpcEventSubscription {
    inner: Arc<ClientInner>,
    id: u64,
}

impl RpcEventSubscription {
    /// Removes the listener before the handle is dropped.
    pub fn unsubscribe(self) {}
}

impl Drop for RpcEventSubscription {
    fn drop(&mut self) {
        if let Ok(mut sync) = self.inner.sync.lock() {
            sync.listeners.retain(|listener| listener.id != self.id);
        }
    }
}

fn is_final_agent_end(event: &JsValue) -> bool {
    event.get("type").and_then(JsValue::as_string) == Some(&"agent_end".into())
}

fn get_data(response: &JsValue) -> Result<Option<&JsValue>, RpcClientError> {
    if response.get("success").is_some_and(js_json::is_truthy) {
        return Ok(response.get("data"));
    }
    let message = match response.get("error") {
        None => JsString::default(),
        Some(value) => js_json::to_js_string(value).map_err(|message| RpcClientError::TypeError {
            message: message.into(),
        })?,
    };
    Err(RpcClientError::CommandFailed { message })
}

fn response_data<T: DeserializeOwned + Serialize>(response: JsValue, command: &str) -> Result<T, RpcClientError> {
    decode_data(get_data(&response)?, command)
}

fn response_field_data<T: DeserializeOwned + Serialize>(
    response: JsValue,
    command: &str,
    field: &str,
) -> Result<T, RpcClientError> {
    let data = get_data(&response)?;
    let absent = match data {
        None => Some("undefined"),
        Some(JsValue::Null) => Some("null"),
        _ => None,
    };
    if let Some(absent) = absent {
        return Err(RpcClientError::TypeError {
            message: format!("Cannot read properties of {absent} (reading '{field}')").into(),
        });
    }
    decode_data(data.and_then(|data| data.get(field)), command)
}

fn decode_data<T: DeserializeOwned + Serialize>(data: Option<&JsValue>, command: &str) -> Result<T, RpcClientError> {
    let data = data.ok_or_else(|| RpcClientError::MissingData {
        command: command.to_owned(),
    })?;
    let value: T = js_json::from_value(data).map_err(|error| RpcClientError::InvalidData {
        command: command.to_owned(),
        message: error.to_string(),
    })?;
    let encoded = js_json::to_value(&value).map_err(|error| RpcClientError::InvalidData {
        command: command.to_owned(),
        message: error.to_string(),
    })?;
    if &encoded != data {
        return Err(RpcClientError::InvalidData {
            command: command.to_owned(),
            message: "declared return type cannot preserve this response value".to_owned(),
        });
    }
    Ok(value)
}

fn process_exit_error(exit: ExitInfo, stderr: &str) -> RpcClientError {
    RpcClientError::ProcessExited {
        code: exit.code,
        signal: exit.signal,
        stderr: stderr.to_owned(),
    }
}

struct SpawnedProcess {
    child: tokio::process::Child,
    stdin: ChildStdin,
    stdout: tokio::process::ChildStdout,
    stderr: tokio::process::ChildStderr,
    pid: u32,
}

fn spawn_arguments(options: &RpcClientOptions) -> Vec<JsString> {
    let mut args = options.node_args.clone();
    args.push(options.cli_path.clone().unwrap_or_else(|| "dist/cli.js".into()));
    args.extend(["--mode".into(), "rpc".into()]);
    if let Some(provider) = options.provider.as_ref().filter(|value| !value.is_empty()) {
        args.extend(["--provider".into(), provider.clone()]);
    }
    if let Some(model) = options.model.as_ref().filter(|value| !value.is_empty()) {
        args.extend(["--model".into(), model.clone()]);
    }
    args.extend(options.args.iter().cloned());
    args
}

fn validate_spawn_options(
    file: &JsString,
    args: &[JsString],
    options: &RpcClientOptions,
    env: &pi_js_compat::js_json::JsObject,
) -> Result<(), RpcClientError> {
    use pi_js_compat::node_child_process::{validate_spawn_arguments, validate_string_argument};
    validate_spawn_arguments(file, args).map_err(RpcClientError::Native)?;
    if let Some(cwd) = &options.cwd {
        validate_string_argument(
            "options.cwd",
            "must be a string, Uint8Array, or URL without null bytes",
            cwd,
        )
        .map_err(RpcClientError::Native)?;
    }
    for (key, value) in js_json::object_entries_lossless(env) {
        let label = pi_js_compat::js_format!("options.env['{}']", key);
        validate_string_argument(&label, "must be a string without null bytes", key).map_err(RpcClientError::Native)?;
        let value = value.as_string().expect("spawn environment values are strings");
        validate_string_argument(&label, "must be a string without null bytes", value)
            .map_err(RpcClientError::Native)?;
    }
    Ok(())
}

fn load_spawn_environment(options: &RpcClientOptions) -> pi_js_compat::js_json::JsObject {
    let mut env: pi_js_compat::js_json::JsObject = pi_js_compat::process_env::vars()
        .into_iter()
        .map(|(key, value)| (key.into(), JsValue::String(value.into())))
        .collect();
    env.extend(
        options
            .env
            .iter()
            .map(|(key, value)| (key.clone(), JsValue::String(value.clone()))),
    );
    env
}

fn spawn_process(options: &RpcClientOptions, stderr: String) -> Result<SpawnedProcess, RpcClientError> {
    let executable = options.executable_path.clone().unwrap_or_else(|| "node".into());
    let args = spawn_arguments(options);
    let env = load_spawn_environment(options);
    validate_spawn_options(&executable, &args, options, &env)?;
    let mut command = Command::new(encode_node_utf8(&executable));
    command.args(args.iter().map(encode_node_utf8));
    if let Some(cwd) = &options.cwd {
        command.current_dir(encode_node_utf8(cwd));
    }
    command
        .env_clear()
        .envs(js_json::object_entries_lossless(&env).into_iter().map(|(key, value)| {
            (
                encode_node_utf8(key),
                encode_node_utf8(value.as_string().expect("spawn environment value")),
            )
        }));
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|error| {
        let record = pi_js_compat::node_fs::node_spawn_error_record(&error, &executable);
        if pi_js_compat::node_child_process::spawn_error_is_async(&error) {
            RpcClientError::ProcessError {
                message: record.message,
                stderr,
            }
        } else {
            RpcClientError::Native(Box::new(record.into()))
        }
    })?;
    let stdin = child.stdin.take().expect("piped child stdin must exist");
    let stdout = child.stdout.take().expect("piped child stdout must exist");
    let stderr = child.stderr.take().expect("piped child stderr must exist");
    let pid = child.id().expect("spawned child must have a pid");
    Ok(SpawnedProcess {
        child,
        stdin,
        stdout,
        stderr,
        pid,
    })
}

fn install_process(inner: &Arc<ClientInner>, generation: u64, spawned: SpawnedProcess, process: &mut ProcessState) {
    // Node installs these listeners in one stack before any child callback can run.
    let mut sync = inner.sync.lock().expect("sync state poisoned");
    sync.active_generation = Some(generation);
    sync.process_error_listener = true;
    let (exit_sender, exit_receiver) = watch::channel(None);

    let stderr_inner = Arc::clone(inner);
    tokio::spawn(async move {
        read_stderr(stderr_inner, spawned.stderr).await;
    });

    let reader_inner = Arc::clone(inner);
    let reader = attach_jsonl_line_reader(spawned.stdout, move |line| {
        reader_inner.handle_line(&line);
        Ok(())
    });

    let wait_inner = Arc::clone(inner);
    tokio::spawn(async move {
        let mut child = spawned.child;
        let status = child.wait().await;
        publish_exit(wait_inner, generation, exit_sender, status);
    });

    process.child = Some(ProcessHandle {
        pid: spawned.pid,
        generation,
        exit: exit_receiver,
    });
    let (stdin_sender, stdin_receiver) = mpsc::unbounded_channel();
    tokio::spawn(write_stdin(
        Arc::clone(inner),
        generation,
        spawned.stdin,
        stdin_receiver,
    ));
    process.reader = Some(reader);
    sync.stdin = Some(stdin_sender);
}

async fn write_stdin(
    inner: Arc<ClientInner>,
    generation: u64,
    mut writer: ChildStdin,
    mut lines: mpsc::UnboundedReceiver<String>,
) {
    while let Some(line) = lines.recv().await {
        if let Err(error) = writer.write_all(line.as_bytes()).await {
            let mut sync = inner.sync.lock().expect("sync state poisoned");
            if sync.active_generation != Some(generation) {
                return;
            }
            let failure = sync.exit_error.clone().unwrap_or_else(|| RpcClientError::StdinError {
                message: encode_node_utf8(&pi_js_compat::node_fs::node_stream_error_record(&error, "write").message),
                stderr: sync.stderr.clone(),
            });
            sync.exit_error = Some(failure.clone());
            reject_pending(&mut sync, failure);
            return;
        }
    }
}

async fn read_stderr(inner: Arc<ClientInner>, mut reader: impl AsyncRead + Unpin) {
    let mut buffer = vec![0_u8; STDERR_CHUNK_BYTES];
    loop {
        match reader.read(&mut buffer).await {
            Ok(0) => return,
            Err(error) => crate::utils::uncaught::throw_native_uncaught(
                pi_js_compat::node_fs::node_stream_error_record(&error, "read").into(),
            ),
            Ok(bytes_read) => {
                let chunk = &buffer[..bytes_read];
                inner
                    .sync
                    .lock()
                    .expect("sync state poisoned")
                    .stderr
                    .push_str(&String::from_utf8_lossy(chunk));
                let _ = std::io::stderr().write_all(chunk);
            }
        }
    }
}

fn publish_exit(
    inner: Arc<ClientInner>,
    generation: u64,
    sender: watch::Sender<Option<ExitInfo>>,
    status: std::io::Result<std::process::ExitStatus>,
) {
    let status = match status {
        Ok(status) => status,
        Err(error) => {
            publish_process_error(
                &inner,
                generation,
                pi_js_compat::node_fs::node_stream_error_record(&error, "wait").into(),
            );
            return;
        }
    };
    let info = exit_info(&status);
    {
        let mut sync = inner.sync.lock().expect("sync state poisoned");
        if sync.active_generation == Some(generation) {
            sync.exit = Some(info);
            sync.stdin = None;
            let error = process_exit_error(info, &sync.stderr);
            sync.exit_error = Some(error.clone());
            reject_pending(&mut sync, error);
        }
    }
    let _ = sender.send(Some(info));
}

fn publish_process_error(inner: &Arc<ClientInner>, generation: u64, native: pi_js_compat::node_error::NativeNodeError) {
    let mut sync = inner.sync.lock().expect("sync state poisoned");
    if sync.active_generation != Some(generation) {
        return;
    }
    if !sync.process_error_listener {
        drop(sync);
        crate::utils::uncaught::throw_native_uncaught(native);
    }
    sync.process_error_listener = false;
    let error = RpcClientError::ProcessError {
        message: native.message,
        stderr: sync.stderr.clone(),
    };
    sync.exit_error = Some(error.clone());
    reject_pending(&mut sync, error);
}

async fn wait_for_exit(receiver: &mut watch::Receiver<Option<ExitInfo>>, timeout: Duration) -> bool {
    matches!(
        tokio::time::timeout(timeout, receiver.wait_for(Option::is_some)).await,
        Ok(Ok(_))
    )
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, AtomicUsize};

    use super::*;

    fn temp_script(name: &str, contents: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("temp dir must be created");
        let path = dir.path().join(name);
        std::fs::write(&path, contents).expect("child script must be written");
        (dir, path)
    }

    mod source_oracle {
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/oracle.rs"));
    }

    struct BrokenRead;
    impl AsyncRead for BrokenRead {
        fn poll_read(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
            _: &mut tokio::io::ReadBuf<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            std::task::Poll::Ready(Err(std::io::Error::from_raw_os_error(
                rustix::io::Errno::IO.raw_os_error(),
            )))
        }
    }

    #[test]
    fn internal_collection_timeout_is_unobserved_before_prompt_ack() {
        if let Ok(output) = std::env::var("PI_RPC_COLLECTION_PROBE") {
            run_collection_probe(PathBuf::from(output));
            return;
        }
        let mut source = source_oracle::TsOracle::spawn(
            "rpc-client-oracle.mts",
            "packages/coding-agent/src/modes/rpc/rpc-client.ts",
        );
        let (root, script) = temp_script("child.mjs", include_str!("../../../../../oracle/rpc-client-child.mjs"));
        for fail_prompt in [false, true] {
            let operation = if fail_prompt {
                "prompt_fail_lifetime"
            } else {
                "promise_lifetime"
            };
            let expected = source.request(&pi_js_compat::js_json!({"operation":operation,"reply":{"type":"response","command":"prompt","success":true}}));
            if fail_prompt {
                assert_ne!(expected["value"]["unhandled"], expected["value"]["result"]["error"]);
            } else {
                assert_eq!(expected["value"]["unhandled"], expected["value"]["result"]["error"]);
            }
            let output_path = root.path().join("uncaught.txt");
            let output = run_private_collection_probe(&script, &output_path, fail_prompt);
            assert_eq!(
                output.status.code(),
                Some(1),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let unhandled = &expected["value"]["unhandled"];
            let expected_text = pi_js_compat::js_format!(
                "{}: {}",
                unhandled["name"].as_string().unwrap(),
                unhandled["message"].as_string().unwrap()
            );
            assert_eq!(
                std::fs::read_to_string(output_path).unwrap(),
                encode_node_utf8(&expected_text)
            );
        }
    }

    fn run_collection_probe(output: PathBuf) {
        crate::utils::uncaught::set_process_crash_handler(Some(Arc::new(move |payload| {
            std::fs::write(
                &output,
                encode_node_utf8(&crate::utils::uncaught::format_uncaught(payload)),
            )
            .unwrap();
        })));
        crate::utils::uncaught::install_uncaught_handler();
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let fail_prompt = std::env::var("PI_RPC_COLLECTION_FAIL").as_deref() == Ok("1");
                let client = RpcClient::new(RpcClientOptions {
                    cli_path: Some(std::env::var("PI_RPC_COLLECTION_SCRIPT").unwrap().into()),
                    env: IndexMap::from([
                        (
                            "PI_RPC_CLIENT_REPLY".into(),
                            r#"{"type":"response","command":"prompt","success":true}"#.into(),
                        ),
                        (
                            "PI_RPC_DELAY_ACK".into(),
                            if fail_prompt { "0".into() } else { "1".into() },
                        ),
                        (
                            "PI_RPC_EXIT_ON_LINE".into(),
                            if fail_prompt { "1".into() } else { "0".into() },
                        ),
                    ]),
                    ..Default::default()
                });
                client.start().await.unwrap();
                let _ = client
                    .prompt_and_wait("input", None, if fail_prompt { 200.0 } else { 10.0 })
                    .await;
                tokio::time::sleep(Duration::from_millis(500)).await;
                std::process::exit(7);
            });
    }

    fn run_private_collection_probe(
        script: &std::path::Path,
        output_path: &std::path::Path,
        fail_prompt: bool,
    ) -> std::process::Output {
        use std::os::unix::process::CommandExt;
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "modes::rpc::rpc_client::tests::internal_collection_timeout_is_unobserved_before_prompt_ack",
                "--nocapture",
            ])
            .env("PI_RPC_COLLECTION_PROBE", output_path)
            .env("PI_RPC_COLLECTION_SCRIPT", script)
            .env("PI_RPC_COLLECTION_FAIL", if fail_prompt { "1" } else { "0" })
            .process_group(0)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let group = rustix::process::Pid::from_raw(i32::try_from(child.id()).unwrap()).unwrap();
        let output = child.wait_with_output().unwrap();
        match rustix::process::kill_process_group(group, Signal::KILL) {
            Ok(()) | Err(rustix::io::Errno::SRCH) => {}
            Err(error) => panic!("clean private probe group: {error}"),
        }
        output
    }

    #[tokio::test]
    async fn native_wait_failure_rejects_pending_requests_instead_of_disappearing() {
        let (_root, script) = temp_script("child.mjs", "process.stdin.resume();setInterval(()=>{},1000);");
        let client = RpcClient::new(RpcClientOptions {
            cli_path: Some(script.to_str().unwrap().into()),
            ..Default::default()
        });
        client.start().await.unwrap();
        let mut request = Box::pin(client.send(RpcCommandBody::GetState));
        assert!(futures_util::poll!(request.as_mut()).is_pending());
        let generation = client.inner.sync.lock().unwrap().active_generation.unwrap();
        let (exit_sender, _exit_receiver) = watch::channel(None);
        publish_exit(
            Arc::clone(&client.inner),
            generation,
            exit_sender,
            Err(std::io::Error::from_raw_os_error(rustix::io::Errno::IO.raw_os_error())),
        );
        let outcome = tokio::time::timeout(Duration::from_millis(100), request).await;
        client.stop().await.unwrap();
        let error = outcome
            .expect("native wait failure must settle the pending request")
            .unwrap_err();
        assert_eq!(error.message(), "Agent process error: wait EIO. Stderr: ");
    }

    #[tokio::test]
    async fn child_error_listener_observes_once_then_routes_the_original_error_uncaught() {
        let mut source = source_oracle::TsOracle::spawn(
            "rpc-stream-errors-oracle.mts",
            "packages/coding-agent/src/modes/rpc/rpc-client.ts",
        );
        let expected = source.request(&pi_js_compat::js_json!({"operation":"child_errors"}));
        let (_root, script) = temp_script("child.mjs", "process.stdin.resume();setInterval(()=>{},1000);");
        let deny = Arc::new(AtomicBool::new(true));
        let denied = deny.clone();
        let client = Arc::new(RpcClient::with_kill(
            RpcClientOptions {
                cli_path: Some(script.to_str().unwrap().into()),
                ..Default::default()
            },
            Arc::new(move |pid, signal| {
                if denied.load(Ordering::SeqCst) {
                    Err(std::io::Error::from_raw_os_error(
                        rustix::io::Errno::PERM.raw_os_error(),
                    ))
                } else {
                    kill_process(pid, signal)
                }
            }),
        ));
        client.start().await.unwrap();
        let mut request = Box::pin(client.get_state());
        assert!(futures_util::poll!(request.as_mut()).is_pending());
        {
            let process = client.inner.process.lock().await;
            client
                .signal_child(process.child.as_ref().unwrap(), Signal::TERM)
                .unwrap();
        }
        let first = request.await.unwrap_err().value();
        let task_client = client.clone();
        let task = tokio::spawn(async move {
            let process = task_client.inner.process.lock().await;
            task_client
                .signal_child(process.child.as_ref().unwrap(), Signal::KILL)
                .unwrap();
        });
        let panic = task
            .await
            .expect_err("second child error must be uncaught")
            .into_panic();
        let second = panic
            .downcast_ref::<pi_js_compat::node_error::NativeNodeError>()
            .unwrap()
            .value();
        deny.store(false, Ordering::SeqCst);
        client.stop().await.unwrap();
        assert_eq!(pi_js_compat::js_json!({"first":first,"second":second}), expected);
    }

    #[tokio::test]
    async fn injected_unreachable_signal_errors_return_native_errors_without_orphan_timers() {
        for (errno, in_kill_phase) in [(rustix::io::Errno::INVAL, false), (rustix::io::Errno::NOSYS, true)] {
            let (_root, script) = temp_script("child.mjs", "process.stdin.resume();setInterval(()=>{},1000);");
            let injected = Arc::new(AtomicBool::new(true));
            let inject = injected.clone();
            let client = RpcClient::with_kill(
                RpcClientOptions {
                    cli_path: Some(script.to_str().unwrap().into()),
                    ..Default::default()
                },
                Arc::new(move |pid, signal| {
                    if !inject.load(Ordering::SeqCst) {
                        return kill_process(pid, signal);
                    }
                    if in_kill_phase && signal == Signal::TERM {
                        return Ok(());
                    }
                    Err(std::io::Error::from_raw_os_error(errno.raw_os_error()))
                }),
            );
            client.start().await.unwrap();
            let error = tokio::time::timeout(Duration::from_secs(2), client.stop())
                .await
                .unwrap()
                .unwrap_err();
            injected.store(false, Ordering::SeqCst);
            client.stop().await.unwrap();
            let value = error.value();
            assert_eq!(value["name"], JsValue::String("Error".into()));
            assert_eq!(value["syscall"], JsValue::String("kill".into()));
            assert_eq!(value["errno"], JsValue::Number(-f64::from(errno.raw_os_error())));
            assert_eq!(
                value["code"],
                JsValue::String(if in_kill_phase {
                    "ENOSYS".into()
                } else {
                    "EINVAL".into()
                })
            );
        }
    }

    #[tokio::test]
    async fn stderr_read_errors_reach_the_shared_native_uncaught_authority() {
        let mut source = source_oracle::TsOracle::spawn(
            "rpc-stream-errors-oracle.mts",
            "packages/coding-agent/src/modes/rpc/rpc-client.ts",
        );
        let expected = source.request(&pi_js_compat::js_json!({"operation":"stderr"}));
        let client = RpcClient::default();
        let task = tokio::spawn(read_stderr(Arc::clone(&client.inner), BrokenRead));
        let panic = task
            .await
            .expect_err("unhandled stderr read error must not disappear")
            .into_panic();
        let error = panic
            .downcast_ref::<pi_js_compat::node_error::NativeNodeError>()
            .expect("shared native error payload");
        assert_eq!(error.value(), expected);
    }

    #[tokio::test]
    async fn concurrent_stops_share_the_same_failure_and_termination_attempt() {
        let mut source = source_oracle::TsOracle::spawn(
            "rpc-client-oracle.mts",
            "packages/coding-agent/src/modes/rpc/rpc-client.ts",
        );
        for missing_pid in [false, true] {
            let expected =
                source.request(&pi_js_compat::js_json!({"operation":"stop_fail","reply":{},"missingPid":missing_pid}));
            let (_root, script) = temp_script("child.mjs", "process.stdin.resume();setInterval(()=>{},1000);");
            let blocked = Arc::new(AtomicBool::new(true));
            let calls = Arc::new(AtomicUsize::new(0));
            let kill_blocked = blocked.clone();
            let kill_calls = calls.clone();
            let client = RpcClient::with_kill(
                RpcClientOptions {
                    cli_path: Some(script.to_str().unwrap().into()),
                    ..Default::default()
                },
                Arc::new(move |pid, signal| {
                    if kill_blocked.load(Ordering::SeqCst) {
                        kill_calls.fetch_add(1, Ordering::SeqCst);
                        if missing_pid {
                            Err(std::io::Error::from_raw_os_error(
                                rustix::io::Errno::SRCH.raw_os_error(),
                            ))
                        } else {
                            Ok(())
                        }
                    } else {
                        kill_process(pid, signal)
                    }
                }),
            );
            client.start().await.unwrap();
            let (first, second) = tokio::join!(client.stop(), client.stop());
            let actual = pi_js_compat::js_json!({"ok":true,"value":{"calls":calls.load(Ordering::SeqCst),"errors":[first.err().map(|error|error.to_string()),second.err().map(|error|error.to_string())]}});
            blocked.store(false, Ordering::SeqCst);
            client.stop().await.unwrap();
            assert_eq!(actual, expected);
        }
    }

    #[tokio::test]
    async fn stop_failure_retains_the_process_handle_and_reports_previous_failure() {
        let (_dir, script) = temp_script(
            "child.mjs",
            r#"process.on("SIGTERM", () => {});
process.stdin.resume();
setInterval(() => {}, 1000);
"#,
        );
        let fail_kill = Arc::new(AtomicBool::new(true));
        let calls = Arc::new(AtomicUsize::new(0));
        let kill_flag = Arc::clone(&fail_kill);
        let kill_calls = Arc::clone(&calls);
        let client = RpcClient::with_kill(
            RpcClientOptions {
                cli_path: Some(script.to_str().unwrap().into()),
                ..RpcClientOptions::default()
            },
            Arc::new(move |pid, signal| {
                kill_calls.fetch_add(1, Ordering::SeqCst);
                if kill_flag.load(Ordering::SeqCst) {
                    return Ok(());
                }
                kill_process(pid, signal)
            }),
        );

        client.start().await.expect("client must start");
        let error = client.stop().await.expect_err("stop must fail");
        assert_eq!(error.to_string(), "RPC child did not exit after SIGKILL. Stderr: ");
        assert_eq!(calls.load(Ordering::SeqCst), 2);

        let error = client.start().await.expect_err("start must be rejected");
        assert_eq!(error.to_string(), "Client already started or stopping");
        let error = client
            .send(RpcCommandBody::GetCommands)
            .await
            .expect_err("send must be rejected");
        assert_eq!(
            error.to_string(),
            "Previous RPC stop failed: RPC child did not exit after SIGKILL. Stderr: "
        );

        fail_kill.store(false, Ordering::SeqCst);
        client.stop().await.expect("retry must stop the child");
    }
}
