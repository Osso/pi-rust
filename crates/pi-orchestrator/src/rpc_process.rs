//! Native child-process transport for the frozen orchestrator RPC wrapper.
use crate::ipc::protocol::{encode_message, parse_response_line};
use indexmap::IndexMap;
use pi_js_compat::{
    js_format,
    js_json::{JsString, JsValue as Value, encode_node_utf8},
};
use std::{
    process::Stdio,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::{ChildStdin, Command},
    sync::{oneshot, watch},
};

pub type Listener = Arc<dyn Fn(Value) + Send + Sync>;
pub type ExitListener = Arc<dyn Fn(Option<JsString>) + Send + Sync>;
type Pending = oneshot::Sender<Result<Value, JsString>>;
#[derive(Default)]
struct State {
    exited: bool,
    stderr: Vec<u8>,
    next_id: u64,
    pending: IndexMap<JsString, Pending>,
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
    fatal: watch::Sender<Option<JsString>>,
}
#[derive(Clone)]
pub struct RpcProcessInstance(Arc<Inner>);

pub fn random_uuid() -> Result<JsString, JsString> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|e| JsString::from(e.to_string()))?;
    bytes[6] = (bytes[6] & 15) | 64;
    bytes[8] = (bytes[8] & 63) | 128;
    let hex = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    Ok(js_format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    ))
}

impl RpcProcessInstance {
    pub fn spawn(cwd: &JsString) -> Result<Self, JsString> {
        let executable = std::env::current_exe().map_err(|e| JsString::from(e.to_string()))?;
        let sibling = executable
            .parent()
            .ok_or("executable has no directory")?
            .join(if cfg!(windows) { "pi.exe" } else { "pi" });
        Self::spawn_command(Command::new(sibling).args(["--mode", "rpc"]), cwd)
    }
    pub fn spawn_command(command: &mut Command, cwd: &JsString) -> Result<Self, JsString> {
        if let Some(error) = pi_js_compat::node_fs::node_path_argument_error(cwd) {
            return Err(error
                .message
                .replace("The argument 'path'", "The property 'options.cwd'"));
        }
        let program = JsString::from(
            command
                .as_std()
                .get_program()
                .to_str()
                .ok_or("RPC executable must be UTF-8")?,
        );
        let mut child = command
            .current_dir(encode_node_utf8(cwd))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| format_spawn_error(&error, &program))?;
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
    pub fn subscribe_fatal_errors(&self) -> watch::Receiver<Option<JsString>> {
        self.0.fatal.subscribe()
    }
    pub async fn send(&self, mut command: Value) -> Result<Value, JsString> {
        let (sender, receiver) = oneshot::channel();
        let id = {
            let mut state = self.0.state.lock().unwrap();
            if state.exited {
                return Err(js_format!(
                    "RPC process is not running. Stderr: {}",
                    JsString::from(String::from_utf8_lossy(&state.stderr).as_ref())
                ));
            }
            let id = if let Some(id) = command.get("id").filter(|v| !v.is_null()) {
                id.as_string().ok_or("RPC id must be a string")?.to_owned()
            } else {
                state.next_id += 1;
                js_format!("orchestrator_{}_{}", state.next_id, random_uuid()?)
            };
            if let Some(orphaned) = state.pending.insert(id.clone(), sender) {
                state.orphaned.push(orphaned);
            }
            id
        };
        command["id"] = Value::String(id.clone());
        if let Err(error) = self.write_message(&command).await {
            self.0.state.lock().unwrap().pending.shift_remove(&id);
            return Err(error);
        }
        receiver.await.map_err(|e| JsString::from(e.to_string()))?
    }
    async fn write_message(&self, value: &Value) -> Result<(), JsString> {
        self.0
            .stdin
            .lock()
            .await
            .write_all(encode_node_utf8(&encode_message(value)).as_bytes())
            .await
            .map_err(|error| {
                let message = pi_js_compat::node_fs::node_pipe_write_error_record(&error).message;
                self.0.fatal.send_replace(Some(message.clone()));
                message
            })
    }
    pub async fn handle_ui_response(&self, response: Value) -> Result<(), JsString> {
        if self.0.state.lock().unwrap().exited {
            return Ok(());
        }
        self.write_message(&response).await
    }
    pub fn set_ui_request_handler(&self, listener: Option<Listener>) {
        self.0.state.lock().unwrap().ui = listener;
    }
    pub fn on_event(&self, listener: Listener) -> usize {
        let mut state = self.0.state.lock().unwrap();
        if let Some((id, _)) = state
            .events
            .iter()
            .find(|(_, existing)| Arc::ptr_eq(existing, &listener))
        {
            return *id;
        }
        let id = self.0.ids.fetch_add(1, Ordering::Relaxed);
        state.events.insert(id, listener);
        id
    }
    pub fn remove_event_listener(&self, id: usize) {
        self.0.state.lock().unwrap().events.shift_remove(&id);
    }
    pub fn on_exit(&self, listener: ExitListener) -> usize {
        let mut state = self.0.state.lock().unwrap();
        if let Some((id, _)) = state
            .exits
            .iter()
            .find(|(_, existing)| Arc::ptr_eq(existing, &listener))
        {
            return *id;
        }
        let id = self.0.ids.fetch_add(1, Ordering::Relaxed);
        state.exits.insert(id, listener);
        id
    }
    pub fn remove_exit_listener(&self, id: usize) {
        self.0.state.lock().unwrap().exits.shift_remove(&id);
    }
    pub async fn dispose(&self) -> Result<(), JsString> {
        let mut exited = self.0.exited.subscribe();
        {
            let mut state = self.0.state.lock().unwrap();
            state.ui = None;
            reject_pending(&mut state, &JsString::from("RPC process disposed"));
            if state.exited {
                return Ok(());
            }
        }
        let pid = rustix::process::Pid::from_raw(self.0.pid as i32).ok_or("invalid RPC pid")?;
        match rustix::process::kill_process(pid, rustix::process::Signal::TERM) {
            Ok(()) => {}
            Err(rustix::io::Errno::SRCH) => {}
            Err(e) => return Err(JsString::from(e.to_string())),
        }
        while !*exited.borrow_and_update() {
            exited.changed().await.map_err(|e| JsString::from(e.to_string()))?;
        }
        Ok(())
    }
}

