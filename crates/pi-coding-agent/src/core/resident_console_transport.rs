//! Resident console transport (`core/resident-console-transport.ts`).

use std::collections::HashMap;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::unix::OwnedWriteHalf;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{mpsc, oneshot};

use crate::utils::private_paths::{create_private_directory, remove_socket_file_if_present, set_private_socket_mode};

pub const RESIDENT_CONSOLE_PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResidentConsoleService {
    Architect,
    Supervisor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResidentConsoleSnapshot<Entry> {
    pub service: ResidentConsoleService,
    pub session_id: String,
    pub cwd: String,
    pub generation: u64,
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
pub type DisconnectListener = Arc<dyn Fn(Option<String>) + Send + Sync>;

type EventListeners<Event> = Arc<Mutex<Vec<(u64, EventListener<Event>)>>>;
type DisconnectListeners = Arc<Mutex<Vec<(u64, DisconnectListener)>>>;
type HostFuture = Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;

/// Host services the transport calls into.
pub trait ResidentConsoleHost<Entry, Event>: Send + Sync {
    fn get_snapshot(&self) -> ResidentConsoleSnapshot<Entry>;
    fn enqueue_prompt(&self, text: String, id: String) -> HostFuture;
    fn subscribe(&self, listener: Arc<dyn Fn(Event) + Send + Sync>) -> Unsubscribe;
}

pub struct ResidentConsoleServerOptions<Entry, Event> {
    pub socket_path: PathBuf,
    pub service: ResidentConsoleService,
    pub host: Arc<dyn ResidentConsoleHost<Entry, Event>>,
}

type LineSender = mpsc::UnboundedSender<String>;

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
    socket_path: PathBuf,
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

    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    pub async fn start(&self) -> Result<(), String> {
        if self.accept_task().is_some() {
            return Ok(());
        }
        assert_unix_socket_support()?;
        if let Some(directory) = self.socket_path.parent() {
            create_private_directory(directory)?;
        }
        let listener = listen_with_stale_socket_recovery(&self.socket_path, "Resident console socket").await?;
        set_private_socket_mode(&self.socket_path)?;
        let state = Arc::clone(&self.state);
        let task = tokio::spawn(async move {
            while let Ok((stream, _address)) = listener.accept().await {
                tokio::spawn(serve_connection(Arc::clone(&state), stream));
            }
        });
        *self.accept_task() = Some(task);
        Ok(())
    }

    pub async fn close(&self) -> Result<(), String> {
        if let Some(task) = self.accept_task().take() {
            task.abort();
        }
        self.state.release_owner();
        for (_, sender) in self.state.connections().drain(..) {
            let _ = sender.send(String::new());
        }
        remove_socket_file_if_present(&self.socket_path);
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
        remove_socket_file_if_present(&self.socket_path);
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
            let _ = owner.sender.send(String::new());
        }
    }

    fn write(&self, sender: &LineSender, message: Value) {
        let _ = sender.send(message.to_string());
    }

    fn send_error(&self, sender: &LineSender, code: &str, message: &str, id: Option<&str>) {
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
    let (sender, mut receiver) = mpsc::unbounded_channel::<String>();
    tokio::spawn(async move {
        while let Some(line) = receiver.recv().await {
            if line.is_empty() {
                break;
            }
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
    let Ok(message) = serde_json::from_str::<Value>(line) else {
        state.send_error(sender, "invalid_message", "Invalid resident console message", None);
        return;
    };
    match message["type"].as_str() {
        Some("attach") => attach(state, connection_id, sender, &message),
        _ => handle_owned_message(state, connection_id, sender, &message).await,
    }
}

fn attach<Entry, Event>(
    state: &Arc<ServerState<Entry, Event>>,
    connection_id: u64,
    sender: &LineSender,
    message: &Value,
) where
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
        return;
    }
    let service = serde_json::from_value::<ResidentConsoleService>(message["service"].clone()).ok();
    if service != Some(state.service) {
        state.send_error(sender, "service_mismatch", "Resident console service mismatch", None);
        return;
    }
    state.release_owner();
    let snapshot = state.host.get_snapshot();
    let sequence = Arc::clone(&state.sequence);
    let event_sender = sender.clone();
    let unsubscribe = state.host.subscribe(Arc::new(move |event: Event| {
        let mut sequence = sequence.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        *sequence += 1;
        let _ = event_sender.send(json!({"type": "event", "sequence": *sequence, "event": event}).to_string());
    }));
    *state.owner() = Some(Owner {
        connection_id,
        sender: sender.clone(),
        unsubscribe: Some(unsubscribe),
        marker: PhantomData,
    });
    state.write(
        sender,
        json!({
            "type": "attached",
            "version": RESIDENT_CONSOLE_PROTOCOL_VERSION,
            "service": snapshot.service,
            "sessionId": snapshot.session_id,
            "cwd": snapshot.cwd,
            "generation": snapshot.generation,
            "branch": snapshot.branch,
        }),
    );
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
    match message["type"].as_str() {
        Some("disconnect") => {
            let _ = sender.send(String::new());
        }
        Some("prompt") => {
            let (Some(id), Some(text)) = (message["id"].as_str(), message["text"].as_str()) else {
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

fn enqueue_prompt<Entry, Event>(state: &Arc<ServerState<Entry, Event>>, sender: LineSender, id: String, text: String)
where
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

fn assert_unix_socket_support() -> Result<(), String> {
    if cfg!(windows) {
        return Err("Resident console requires Unix domain sockets".to_owned());
    }
    Ok(())
}

async fn listen_with_stale_socket_recovery(socket_path: &Path, label: &str) -> Result<UnixListener, String> {
    match UnixListener::bind(socket_path) {
        Ok(listener) => Ok(listener),
        Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
            if socket_accepts_connections(socket_path).await? {
                return Err(format!("{label} is already active: {}", socket_path.display()));
            }
            remove_socket_file_if_present(socket_path);
            UnixListener::bind(socket_path).map_err(|error| error.to_string())
        }
        Err(error) => Err(error.to_string()),
    }
}

async fn socket_accepts_connections(socket_path: &Path) -> Result<bool, String> {
    match UnixStream::connect(socket_path).await {
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
        Err(error) => Err(error.to_string()),
    }
}

struct ClientState<Event> {
    pending: Mutex<HashMap<String, oneshot::Sender<Result<(), String>>>>,
    listeners: EventListeners<Event>,
    close_listeners: DisconnectListeners,
    next_listener: AtomicU64,
    closed: AtomicBool,
    finish_guard: AtomicBool,
    finished: tokio::sync::watch::Sender<bool>,
}

pub struct ResidentConsoleClient<Entry, Event> {
    snapshot: ResidentConsoleSnapshot<Entry>,
    sender: mpsc::UnboundedSender<String>,
    state: Arc<ClientState<Event>>,
}

impl<Entry, Event> ResidentConsoleClient<Entry, Event>
where
    Entry: DeserializeOwned + Send + 'static,
    Event: DeserializeOwned + Send + 'static,
{
    pub async fn connect(socket_path: &str, service: ResidentConsoleService) -> Result<Self, String> {
        assert_unix_socket_support()?;
        let stream = connect_resident_console(socket_path).await?;
        let (read_half, write_half) = stream.into_split();
        let mut reader = BufReader::new(read_half);
        let (sender, receiver) = mpsc::unbounded_channel::<String>();
        spawn_client_writer(write_half, receiver);
        let _ = sender.send(
            json!({
                "type": "attach",
                "version": RESIDENT_CONSOLE_PROTOCOL_VERSION,
                "service": service,
            })
            .to_string(),
        );
        let mut line = String::new();
        reader.read_line(&mut line).await.map_err(|error| error.to_string())?;
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

    pub async fn prompt(&self, id: &str, text: &str) -> Result<(), String> {
        if self.state.closed.load(Ordering::SeqCst) {
            return Err("Resident console is closed".to_owned());
        }
        if id.is_empty() || text.is_empty() {
            return Err("Resident console prompt id and text are required".to_owned());
        }
        let (result_sender, result_receiver) = oneshot::channel();
        {
            let mut pending = self.state.pending();
            if pending.contains_key(id) {
                return Err(format!("Resident console prompt already pending: {id}"));
            }
            pending.insert(id.to_owned(), result_sender);
        }
        let _ = self
            .sender
            .send(json!({"type": "prompt", "id": id, "text": text}).to_string());
        match result_receiver.await {
            Ok(result) => result,
            Err(_) => Err("Resident console disconnected".to_owned()),
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
        let _ = self.sender.send(json!({"type": "disconnect"}).to_string());
        let _ = self.sender.send(String::new());
        let _ = finished.wait_for(|value| *value).await;
    }
}

impl<Event> ClientState<Event> {
    fn pending(&self) -> MutexGuard<'_, HashMap<String, oneshot::Sender<Result<(), String>>>> {
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

    fn finish(&self, error: Option<String>) {
        if self.finish_guard.swap(true, Ordering::SeqCst) {
            return;
        }
        let failure = error
            .clone()
            .unwrap_or_else(|| "Resident console disconnected".to_owned());
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
) -> Result<ResidentConsoleSnapshot<Entry>, String>
where
    Entry: DeserializeOwned,
{
    let Ok(message) = serde_json::from_str::<Value>(line) else {
        return Err("Invalid resident console attach response".to_owned());
    };
    if message["type"] == "error" {
        return Err(message["message"]
            .as_str()
            .map_or_else(|| "Resident attach failed".to_owned(), str::to_owned));
    }
    let valid = message["type"] == "attached"
        && message["version"].as_u64() == Some(u64::from(RESIDENT_CONSOLE_PROTOCOL_VERSION))
        && serde_json::from_value::<ResidentConsoleService>(message["service"].clone()).ok() == Some(service)
        && message["sessionId"].is_string()
        && message["cwd"].is_string()
        && message["generation"].is_u64()
        && message["branch"].is_array();
    if !valid {
        return Err("Invalid resident console attach response".to_owned());
    }
    let branch: Vec<Entry> = serde_json::from_value(message["branch"].clone())
        .map_err(|_| "Invalid resident console attach response".to_owned())?;
    Ok(ResidentConsoleSnapshot {
        service,
        session_id: message["sessionId"].as_str().unwrap_or_default().to_owned(),
        cwd: message["cwd"].as_str().unwrap_or_default().to_owned(),
        generation: message["generation"].as_u64().unwrap_or_default(),
        branch,
    })
}

fn spawn_client_writer(mut write_half: OwnedWriteHalf, mut receiver: mpsc::UnboundedReceiver<String>) {
    tokio::spawn(async move {
        while let Some(line) = receiver.recv().await {
            if line.is_empty() {
                break;
            }
            if write_half.write_all(format!("{line}\n").as_bytes()).await.is_err() {
                break;
            }
            if write_half.flush().await.is_err() {
                break;
            }
        }
        let _ = write_half.shutdown().await;
    });
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
                    state.finish(Some("Resident console disconnected".to_owned()));
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
    let Ok(message) = serde_json::from_str::<Value>(line) else {
        state.finish(Some("Invalid resident console response".to_owned()));
        return;
    };
    match message["type"].as_str() {
        Some("event") if message["sequence"].is_u64() && message.get("event").is_some() => {
            let Ok(event) = serde_json::from_value::<Event>(message["event"].clone()) else {
                state.finish(Some("Invalid resident console response".to_owned()));
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
        Some("prompt_accepted" | "error") => {
            let Some(id) = message["id"].as_str() else {
                return;
            };
            let Some(pending) = state.pending().remove(id) else {
                return;
            };
            if message["type"] == "prompt_accepted" {
                let _ = pending.send(Ok(()));
            } else {
                let error = message["message"]
                    .as_str()
                    .map_or_else(|| "Resident prompt rejected".to_owned(), str::to_owned);
                let _ = pending.send(Err(error));
            }
        }
        _ => {}
    }
}

async fn connect_resident_console(socket_path: &str) -> Result<UnixStream, String> {
    match UnixStream::connect(socket_path).await {
        Ok(stream) => Ok(stream),
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
            ) =>
        {
            Err(format!("Resident console is unavailable: {socket_path}"))
        }
        Err(error) => Err(error.to_string()),
    }
}
