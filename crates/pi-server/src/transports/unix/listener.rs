use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::io::ErrorKind;
use std::os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use pi_protocol::DEFAULT_MAX_FRAME_LENGTH;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{UnixListener as TokioUnixListener, UnixStream};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;

use crate::connection::{ByteConnection, ByteConnectionAcceptor, ByteConnectionHandler, ConnectionResult};
use crate::listener::{ListenerResult, PiServerListener};
use crate::server::ErrorObserver;
use crate::transports::unix::types::UnixListenerOptions;
use crate::types::BoxFuture;

const DEFAULT_SOCKET_MODE: u32 = 0o600;
const DEFAULT_GRACEFUL_CLOSE_TIMEOUT_MS: u64 = 5_000;
const MAX_TIMER_DELAY_MS: u64 = 2_147_483_647;
const SOCKET_PROBE_TIMEOUT_MS: u64 = 1_000;
#[cfg(target_os = "linux")]
const MAX_UNIX_SOCKET_PATH_BYTES: usize = 107;
#[cfg(not(target_os = "linux"))]
const MAX_UNIX_SOCKET_PATH_BYTES: usize = 103;

/// Returned when Unix listener options violate their typed constraints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnixListenerConfigError {
    message: String,
}

impl UnixListenerConfigError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for UnixListenerConfigError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for UnixListenerConfigError {}

pub fn validate_unix_socket_path(path: &str, description: &str) -> Result<(), UnixListenerConfigError> {
    if path.is_empty() {
        return Err(UnixListenerConfigError::new(format!("{description} must not be empty")));
    }
    if path.len() > MAX_UNIX_SOCKET_PATH_BYTES {
        return Err(UnixListenerConfigError::new(format!(
            "{description} is too long; maximum is {MAX_UNIX_SOCKET_PATH_BYTES} UTF-8 bytes"
        )));
    }
    Ok(())
}

#[derive(Clone)]
struct ResolvedUnixListenerOptions {
    path: String,
    mode: u32,
    graceful_close_timeout_ms: u64,
    max_pending_bytes: usize,
    on_error: Option<ErrorObserver>,
}

