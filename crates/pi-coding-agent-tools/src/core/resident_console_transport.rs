//! Resident console transport (`core/resident-console-transport.ts`).

use std::collections::HashMap;
use std::marker::PhantomData;
use std::path::Path;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use pi_js_compat::{
    js_format, js_json as json,
    js_json::{self, JsString, JsValue as Value, encode_node_utf8},
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::unix::OwnedWriteHalf;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{mpsc, oneshot};

use crate::utils::private_paths::{remove_socket_file_if_present, set_private_socket_mode};

pub const RESIDENT_CONSOLE_PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResidentConsoleService {
    Architect,
    Supervisor,
}

impl ResidentConsoleService {
    /// CLI flag / socket suffix for the service.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Architect => "architect",
            Self::Supervisor => "supervisor",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResidentConsoleManagedBy {
    External,
    Pi,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResidentConsoleIdentity {
    pub version: JsString,
    pub pid: f64,
    pub executable: JsString,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entrypoint: Option<JsString>,
    pub instance_id: JsString,
    pub managed_by: ResidentConsoleManagedBy,
    pub ready: bool,
}

impl ResidentConsoleIdentity {
    /// Compiled runtimes identify their executable as the entrypoint, never argv[1].
    pub fn for_compiled_process(instance_id: JsString, managed_by: ResidentConsoleManagedBy) -> Self {
        let executable = std::env::current_exe()
            .unwrap_or_else(|error| crate::utils::uncaught::throw_uncaught("Error", error.to_string()));
        let executable = pi_js_compat::node_fs::decode_node_path(executable.as_os_str());
        Self {
            version: crate::config::package_config().version.clone(),
            pid: std::process::id() as f64,
            entrypoint: Some(executable.clone()),
            executable,
            instance_id,
            managed_by,
            ready: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResidentConsoleSnapshot<Entry> {
    pub service: ResidentConsoleService,
    pub session_id: JsString,
    pub cwd: JsString,
    pub generation: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<ResidentConsoleIdentity>,
    pub branch: Vec<Entry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResidentConsoleEvent<Event> {
    pub sequence: u64,
    pub event: Event,
}

pub type Unsubscribe = Box<dyn FnOnce() + Send>;

/// Event listener handed to [`ResidentConsoleClient::on_event`].
pub type EventListener<Event> = Arc<dyn Fn(&ResidentConsoleEvent<Event>) + Send + Sync>;

/// Disconnect listener handed to [`ResidentConsoleClient::on_disconnect`].
pub type DisconnectListener = Arc<dyn Fn(Option<JsString>) + Send + Sync>;

type EventListeners<Event> = Arc<Mutex<Vec<(u64, EventListener<Event>)>>>;
type DisconnectListeners = Arc<Mutex<Vec<(u64, DisconnectListener)>>>;
type HostFuture = Pin<Box<dyn Future<Output = Result<(), JsString>> + Send>>;

/// Host services the transport calls into.
pub trait ResidentConsoleHost<Entry, Event>: Send + Sync {
    fn get_snapshot(&self) -> ResidentConsoleSnapshot<Entry>;
    fn enqueue_prompt(&self, text: JsString, id: JsString) -> HostFuture;
    fn subscribe(&self, listener: Arc<dyn Fn(Event) + Send + Sync>) -> Unsubscribe;
}

pub struct ResidentConsoleServerOptions<Entry, Event> {
    pub socket_path: JsString,
    pub service: ResidentConsoleService,
    pub host: Arc<dyn ResidentConsoleHost<Entry, Event>>,
}

type LineSender = mpsc::UnboundedSender<Option<Value>>;

struct Owner<Event> {
    connection_id: u64,
    sender: LineSender,
    unsubscribe: Option<Unsubscribe>,
    marker: PhantomData<Event>,
}

struct ServerState<Entry, Event> {
    service: ResidentConsoleService,
    host: Arc<dyn ResidentConsoleHost<Entry, Event>>,
    owner: Mutex<Option<Owner<Event>>>,
    connections: Mutex<Vec<(u64, LineSender)>>,
    sequence: Arc<Mutex<u64>>,
    next_connection: AtomicU64,
}

pub struct ResidentConsoleServer<Entry, Event> {
    socket_path: JsString,
    state: Arc<ServerState<Entry, Event>>,
    accept_task: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl<Entry, Event> ResidentConsoleServer<Entry, Event>
where
    Entry: Serialize + DeserializeOwned + Send + Sync + 'static,
    Event: Serialize + Send + Sync + 'static,
{
    pub fn new(options: ResidentConsoleServerOptions<Entry, Event>) -> Self {
        Self {
            socket_path: options.socket_path,
            state: Arc::new(ServerState {
                service: options.service,
                host: options.host,
                owner: Mutex::new(None),
                connections: Mutex::new(Vec::new()),
                sequence: Arc::new(Mutex::new(0)),
                next_connection: AtomicU64::new(1),
            }),
            accept_task: Mutex::new(None),
        }
    }

    pub fn socket_path(&self) -> &JsString {
        &self.socket_path
    }

    pub async fn start(&self) -> Result<(), JsString> {
        if self.accept_task().is_some() {
            return Ok(());
        }
        assert_unix_socket_support()?;
        let socket_path = encode_node_utf8(&self.socket_path);
        if let Some(directory) = Path::new(&socket_path).parent() {
            use std::os::unix::fs::DirBuilderExt;
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(directory)
                .map_err(|error| {
                    let path = crate::utils::paths::node_dirname(&self.socket_path);
                    pi_js_compat::node_fs::node_fs_error(&error, "mkdir", &path)
                })?;
        }
        let listener = listen_with_stale_socket_recovery(&self.socket_path, "Resident console socket").await?;
        set_private_socket_mode(Path::new(&encode_node_utf8(&self.socket_path)))?;
        let state = Arc::clone(&self.state);
        let task = tokio::spawn(async move {
            while let Ok((stream, _address)) = listener.accept().await {
                tokio::spawn(serve_connection(Arc::clone(&state), stream));
            }
        });
        *self.accept_task() = Some(task);
        Ok(())
    }

    pub async fn close(&self) -> Result<(), JsString> {
        if let Some(task) = self.accept_task().take() {
            task.abort();
        }
        self.state.release_owner();
        for (_, sender) in self.state.connections().drain(..) {
            let _ = sender.send(None);
        }
        remove_socket_file_if_present(Path::new(&encode_node_utf8(&self.socket_path)));
        Ok(())
    }

    fn accept_task(&self) -> MutexGuard<'_, Option<tokio::task::JoinHandle<()>>> {
        self.accept_task
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl<Entry, Event> Drop for ResidentConsoleServer<Entry, Event> {
    fn drop(&mut self) {
        if let Some(task) = self
            .accept_task
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            task.abort();
        }
        remove_socket_file_if_present(Path::new(&encode_node_utf8(&self.socket_path)));
    }
}

impl<Entry, Event> ServerState<Entry, Event>
where
    Entry: Serialize + Send + Sync + 'static,
    Event: Serialize + Send + Sync + 'static,
{
    fn owner(&self) -> MutexGuard<'_, Option<Owner<Event>>> {
        self.owner.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn connections(&self) -> MutexGuard<'_, Vec<(u64, LineSender)>> {
        self.connections
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn release_owner(&self) {
        if let Some(mut owner) = self.owner().take() {
            if let Some(unsubscribe) = owner.unsubscribe.take() {
                unsubscribe();
            }
            let _ = owner.sender.send(None);
        }
    }

    fn write(&self, sender: &LineSender, message: Value) {
        let _ = sender.send(Some(message));
    }

    fn send_error(&self, sender: &LineSender, code: &str, message: impl Into<JsString>, id: Option<&JsString>) {
        let message = message.into();
        let mut payload = json!({"type": "error", "code": code, "message": message});
        if let Some(id) = id {
            payload["id"] = json!(id);
        }
        self.write(sender, payload);
    }
}

async fn serve_connection<Entry, Event>(state: Arc<ServerState<Entry, Event>>, stream: UnixStream)
where
    Entry: Serialize + Send + Sync + 'static,
    Event: Serialize + Send + Sync + 'static,
{
    let connection_id = state.next_connection.fetch_add(1, Ordering::SeqCst);
    let (read_half, write_half) = stream.into_split();
    let sender = spawn_writer(write_half);
    state.connections().push((connection_id, sender.clone()));
    let mut lines = BufReader::new(read_half).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        if line.is_empty() {
            continue;
        }
        handle_line(&state, connection_id, &sender, &line).await;
    }
    let is_owner = state
        .owner()
        .as_ref()
        .is_some_and(|owner| owner.connection_id == connection_id);
    if is_owner {
        state.release_owner();
    }
    state.connections().retain(|(id, _)| *id != connection_id);
}

fn spawn_writer(mut write_half: OwnedWriteHalf) -> LineSender {
    let (sender, mut receiver) = mpsc::unbounded_channel::<Option<Value>>();
    tokio::spawn(async move {
        while let Some(Some(message)) = receiver.recv().await {
            let line = js_json::stringify_lossless(&message);
            if write_half.write_all(line.as_bytes()).await.is_err() {
                break;
            }
            if write_half.write_all(b"\n").await.is_err() {
                break;
            }
            if write_half.flush().await.is_err() {
                break;
            }
        }
        let _ = write_half.shutdown().await;
    });
    sender
}

async fn handle_line<Entry, Event>(
    state: &Arc<ServerState<Entry, Event>>,
    connection_id: u64,
    sender: &LineSender,
    line: &str,
) where
    Entry: Serialize + Send + Sync + 'static,
    Event: Serialize + Send + Sync + 'static,
{
    let Ok(message) = js_json::parse_lossless(line) else {
        state.send_error(sender, "invalid_message", "Invalid resident console message", None);
        return;
    };
    match message["type"].as_string() {
        Some(kind) if kind == "attach" || kind == "probe" => {
            if !validate_request(state, sender, &message) {
                return;
            }
            if kind == "probe" {
                state.write(sender, snapshot_message("probed", &state.host.get_snapshot()));
            } else {
                attach(state, connection_id, sender);
            }
        }
        _ => handle_owned_message(state, connection_id, sender, &message).await,
    }
}

fn validate_request<Entry, Event>(state: &ServerState<Entry, Event>, sender: &LineSender, message: &Value) -> bool
where
    Entry: Serialize + Send + Sync + 'static,
    Event: Serialize + Send + Sync + 'static,
{
    if message["version"].as_u64() != Some(u64::from(RESIDENT_CONSOLE_PROTOCOL_VERSION)) {
        state.send_error(
            sender,
            "version_mismatch",
            "Resident console protocol version mismatch",
            None,
        );
        return false;
    }
    let service = js_json::from_value::<ResidentConsoleService>(&message["service"]).ok();
    if service != Some(state.service) {
        state.send_error(sender, "service_mismatch", "Resident console service mismatch", None);
        return false;
    }
    true
}

fn attach<Entry, Event>(state: &Arc<ServerState<Entry, Event>>, connection_id: u64, sender: &LineSender)
where
    Entry: Serialize + Send + Sync + 'static,
    Event: Serialize + Send + Sync + 'static,
{
    state.release_owner();
    let snapshot = state.host.get_snapshot();
    *state.owner() = Some(Owner {
        connection_id,
        sender: sender.clone(),
        unsubscribe: None,
        marker: PhantomData,
    });
    state.write(sender, snapshot_message("attached", &snapshot));
    let sequence = Arc::clone(&state.sequence);
    let event_sender = sender.clone();
    let unsubscribe = state.host.subscribe(Arc::new(move |event: Event| {
        let mut sequence = sequence.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        *sequence += 1;
        let _ = event_sender.send(Some(json!({"type": "event", "sequence": *sequence, "event": event})));
    }));
    if let Some(owner) = state.owner().as_mut() {
        owner.unsubscribe = Some(unsubscribe);
    }
}

fn snapshot_message<Entry: Serialize>(kind: &str, snapshot: &ResidentConsoleSnapshot<Entry>) -> Value {
    let mut message = json!({"type": kind, "version": RESIDENT_CONSOLE_PROTOCOL_VERSION});
    let snapshot = json!(snapshot);
    message
        .as_object_mut()
        .expect("frame object")
        .extend(snapshot.as_object().expect("snapshot object").clone());
    message
}

async fn handle_owned_message<Entry, Event>(
    state: &Arc<ServerState<Entry, Event>>,
    connection_id: u64,
    sender: &LineSender,
    message: &Value,
) where
    Entry: Serialize + Send + Sync + 'static,
    Event: Serialize + Send + Sync + 'static,
{
    let is_owner = state
        .owner()
        .as_ref()
        .is_some_and(|owner| owner.connection_id == connection_id);
    if !is_owner {
        state.send_error(sender, "missing_attach", "Resident console attach is required", None);
        return;
    }
    match message["type"].as_string() {
        Some(kind) if kind == "disconnect" => {
            let _ = sender.send(None);
        }
        Some(kind) if kind == "prompt" => {
            let (Some(id), Some(text)) = (message["id"].as_string(), message["text"].as_string()) else {
                state.send_error(sender, "invalid_message", "Invalid resident console message", None);
                return;
            };
            if id.is_empty() || text.is_empty() {
                state.send_error(sender, "invalid_message", "Invalid resident console message", None);
                return;
            }
            enqueue_prompt(state, sender.clone(), id.to_owned(), text.to_owned());
        }
        _ => state.send_error(sender, "invalid_message", "Invalid resident console message", None),
    }
}

fn enqueue_prompt<Entry, Event>(
    state: &Arc<ServerState<Entry, Event>>,
    sender: LineSender,
    id: JsString,
    text: JsString,
) where
    Entry: Serialize + Send + Sync + 'static,
    Event: Serialize + Send + Sync + 'static,
{
    let host = Arc::clone(&state.host);
    let state = Arc::clone(state);
    tokio::spawn(async move {
        match host.enqueue_prompt(text, id.clone()).await {
            Ok(()) => {
                state.write(&sender, json!({"type": "prompt_accepted", "id": id}));
            }
            Err(message) => state.send_error(&sender, "prompt_rejected", &message, Some(&id)),
        }
    });
}

fn assert_unix_socket_support() -> Result<(), JsString> {
    if cfg!(windows) {
        return Err("Resident console requires Unix domain sockets".into());
    }
    Ok(())
}

async fn listen_with_stale_socket_recovery(socket_path: &JsString, label: &str) -> Result<UnixListener, JsString> {
    match UnixListener::bind(encode_node_utf8(socket_path)) {
        Ok(listener) => Ok(listener),
        Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
            if socket_accepts_connections(socket_path).await? {
                return Err(js_format!("{} is already active: {}", label, socket_path));
            }
            remove_socket_file_if_present(Path::new(&encode_node_utf8(socket_path)));
            UnixListener::bind(encode_node_utf8(socket_path)).map_err(|error| JsString::from(error.to_string()))
        }
        Err(error) => Err(error.to_string().into()),
    }
}

async fn socket_accepts_connections(socket_path: &JsString) -> Result<bool, JsString> {
    match UnixStream::connect(encode_node_utf8(socket_path)).await {
        Ok(stream) => {
            drop(stream);
            Ok(true)
        }
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::NotFound
            ) =>
        {
            Ok(false)
        }
        Err(error) => Err(error.to_string().into()),
    }
}

/// Read a snapshot without attaching or displacing the resident's current owner.
pub async fn probe_resident_console<Entry: DeserializeOwned>(
    socket_path: impl Into<JsString>,
    service: ResidentConsoleService,
) -> Result<ResidentConsoleSnapshot<Entry>, JsString> {
    let socket_path = socket_path.into();
    assert_unix_socket_support()?;
    let mut stream = connect_resident_console(&socket_path).await?;
    let request = json!({"type":"probe","version":RESIDENT_CONSOLE_PROTOCOL_VERSION,"service":service});
    stream
        .write_all(format!("{}\n", js_json::stringify_lossless(&request)).as_bytes())
        .await
        .map_err(|error| JsString::from(error.to_string()))?;
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    if reader
        .read_line(&mut line)
        .await
        .map_err(|error| JsString::from(error.to_string()))?
        == 0
    {
        return Err("Resident console disconnected before attach".into());
    }
    parse_attach_response(&line, service)
}

struct ClientState<Event> {
    pending: Mutex<HashMap<JsString, oneshot::Sender<Result<(), JsString>>>>,
    listeners: EventListeners<Event>,
    close_listeners: DisconnectListeners,
    next_listener: AtomicU64,
    closed: AtomicBool,
    finish_guard: AtomicBool,
    finished: tokio::sync::watch::Sender<bool>,
}

pub struct ResidentConsoleClient<Entry, Event> {
    snapshot: ResidentConsoleSnapshot<Entry>,
    sender: LineSender,
    state: Arc<ClientState<Event>>,
}

impl<Entry, Event> ResidentConsoleClient<Entry, Event>
where
    Entry: DeserializeOwned + Send + 'static,
    Event: DeserializeOwned + Send + 'static,
{
    pub async fn connect(socket_path: impl Into<JsString>, service: ResidentConsoleService) -> Result<Self, JsString> {
        let socket_path = socket_path.into();
        assert_unix_socket_support()?;
        let stream = connect_resident_console(&socket_path).await?;
        let (read_half, write_half) = stream.into_split();
        let mut reader = BufReader::new(read_half);
        let sender = spawn_writer(write_half);
        let _ = sender.send(Some(json!({
            "type": "attach",
            "version": RESIDENT_CONSOLE_PROTOCOL_VERSION,
            "service": service,
        })));
        let mut line = String::new();
        if reader.read_line(&mut line).await.map_err(|error| error.to_string())? == 0 {
            return Err("Resident console disconnected before attach".into());
        }
        let snapshot = parse_attach_response::<Entry>(&line, service)?;
        let (finished, _finished_rx) = tokio::sync::watch::channel(false);
        let state = Arc::new(ClientState {
            pending: Mutex::new(HashMap::new()),
            listeners: Arc::new(Mutex::new(Vec::new())),
            close_listeners: Arc::new(Mutex::new(Vec::new())),
            next_listener: AtomicU64::new(1),
            closed: AtomicBool::new(false),
            finish_guard: AtomicBool::new(false),
            finished,
        });
        spawn_client_reader(reader, Arc::clone(&state));
        Ok(Self {
            snapshot,
            sender,
            state,
        })
    }

    pub fn snapshot(&self) -> &ResidentConsoleSnapshot<Entry> {
        &self.snapshot
    }

    pub async fn prompt(&self, id: impl Into<JsString>, text: impl Into<JsString>) -> Result<(), JsString> {
        let id = id.into();
        let text = text.into();
        if self.state.closed.load(Ordering::SeqCst) {
            return Err("Resident console is closed".into());
        }
        if id.is_empty() || text.is_empty() {
            return Err("Resident console prompt id and text are required".into());
        }
        let (result_sender, result_receiver) = oneshot::channel();
        {
            let mut pending = self.state.pending();
            if pending.contains_key(&id) {
                return Err(js_format!("Resident console prompt already pending: {}", id));
            }
            pending.insert(id.to_owned(), result_sender);
        }
        let _ = self
            .sender
            .send(Some(json!({"type": "prompt", "id": id, "text": text})));
        match result_receiver.await {
            Ok(result) => result,
            Err(_) => Err("Resident console disconnected".into()),
        }
    }

    pub fn on_event(&self, listener: EventListener<Event>) -> Unsubscribe {
        let id = self.state.next_listener.fetch_add(1, Ordering::SeqCst);
        self.state.listeners().push((id, listener));
        let listeners = Arc::clone(&self.state.listeners);
        Box::new(move || {
            listeners.lock().unwrap().retain(|(listener_id, _)| *listener_id != id);
        })
    }

    pub fn on_disconnect(&self, listener: DisconnectListener) -> Unsubscribe {
        let id = self.state.next_listener.fetch_add(1, Ordering::SeqCst);
        self.state.close_listeners().push((id, listener));
        let listeners = Arc::clone(&self.state.close_listeners);
        Box::new(move || {
            listeners.lock().unwrap().retain(|(listener_id, _)| *listener_id != id);
        })
    }

    pub async fn close(&self) {
        if self.state.closed.swap(true, Ordering::SeqCst) {
            return;
        }
        let mut finished = self.state.finished.subscribe();
        let _ = self.sender.send(Some(json!({"type": "disconnect"})));
        let _ = self.sender.send(None);
        let _ = finished.wait_for(|value| *value).await;
    }
}

impl<Event> ClientState<Event> {
    fn pending(&self) -> MutexGuard<'_, HashMap<JsString, oneshot::Sender<Result<(), JsString>>>> {
        self.pending.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn listeners(&self) -> MutexGuard<'_, Vec<(u64, EventListener<Event>)>> {
        self.listeners.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn close_listeners(&self) -> MutexGuard<'_, Vec<(u64, DisconnectListener)>> {
        self.close_listeners
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn finish(&self, error: Option<JsString>) {
        if self.finish_guard.swap(true, Ordering::SeqCst) {
            return;
        }
        self.closed.store(true, Ordering::SeqCst);
        let failure = error.clone().unwrap_or_else(|| "Resident console disconnected".into());
        for (_, sender) in self.pending().drain() {
            let _ = sender.send(Err(failure.clone()));
        }
        for (_, listener) in self.close_listeners().iter() {
            listener(error.clone());
        }
        let _ = self.finished.send(true);
    }
}

fn parse_attach_response<Entry>(
    line: &str,
    service: ResidentConsoleService,
) -> Result<ResidentConsoleSnapshot<Entry>, JsString>
where
    Entry: DeserializeOwned,
{
    let Ok(message) = js_json::parse_lossless(line) else {
        return Err("Invalid resident console attach response".into());
    };
    if message["type"].as_string().is_some_and(|kind| kind == "error") {
        return Err(message["message"]
            .as_string()
            .cloned()
            .unwrap_or_else(|| "Resident attach failed".into()));
    }
    let valid = message["type"]
        .as_string()
        .is_some_and(|kind| kind == "attached" || kind == "probed")
        && message["version"].as_u64() == Some(u64::from(RESIDENT_CONSOLE_PROTOCOL_VERSION))
        && js_json::from_value::<ResidentConsoleService>(&message["service"]).ok() == Some(service)
        && message["sessionId"].is_string()
        && message["cwd"].is_string()
        && message["generation"].as_u64().is_some()
        && message["branch"].is_array();
    if !valid {
        return Err("Invalid resident console attach response".into());
    }
    let branch: Vec<Entry> = js_json::from_value(&message["branch"])
        .map_err(|_| JsString::from("Invalid resident console attach response"))?;
    let identity = message
        .get("identity")
        .map(read_resident_console_identity)
        .transpose()?;
    Ok(ResidentConsoleSnapshot {
        service,
        session_id: message["sessionId"].as_string().cloned().unwrap_or_default(),
        cwd: message["cwd"].as_string().cloned().unwrap_or_default(),
        generation: message["generation"].as_u64().unwrap_or_default(),
        identity,
        branch,
    })
}

fn read_resident_console_identity(value: &Value) -> Result<ResidentConsoleIdentity, JsString> {
    let identity: ResidentConsoleIdentity =
        js_json::from_value(value).map_err(|_| JsString::from("Invalid resident console identity"))?;
    if !identity.ready || value.get("entrypoint").is_some_and(|entry| !entry.is_string()) {
        return Err("Invalid resident console identity".into());
    }
    Ok(identity)
}

fn spawn_client_reader<Event>(reader: BufReader<tokio::net::unix::OwnedReadHalf>, state: Arc<ClientState<Event>>)
where
    Event: DeserializeOwned + Send + 'static,
{
    tokio::spawn(async move {
        let mut lines = reader.lines();
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => handle_client_line(&state, &line),
                Ok(None) => break,
                Err(_) => {
                    state.finish(Some("Resident console disconnected".into()));
                    return;
                }
            }
        }
        state.finish(None);
    });
}

fn handle_client_line<Event>(state: &Arc<ClientState<Event>>, line: &str)
where
    Event: DeserializeOwned,
{
    if line.is_empty() {
        return;
    }
    let Ok(message) = js_json::parse_lossless(line) else {
        state.finish(Some("Invalid resident console response".into()));
        return;
    };
    match message["type"].as_string() {
        Some(kind) if kind == "event" && message["sequence"].as_u64().is_some() && message.get("event").is_some() => {
            let Ok(event) = js_json::from_value::<Event>(&message["event"]) else {
                state.finish(Some("Invalid resident console response".into()));
                return;
            };
            let event = ResidentConsoleEvent {
                sequence: message["sequence"].as_u64().unwrap_or_default(),
                event,
            };
            for (_, listener) in state.listeners().iter() {
                listener(&event);
            }
        }
        Some(kind) if kind == "prompt_accepted" || kind == "error" => {
            let Some(id) = message["id"].as_string() else {
                return;
            };
            let Some(pending) = state.pending().remove(id) else {
                return;
            };
            if message["type"]
                .as_string()
                .is_some_and(|kind| kind == "prompt_accepted")
            {
                let _ = pending.send(Ok(()));
            } else {
                let error = message["message"]
                    .as_string()
                    .cloned()
                    .unwrap_or_else(|| "Resident prompt rejected".into());
                let _ = pending.send(Err(error));
            }
        }
        _ => {}
    }
}

async fn connect_resident_console(socket_path: &JsString) -> Result<UnixStream, JsString> {
    match UnixStream::connect(encode_node_utf8(socket_path)).await {
        Ok(stream) => Ok(stream),
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
            ) =>
        {
            Err(js_format!("Resident console is unavailable: {}", socket_path))
        }
        Err(error) => Err(error.to_string().into()),
    }
}