fn format_spawn_error(error: &std::io::Error, program: &JsString) -> JsString {
    let record = pi_js_compat::node_fs::node_spawn_error_record(error, program);
    if record.syscall == "spawn" {
        record.message
    } else {
        js_format!("RPC process error: {}. Stderr: ", record.message)
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
        // Node's exit event is independent of EOF on descendant-inherited pipes.
        finish_exit(&inner, status);
        let output_result = stdout_task.await;
        if let Err(error) = output_result.unwrap_or_else(|e| Err(JsString::from(e.to_string()))) {
            eprintln!("RPC stdout: {}", encode_node_utf8(&error));
        }
        if let Err(error) = stderr_task.await {
            eprintln!("RPC stderr reader: {error}");
        }
    });
}

async fn read_stderr(inner: Arc<Inner>, mut stderr: tokio::process::ChildStderr) {
    let mut bytes = [0; 8192];
    loop {
        match stderr.read(&mut bytes).await {
            Ok(0) => return,
            Ok(count) => inner.state.lock().unwrap().stderr.extend_from_slice(&bytes[..count]),
            Err(error) => {
                eprintln!("RPC stderr: {error}");
                return;
            }
        }
    }
}

async fn read_stdout(inner: &Inner, mut stdout: tokio::process::ChildStdout) -> Result<(), JsString> {
    let mut decoder = pi_coding_agent::modes::rpc::JsonlLineDecoder::new();
    let mut chunk = [0; 8192];
    loop {
        let count = stdout
            .read(&mut chunk)
            .await
            .map_err(|error| pi_js_compat::node_fs::node_stream_error_record(&error, "read").message)?;
        // Frozen orchestrator discards the unterminated tail (unlike the shared reader's end()).
        if count == 0 {
            return Ok(());
        }
        for line in decoder.push_bytes(&chunk[..count]) {
            let line = pi_js_compat::string::js_trim(&line);
            if !line.is_empty() {
                dispatch_line(inner, &JsString::from(line))?;
            }
        }
    }
}

fn dispatch_line(inner: &Inner, line: &JsString) -> Result<(), JsString> {
    let value = parse_response_line(line)?;
    let listeners = {
        let mut state = inner.state.lock().unwrap();
        match value["type"].as_string() {
            Some(tag) if tag == "response" => {
                if let Some(id) = value["id"].as_string().filter(|v| !v.is_empty())
                    && let Some(sender) = state.pending.shift_remove(id)
                {
                    let _ = sender.send(Ok(value.clone()));
                }
                vec![]
            }
            Some(tag) if tag == "extension_ui_request" => state.ui.iter().cloned().collect(),
            _ => {
                drop(state);
                notify_events(inner, &value);
                return Ok(());
            }
        }
    };
    for listener in listeners {
        listener(value.clone());
    }
    Ok(())
}
fn next_listener<T: Clone>(listeners: &IndexMap<usize, T>, previous: Option<usize>) -> Option<(usize, T)> {
    listeners
        .iter()
        .find(|(id, _)| previous.is_none_or(|previous| **id > previous))
        .map(|(id, listener)| (*id, listener.clone()))
}

fn notify_events(inner: &Inner, value: &Value) {
    let mut previous = None;
    loop {
        let next = next_listener(&inner.state.lock().unwrap().events, previous);
        let Some((id, listener)) = next else { return };
        previous = Some(id);
        listener(value.clone());
    }
}

fn notify_exit(inner: &Inner, error: &JsString) {
    let mut previous = None;
    loop {
        let next = next_listener(&inner.state.lock().unwrap().exits, previous);
        let Some((id, listener)) = next else { return };
        previous = Some(id);
        listener(Some(error.clone()));
    }
}

fn reject_pending(state: &mut State, error: &JsString) {
    for (_, sender) in state.pending.drain(..) {
        let _ = sender.send(Err(error.into()));
    }
}
fn finish_exit(inner: &Inner, status: std::io::Result<std::process::ExitStatus>) {
    use std::os::unix::process::ExitStatusExt;
    let error = {
        let mut state = inner.state.lock().unwrap();
        state.exited = true;
        let error = match status {
            Ok(status) => js_format!(
                "RPC process exited (code={} signal={}). Stderr: {}",
                status.code().map(|v| v.to_string()).unwrap_or("null".into()),
                status
                    .signal()
                    .map(pi_coding_agent::utils::child_process::signal_name)
                    .unwrap_or("null".into()),
                JsString::from(String::from_utf8_lossy(&state.stderr).as_ref())
            ),
            Err(e) => js_format!(
                "RPC process error: {}. Stderr: {}",
                e.to_string(),
                JsString::from(String::from_utf8_lossy(&state.stderr).as_ref())
            ),
        };
        reject_pending(&mut state, &error);
        error
    };
    notify_exit(inner, &error);
    inner.exited.send_replace(true);
}
