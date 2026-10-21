//! Native child-process transport for the frozen orchestrator RPC wrapper.
use crate::ipc::protocol::{encode_message, parse_response_line};
use indexmap::IndexMap;
use serde_json::Value;
use std::{
    collections::HashMap,
    path::Path,
    process::Stdio,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{ChildStdin, Command},
    sync::{oneshot, watch},
};

pub type Listener = Arc<dyn Fn(Value) + Send + Sync>;
pub type ExitListener = Arc<dyn Fn(Option<String>) + Send + Sync>;
type Pending = oneshot::Sender<Result<Value, String>>;
#[derive(Default)]
struct State {
    exited: bool,
    stderr: String,
    next_id: u64,
    pending: HashMap<String, Pending>,
    // Frozen Map replacement loses the old resolver: retain it, unsettled, until instance drop.
    orphaned: Vec<Pending>,
    events: IndexMap<usize, Listener>,
    exits: IndexMap<usize, ExitListener>,
    ui: Option<Listener>,
}
struct Inner {
    pid: u32,
    state: Mutex<State>,
    stdin: tokio::sync::Mutex<ChildStdin>,
    ids: AtomicUsize,
    exited: watch::Sender<bool>,
    fatal: watch::Sender<Option<String>>,
}
#[derive(Clone)]
pub struct RpcProcessInstance(Arc<Inner>);

pub fn random_uuid() -> Result<String, String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|e| e.to_string())?;
    bytes[6] = (bytes[6] & 15) | 64;
    bytes[8] = (bytes[8] & 63) | 128;
    let hex = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    ))
}