impl ResolvedUnixListenerOptions {
    fn resolve(options: &UnixListenerOptions) -> Result<Self, UnixListenerConfigError> {
        validate_unix_socket_path(&options.path, "PiServer Unix socket path")?;
        let mode = options.mode.unwrap_or(DEFAULT_SOCKET_MODE);
        if mode > 0o777 {
            return Err(UnixListenerConfigError::new(
                "PiServer Unix socket mode must be an integer between 0 and 0o777",
            ));
        }
        let max_frame_length = options.max_frame_length.unwrap_or(DEFAULT_MAX_FRAME_LENGTH);
        if max_frame_length == 0 {
            return Err(UnixListenerConfigError::new(
                "PiServer maxFrameLength must be an integer between 1 and 4294967295",
            ));
        }
        let max_pending_bytes = options.max_pending_bytes.unwrap_or(max_frame_length as usize * 4);
        if max_pending_bytes < max_frame_length as usize + 4 {
            return Err(UnixListenerConfigError::new(
                "PiServer maxPendingBytes must be a safe integer at least maxFrameLength + 4",
            ));
        }
        let graceful_close_timeout_ms = options
            .graceful_close_timeout_ms
            .unwrap_or(DEFAULT_GRACEFUL_CLOSE_TIMEOUT_MS);
        if graceful_close_timeout_ms == 0 || graceful_close_timeout_ms > MAX_TIMER_DELAY_MS {
            return Err(UnixListenerConfigError::new(
                "PiServer gracefulCloseTimeoutMs must be an integer between 1 and 2147483647",
            ));
        }
        Ok(Self {
            path: options.path.clone(),
            mode,
            graceful_close_timeout_ms,
            max_pending_bytes,
            on_error: options.on_error.clone(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileIdentity {
    dev: u64,
    ino: u64,
}

#[derive(Default)]
struct ListenerState {
    started: bool,
    closing: bool,
    accept_task: Option<JoinHandle<()>>,
    socket_identity: Option<FileIdentity>,
    owned_bind_path: Option<PathBuf>,
    bound_path: Option<String>,
}

pub struct UnixListener {
    options: ResolvedUnixListenerOptions,
    state: Mutex<ListenerState>,
    accept: Mutex<Option<ByteConnectionAcceptor>>,
    connections: Arc<Mutex<BTreeMap<u64, Arc<UnixByteConnection>>>>,
    next_connection: AtomicU64,
    closing: Arc<AtomicBool>,
    close_lock: tokio::sync::Mutex<()>,
    closed: AtomicBool,
}

impl UnixListener {
    fn new(options: ResolvedUnixListenerOptions) -> Self {
        Self {
            options,
            state: Mutex::new(ListenerState::default()),
            accept: Mutex::new(None),
            connections: Arc::new(Mutex::new(BTreeMap::new())),
            next_connection: AtomicU64::new(1),
            closing: Arc::new(AtomicBool::new(false)),
            close_lock: tokio::sync::Mutex::new(()),
            closed: AtomicBool::new(false),
        }
    }

    fn report_error(&self, error: &(dyn Error + Send + Sync + 'static)) {
        let Some(observer) = &self.options.on_error else {
            return;
        };
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| observer(error)));
    }

    async fn start_impl(&self, accept: ByteConnectionAcceptor) -> ListenerResult {
        {
            let state = self.state.lock().expect("listener state mutex poisoned");
            if state.started {
                return Err(Box::new(std::io::Error::other("Unix listener is already started")));
            }
            if state.closing {
                return Err(Box::new(std::io::Error::other("Unix listener is closing or closed")));
            }
        }
        *self.accept.lock().expect("accept mutex poisoned") = Some(accept.clone());
        let owned_bind_path = get_owned_bind_path(&self.options.path);
        let result = self.bind(accept, &owned_bind_path).await;
        if let Err(error) = result {
            self.cleanup_owned_socket();
            if let Err(cleanup_error) = remove_path(&owned_bind_path) {
                self.report_error(cleanup_error.as_ref());
            }
            return Err(error);
        }
        Ok(())
    }

    async fn bind(&self, accept: ByteConnectionAcceptor, owned_bind_path: &Path) -> ListenerResult {
        validate_unix_socket_path(&owned_bind_path.to_string_lossy(), "PiServer private Unix bind path")
            .map_err(|error| Box::new(error) as Box<dyn Error + Send + Sync>)?;
        create_parent_directory(&self.options.path)?;
        remove_stale_socket(Path::new(&self.options.path)).await?;
        remove_stale_socket(owned_bind_path).await?;
        let listener = TokioUnixListener::bind(owned_bind_path)?;
        let metadata = std::fs::symlink_metadata(owned_bind_path)?;
        if !is_socket(&metadata) {
            return Err(Box::new(std::io::Error::other(format!(
                "Unix listener path is not a socket after binding: {}",
                owned_bind_path.display()
            ))));
        }
        let identity = FileIdentity {
            dev: metadata.dev(),
            ino: metadata.ino(),
        };
        {
            let mut state = self.state.lock().expect("listener state mutex poisoned");
            state.socket_identity = Some(identity);
            state.owned_bind_path = Some(owned_bind_path.to_path_buf());
        }
        std::fs::hard_link(owned_bind_path, &self.options.path)?;
        set_socket_mode(&self.options.path, self.options.mode)?;

        let task = tokio::spawn(accept_loop(
            listener,
            AcceptLoopOptions {
                connections: self.connections.clone(),
                closing: self.closing.clone(),
                first_connection_id: self.next_connection.fetch_add(1, Ordering::SeqCst),
                accept,
                graceful_close_timeout_ms: self.options.graceful_close_timeout_ms,
                max_pending_bytes: self.options.max_pending_bytes,
                on_error: self.options.on_error.clone(),
            },
        ));
        let mut state = self.state.lock().expect("listener state mutex poisoned");
        state.started = true;
        state.bound_path = Some(self.options.path.clone());
        state.accept_task = Some(task);
        Ok(())
    }

    async fn close_impl(&self) -> ListenerResult {
        let (accept_task, owned_bind_path) = {
            let mut state = self.state.lock().expect("listener state mutex poisoned");
            state.closing = true;
            state.bound_path = None;
            (state.accept_task.take(), state.owned_bind_path.take())
        };
        self.closing.store(true, Ordering::SeqCst);
        if let Some(task) = accept_task {
            task.abort();
        }
        tokio::task::yield_now().await;
        let connections: Vec<Arc<UnixByteConnection>> = self
            .connections
            .lock()
            .expect("connection map mutex poisoned")
            .values()
            .cloned()
            .collect();
        for connection in connections {
            if let Err(error) = connection.close(None).await {
                self.report_error(error.as_ref());
            }
        }
        self.cleanup_owned_socket();
        if let Some(owned_bind_path) = owned_bind_path
            && let Err(error) = remove_path(&owned_bind_path)
        {
            self.report_error(error.as_ref());
        }
        self.connections.lock().expect("connection map mutex poisoned").clear();
        Ok(())
    }

    fn cleanup_owned_socket(&self) {
        let identity = {
            let mut state = self.state.lock().expect("listener state mutex poisoned");
            state.socket_identity.take()
        };
        let Some(identity) = identity else {
            return;
        };
        if let Err(error) = cleanup_owned_socket_at(Path::new(&self.options.path), identity) {
            self.report_error(error.as_ref());
        }
    }
}

impl PiServerListener for UnixListener {
    fn address(&self) -> Option<String> {
        self.state
            .lock()
            .expect("listener state mutex poisoned")
            .bound_path
            .clone()
    }

    fn start(&self, accept: ByteConnectionAcceptor) -> BoxFuture<'_, ListenerResult> {
        Box::pin(async move { self.start_impl(accept).await })
    }

    fn close(&self) -> BoxFuture<'_, ListenerResult> {
        Box::pin(async move {
            if self.closed.load(Ordering::SeqCst) {
                return Ok(());
            }
            let _guard = self.close_lock.lock().await;
            if self.closed.load(Ordering::SeqCst) {
                return Ok(());
            }
            let result = self.close_impl().await;
            self.closed.store(true, Ordering::SeqCst);
            result
        })
    }
}

pub fn create_unix_listener(
    options: UnixListenerOptions,
) -> Result<Arc<dyn PiServerListener>, UnixListenerConfigError> {
    let options = ResolvedUnixListenerOptions::resolve(&options)?;
    Ok(Arc::new(UnixListener::new(options)))
}

struct AcceptLoopOptions {
    connections: Arc<Mutex<BTreeMap<u64, Arc<UnixByteConnection>>>>,
    closing: Arc<AtomicBool>,
    first_connection_id: u64,
    accept: ByteConnectionAcceptor,
    graceful_close_timeout_ms: u64,
    max_pending_bytes: usize,
    on_error: Option<ErrorObserver>,
}

async fn accept_loop(listener: TokioUnixListener, options: AcceptLoopOptions) {
    let mut next_id = options.first_connection_id;
    loop {
        let (stream, _) = match listener.accept().await {
            Ok(accepted) => accepted,
            Err(error) => {
                if options.closing.load(Ordering::SeqCst) {
                    return;
                }
                if let Some(observer) = &options.on_error {
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| observer(&error)));
                }
                continue;
            }
        };
        if options.closing.load(Ordering::SeqCst) {
            drop(stream);
            continue;
        }
        let (connection, reader) =
            UnixByteConnection::new(stream, options.graceful_close_timeout_ms, options.max_pending_bytes);
        let id = next_id;
        next_id += 1;
        options
            .connections
            .lock()
            .expect("connection map mutex poisoned")
            .insert(id, connection.clone());
        let handler = (options.accept)(connection.clone() as Arc<dyn ByteConnection>);
        let task_connections = options.connections.clone();
        tokio::spawn(async move {
            read_connection(reader, connection, handler, task_connections, id).await;
        });
    }
}

async fn read_connection(
    mut reader: OwnedReadHalf,
    connection: Arc<UnixByteConnection>,
    mut handler: Box<dyn ByteConnectionHandler>,
    connections: Arc<Mutex<BTreeMap<u64, Arc<UnixByteConnection>>>>,
    id: u64,
) {
    let mut buffer = [0_u8; 8 * 1024];
    loop {
        match reader.read(&mut buffer).await {
            Ok(0) => {
                connection.finish_read().await;
                break;
            }
            Ok(bytes) => handler.on_data(&buffer[..bytes]),
            Err(error) => {
                handler.on_error(error);
                connection.finish_read().await;
                break;
            }
        }
    }
    connection.mark_closed();
    connections.lock().expect("connection map mutex poisoned").remove(&id);
    handler.on_close();
}

/// One Unix-domain stream with ordered writes and bounded pending bytes.
pub struct UnixByteConnection {
    commands: mpsc::UnboundedSender<WriteCommand>,
    closed: AtomicBool,
    closing: AtomicBool,
    pending_bytes: AtomicUsize,
    max_pending_bytes: usize,
    graceful_close_timeout_ms: u64,
    close_lock: tokio::sync::Mutex<()>,
    closed_tx: watch::Sender<bool>,
}

enum WriteCommand {
    Chunk {
        bytes: Vec<u8>,
        result: oneshot::Sender<ConnectionResult>,
    },
    Shutdown {
        result: oneshot::Sender<ConnectionResult>,
    },
    Close {
        final_chunk: Option<Vec<u8>>,
        result: oneshot::Sender<ConnectionResult>,
    },
}