impl RpcProcessInstance {
    pub fn spawn(cwd: &Path) -> Result<Self, String> {
        let executable = std::env::current_exe().map_err(|e| e.to_string())?;
        let sibling = executable
            .parent()
            .ok_or("executable has no directory")?
            .join(if cfg!(windows) { "pi.exe" } else { "pi" });
        Self::spawn_command(Command::new(sibling).args(["--mode", "rpc"]), cwd)
    }
    pub fn spawn_command(command: &mut Command, cwd: &Path) -> Result<Self, String> {
        let mut child = command
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("RPC process error: {e}. Stderr: "))?;
        let stdin = child.stdin.take().ok_or("Failed to create RPC process stdio")?;
        let stdout = child.stdout.take().ok_or("Failed to create RPC process stdio")?;
        let stderr = child.stderr.take().ok_or("Failed to create RPC process stderr")?;
        let (exited, _) = watch::channel(false);
        let (fatal, _) = watch::channel(None);
        let inner = Arc::new(Inner {
            pid: child.id().ok_or("RPC process has no pid")?,
            state: Mutex::new(State::default()),
            stdin: tokio::sync::Mutex::new(stdin),
            ids: AtomicUsize::new(0),
            exited,
            fatal,
        });
        attach_readers(inner.clone(), stdout, stderr, child);
        Ok(Self(inner))
    }
    pub fn subscribe_fatal_errors(&self) -> watch::Receiver<Option<String>> {
        self.0.fatal.subscribe()
    }
    pub async fn send(&self, mut command: Value) -> Result<Value, String> {
        let (sender, receiver) = oneshot::channel();
        let id = {
            let mut state = self.0.state.lock().unwrap();
            if state.exited {
                return Err(format!("RPC process is not running. Stderr: {}", state.stderr));
            }
            let id = if let Some(id) = command.get("id").filter(|v| !v.is_null()) {
                id.as_str().ok_or("RPC id must be a string")?.to_owned()
            } else {
                state.next_id += 1;
                format!("orchestrator_{}_{}", state.next_id, random_uuid()?)
            };
            if let Some(orphaned) = state.pending.insert(id.clone(), sender) {
                state.orphaned.push(orphaned);
            }
            id
        };
        command["id"] = Value::String(id.clone());
        if let Err(error) = self.write_message(&command).await {
            self.0.state.lock().unwrap().pending.remove(&id);
            return Err(error);
        }
        receiver.await.map_err(|e| e.to_string())?
    }
    async fn write_message(&self, value: &Value) -> Result<(), String> {
        self.0
            .stdin
            .lock()
            .await
            .write_all(encode_message(value).as_bytes())
            .await
            .map_err(|e| e.to_string())
    }
    pub async fn handle_ui_response(&self, response: Value) -> Result<(), String> {
        if self.0.state.lock().unwrap().exited {
            return Ok(());
        }
        self.write_message(&response).await
    }
    pub fn set_ui_request_handler(&self, listener: Option<Listener>) {
        self.0.state.lock().unwrap().ui = listener;
    }
    pub fn on_event(&self, listener: Listener) -> usize {
        let id = self.0.ids.fetch_add(1, Ordering::Relaxed);
        self.0.state.lock().unwrap().events.insert(id, listener);
        id
    }
    pub fn remove_event_listener(&self, id: usize) {
        self.0.state.lock().unwrap().events.shift_remove(&id);
    }
    pub fn on_exit(&self, listener: ExitListener) -> usize {
        let id = self.0.ids.fetch_add(1, Ordering::Relaxed);
        self.0.state.lock().unwrap().exits.insert(id, listener);
        id
    }
    pub fn remove_exit_listener(&self, id: usize) {
        self.0.state.lock().unwrap().exits.shift_remove(&id);
    }
    pub async fn dispose(&self) -> Result<(), String> {
        let mut exited = self.0.exited.subscribe();
        {
            let mut state = self.0.state.lock().unwrap();
            state.ui = None;
            reject_pending(&mut state, "RPC process disposed");
            if state.exited {
                return Ok(());
            }
        }
        let pid = rustix::process::Pid::from_raw(self.0.pid as i32).ok_or("invalid RPC pid")?;
        match rustix::process::kill_process(pid, rustix::process::Signal::TERM) {
            Ok(()) => {}
            Err(rustix::io::Errno::SRCH) => {}
            Err(e) => return Err(e.to_string()),
        }
        while !*exited.borrow_and_update() {
            exited.changed().await.map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

fn attach_readers(
    inner: Arc<Inner>,
    stdout: tokio::process::ChildStdout,
    stderr: tokio::process::ChildStderr,
    mut child: tokio::process::Child,
) {
    let output = inner.clone();
    let stdout_task = tokio::spawn(async move {
        let result = read_stdout(&output, stdout).await;
        if let Err(error) = &result {
            output.fatal.send_replace(Some(error.clone()));
        }
        result
    });
    let stderr_task = tokio::spawn(read_stderr(inner.clone(), stderr));
    tokio::spawn(async move {
        let status = child.wait().await;
        let output_result = stdout_task.await;
        if let Err(error) = output_result.unwrap_or_else(|e| Err(e.to_string())) {
            eprintln!("RPC stdout: {error}");
        }
        if let Err(error) = stderr_task.await {
            eprintln!("RPC stderr reader: {error}");
        }
        finish_exit(&inner, status);
    });
}

async fn read_stderr(inner: Arc<Inner>, mut stderr: tokio::process::ChildStderr) {
    let mut bytes = [0; 8192];
    loop {
        match stderr.read(&mut bytes).await {
            Ok(0) => return,
            Ok(count) => inner
                .state
                .lock()
                .unwrap()
                .stderr
                .push_str(&String::from_utf8_lossy(&bytes[..count])),
            Err(error) => {
                eprintln!("RPC stderr: {error}");
                return;
            }
        }
    }
}

async fn read_stdout(inner: &Inner, stdout: tokio::process::ChildStdout) -> Result<(), String> {
    let mut lines = BufReader::new(stdout).lines();
    while let Some(line) = lines.next_line().await.map_err(|e| e.to_string())? {
        if !line.trim().is_empty() {
            dispatch_line(inner, &line)?;
        }
    }
    Ok(())
}

fn dispatch_line(inner: &Inner, line: &str) -> Result<(), String> {
    let value = parse_response_line(line)?;
    let listeners = {
        let mut state = inner.state.lock().unwrap();
        match value["type"].as_str() {
            Some("response") => {
                if let Some(id) = value["id"].as_str().filter(|v| !v.is_empty()) {
                    if let Some(sender) = state.pending.remove(id) {
                        let _ = sender.send(Ok(value.clone()));
                    }
                }
                vec![]
            }
            Some("extension_ui_request") => state.ui.iter().cloned().collect(),
            _ => state.events.values().cloned().collect(),
        }
    };
    for listener in listeners {
        listener(value.clone());
    }
    Ok(())
}
fn reject_pending(state: &mut State, error: &str) {
    for (_, sender) in state.pending.drain() {
        let _ = sender.send(Err(error.into()));
    }
}
fn finish_exit(inner: &Inner, status: std::io::Result<std::process::ExitStatus>) {
    use std::os::unix::process::ExitStatusExt;
    let (error, listeners) = {
        let mut state = inner.state.lock().unwrap();
        state.exited = true;
        let error = match status {
            Ok(status) => format!(
                "RPC process exited (code={} signal={}). Stderr: {}",
                status.code().map(|v| v.to_string()).unwrap_or("null".into()),
                status
                    .signal()
                    .map(pi_coding_agent::utils::child_process::signal_name)
                    .unwrap_or("null".into()),
                state.stderr
            ),
            Err(e) => format!("RPC process error: {e}. Stderr: {}", state.stderr),
        };
        reject_pending(&mut state, &error);
        (error, state.exits.values().cloned().collect::<Vec<_>>())
    };
    for listener in listeners {
        listener(Some(error.clone()));
    }
    inner.exited.send_replace(true);
}