impl UnixByteConnection {
    pub fn new(
        stream: UnixStream,
        graceful_close_timeout_ms: u64,
        max_pending_bytes: usize,
    ) -> (Arc<Self>, OwnedReadHalf) {
        let (reader, writer) = stream.into_split();
        let (closed_tx, _) = watch::channel(false);
        let (commands, receiver) = mpsc::unbounded_channel();
        let connection = Arc::new(Self {
            commands,
            closed: AtomicBool::new(false),
            closing: AtomicBool::new(false),
            pending_bytes: AtomicUsize::new(0),
            max_pending_bytes,
            graceful_close_timeout_ms,
            close_lock: tokio::sync::Mutex::new(()),
            closed_tx,
        });
        let weak = Arc::downgrade(&connection);
        tokio::spawn(write_loop(writer, receiver, weak));
        (connection, reader)
    }

    pub fn mark_closed(&self) {
        if self.closed.swap(true, Ordering::SeqCst) {
            return;
        }
        self.closing.store(true, Ordering::SeqCst);
        let _ = self.closed_tx.send(true);
    }

    /// Ends the write side after the peer stops reading, matching half-close defaults.
    async fn finish_read(&self) {
        let (result_tx, result_rx) = oneshot::channel();
        if self
            .commands
            .send(WriteCommand::Shutdown { result: result_tx })
            .is_err()
        {
            return;
        }
        let _ = result_rx.await;
    }

    /// Framed bytes accepted by `send` that have not completed writing yet.
    pub fn pending_bytes(&self) -> usize {
        self.pending_bytes.load(Ordering::SeqCst)
    }

    fn reserve(&self, size: usize) -> Result<(), Box<dyn Error + Send + Sync>> {
        if self.closed.load(Ordering::SeqCst) || self.closing.load(Ordering::SeqCst) {
            return Err(Box::new(std::io::Error::other("Unix connection is closed")));
        }
        let previous = self.pending_bytes.fetch_add(size, Ordering::SeqCst);
        if previous + size > self.max_pending_bytes {
            self.pending_bytes.fetch_sub(size, Ordering::SeqCst);
            return Err(Box::new(std::io::Error::other(
                "Unix connection exceeded its pending byte limit",
            )));
        }
        Ok(())
    }

    fn send_impl(&self, chunk: Vec<u8>) -> BoxFuture<'static, ConnectionResult> {
        let size = chunk.len();
        if let Err(error) = self.reserve(size) {
            return Box::pin(async move { Err(error) });
        }
        let (result_tx, result_rx) = oneshot::channel();
        if self
            .commands
            .send(WriteCommand::Chunk {
                bytes: chunk,
                result: result_tx,
            })
            .is_err()
        {
            self.pending_bytes.fetch_sub(size, Ordering::SeqCst);
            return Box::pin(async { Err(writer_stopped_error()) });
        }
        Box::pin(async move { result_rx.await.unwrap_or_else(|_| Err(writer_stopped_error())) })
    }

    async fn close_impl(&self, final_chunk: Option<Vec<u8>>) -> ConnectionResult {
        if self.closed.load(Ordering::SeqCst) {
            self.mark_closed();
            return Ok(());
        }
        let _guard = self.close_lock.lock().await;
        if self.closed.load(Ordering::SeqCst) {
            return Ok(());
        }
        self.closing.store(true, Ordering::SeqCst);
        let (result_tx, result_rx) = oneshot::channel();
        if self
            .commands
            .send(WriteCommand::Close {
                final_chunk,
                result: result_tx,
            })
            .is_err()
        {
            return Err(writer_stopped_error());
        }
        let write_result = result_rx.await.unwrap_or_else(|_| Err(writer_stopped_error()));
        if write_result.is_err() {
            self.mark_closed();
            return write_result;
        }
        let mut closed = self.closed_tx.subscribe();
        let graceful = Duration::from_millis(self.graceful_close_timeout_ms);
        let _ = tokio::time::timeout(graceful, async {
            while !*closed.borrow() && closed.changed().await.is_ok() {}
        })
        .await;
        self.mark_closed();
        write_result
    }
}

fn writer_stopped_error() -> Box<dyn Error + Send + Sync> {
    Box::new(std::io::Error::other("Unix connection writer stopped"))
}

async fn write_loop(
    mut writer: OwnedWriteHalf,
    mut commands: mpsc::UnboundedReceiver<WriteCommand>,
    connection: Weak<UnixByteConnection>,
) {
    while let Some(command) = commands.recv().await {
        match command {
            WriteCommand::Chunk { bytes, result } => {
                let size = bytes.len();
                let outcome = writer
                    .write_all(&bytes)
                    .await
                    .map_err(|error| Box::new(error) as Box<dyn Error + Send + Sync>);
                if let Some(connection) = connection.upgrade() {
                    connection.pending_bytes.fetch_sub(size, Ordering::SeqCst);
                }
                let _ = result.send(outcome);
            }
            WriteCommand::Shutdown { result } => {
                let outcome = writer
                    .shutdown()
                    .await
                    .map_err(|error| Box::new(error) as Box<dyn Error + Send + Sync>);
                let _ = result.send(outcome);
            }
            WriteCommand::Close { final_chunk, result } => {
                let outcome = async {
                    if let Some(final_chunk) = final_chunk {
                        writer.write_all(&final_chunk).await?;
                    }
                    writer.shutdown().await?;
                    Ok(())
                }
                .await;
                let _ = result.send(outcome);
                return;
            }
        }
    }
}

impl ByteConnection for UnixByteConnection {
    fn closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    fn send(&self, chunk: Vec<u8>) -> BoxFuture<'static, ConnectionResult> {
        self.send_impl(chunk)
    }

    fn close(&self, final_chunk: Option<Vec<u8>>) -> BoxFuture<'_, ConnectionResult> {
        Box::pin(async move { self.close_impl(final_chunk).await })
    }
}

fn get_owned_bind_path(path: &str) -> PathBuf {
    let digest = Sha256::digest(path.as_bytes());
    let hex = format!("{digest:x}");
    let suffix = &hex[..8];
    let parent = Path::new(path).parent().unwrap_or_else(|| Path::new("."));
    parent.join(format!(".p-{suffix}"))
}

fn random_suffix(prefix: &str) -> String {
    let id = pi_ai::utils::uuid::random_uuid();
    format!("{prefix}{}", &id[..6])
}

async fn remove_stale_socket(path: &Path) -> ListenerResult {
    let original = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(Box::new(error)),
    };
    if !is_socket(&original) {
        return Err(Box::new(std::io::Error::other(format!(
            "Refusing to remove non-socket Unix listener path: {}",
            path.display()
        ))));
    }
    if is_socket_live(path).await? {
        return Err(Box::new(std::io::Error::other(format!(
            "Unix listener is already running: {}",
            path.display()
        ))));
    }

    let preserved = path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(random_suffix(".s-"));
    match std::fs::rename(path, &preserved) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(Box::new(error)),
    }
    let current = std::fs::symlink_metadata(&preserved)?;
    if !is_socket(&current) || current.dev() != original.dev() || current.ino() != original.ino() {
        if !path.exists() {
            std::fs::rename(&preserved, path)?;
        }
        return Err(Box::new(std::io::Error::other(format!(
            "Unix listener path changed while checking for a stale socket: {}",
            path.display()
        ))));
    }
    remove_path(&preserved)
}

fn cleanup_owned_socket_at(path: &Path, identity: FileIdentity) -> ListenerResult {
    let current = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(Box::new(error)),
    };
    if !is_socket(&current) || current.dev() != identity.dev || current.ino() != identity.ino {
        return Ok(());
    }
    let preserved = path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(random_suffix(".c-"));
    match std::fs::rename(path, &preserved) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(Box::new(error)),
    }
    let moved = std::fs::symlink_metadata(&preserved)?;
    if is_socket(&moved) && moved.dev() == identity.dev && moved.ino() == identity.ino {
        remove_path(&preserved)?;
        return Ok(());
    }
    if !path.exists() {
        std::fs::rename(&preserved, path)?;
    }
    Err(Box::new(std::io::Error::other(format!(
        "Unix listener path changed during cleanup; preserved replacement at {}",
        preserved.display()
    ))))
}

fn remove_path(path: &Path) -> ListenerResult {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(Box::new(error)),
    }
}

async fn is_socket_live(path: &Path) -> Result<bool, Box<dyn Error + Send + Sync>> {
    let probe = tokio::time::timeout(
        Duration::from_millis(SOCKET_PROBE_TIMEOUT_MS),
        UnixStream::connect(path),
    )
    .await;
    match probe {
        Err(_elapsed) => Ok(true),
        Ok(Ok(_stream)) => Ok(true),
        Ok(Err(error)) => match error.kind() {
            ErrorKind::ConnectionRefused | ErrorKind::NotFound | ErrorKind::BrokenPipe | ErrorKind::ConnectionReset => {
                Ok(false)
            }
            _ => match error.raw_os_error() {
                Some(111 | 2 | 32 | 104) => Ok(false),
                _ => Err(Box::new(error)),
            },
        },
    }
}

fn create_parent_directory(path: &str) -> ListenerResult {
    let parent = Path::new(path)
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(parent)?;
    Ok(())
}

fn set_socket_mode(path: &str, mode: u32) -> ListenerResult {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))?;
    Ok(())
}

fn is_socket(metadata: &std::fs::Metadata) -> bool {
    metadata.file_type().is_socket()
}
