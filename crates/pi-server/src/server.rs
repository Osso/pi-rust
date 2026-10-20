use std::collections::BTreeMap;
use std::error::Error;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::connection::{
    ByteConnection, ByteConnectionAcceptor, ByteConnectionHandler, ConnectionStage, ConnectionState,
};
use crate::errors::{
    INTERNAL_SERVER_ERROR_MESSAGE, InternalServerError, NOT_IMPLEMENTED_MESSAGE, PiServerError,
    PiServerOperationErrorCode,
};
use crate::listener::{ListenerError, ListenerResult, PiServerListener};
use crate::sessions::{LiveSessionManager, unwrap_shared_error};
use crate::snapshots::ServerSnapshotPublisher;
use crate::types::{BoxFuture, PiServerService, handshake_signal};
use pi_protocol::{
    ClientHello, ClientMessage, DEFAULT_MAX_FRAME_LENGTH, EventEnvelope, PROTOCOL_VERSION, ProtocolError,
    ProtocolErrorCode, ProtocolValidationError, RequestEnvelope, ServerEvent, ServerHello, ServerHelloError,
    ServerMessage, is_supported_protocol_version,
};

const DEFAULT_HANDSHAKE_TIMEOUT_MS: u64 = 5_000;
const MAX_TIMER_DELAY_MS: u64 = 2_147_483_647;

pub type ErrorObserver = Arc<dyn Fn(&(dyn Error + Send + Sync + 'static)) + Send + Sync>;

/// Options used to construct a [`PiServer`].
#[derive(Clone, Default)]
pub struct PiServerOptions {
    pub listeners: Vec<Arc<dyn PiServerListener>>,
    pub max_frame_length: Option<u32>,
    pub handshake_timeout_ms: Option<u64>,
    pub server_id: Option<String>,
    pub on_error: Option<ErrorObserver>,
}

/// Returned when server options violate their typed constraints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PiServerConfigError {
    EmptyServerId,
    InvalidMaxFrameLength,
    InvalidHandshakeTimeoutMs,
}

impl std::fmt::Display for PiServerConfigError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyServerId => formatter.write_str("PiServer serverId must not be empty"),
            Self::InvalidMaxFrameLength => {
                formatter.write_str("PiServer maxFrameLength must be an integer between 1 and 4294967295")
            }
            Self::InvalidHandshakeTimeoutMs => {
                formatter.write_str("PiServer handshakeTimeoutMs must be an integer between 1 and 2147483647")
            }
        }
    }
}

impl Error for PiServerConfigError {}

pub struct PiServer {
    core: Arc<ServerCore>,
    listeners: Vec<Arc<dyn PiServerListener>>,
}

pub(crate) struct ServerCore {
    pub id: String,
    max_frame_length: u32,
    handshake_timeout: Duration,
    on_error: Option<ErrorObserver>,
    closing: AtomicBool,
    closed: AtomicBool,
    started: AtomicBool,
    starting: AtomicBool,
    close_lock: tokio::sync::Mutex<()>,
    pub connections: Mutex<BTreeMap<String, Arc<ConnectionState>>>,
    pub service: Arc<dyn PiServerService>,
    pub sessions: Arc<LiveSessionManager>,
    pub snapshots: Arc<ServerSnapshotPublisher>,
}

impl PiServer {
    pub fn new(service: Arc<dyn PiServerService>, options: PiServerOptions) -> Result<Self, PiServerConfigError> {
        let server_id = options.server_id.unwrap_or_else(pi_ai::utils::uuid::random_uuid);
        if server_id.is_empty() {
            return Err(PiServerConfigError::EmptyServerId);
        }
        let max_frame_length = options.max_frame_length.unwrap_or(DEFAULT_MAX_FRAME_LENGTH);
        if max_frame_length == 0 {
            return Err(PiServerConfigError::InvalidMaxFrameLength);
        }
        let handshake_timeout_ms = options.handshake_timeout_ms.unwrap_or(DEFAULT_HANDSHAKE_TIMEOUT_MS);
        if handshake_timeout_ms == 0 || handshake_timeout_ms > MAX_TIMER_DELAY_MS {
            return Err(PiServerConfigError::InvalidHandshakeTimeoutMs);
        }
        let core = Arc::new_cyclic(|weak| ServerCore {
            id: server_id,
            max_frame_length,
            handshake_timeout: Duration::from_millis(handshake_timeout_ms),
            on_error: options.on_error,
            closing: AtomicBool::new(false),
            closed: AtomicBool::new(false),
            started: AtomicBool::new(false),
            starting: AtomicBool::new(false),
            close_lock: tokio::sync::Mutex::new(()),
            connections: Mutex::new(BTreeMap::new()),
            service: service.clone(),
            sessions: Arc::new(LiveSessionManager::new(weak.clone(), service)),
            snapshots: Arc::new(ServerSnapshotPublisher::new(weak.clone())),
        });
        Ok(Self {
            core,
            listeners: options.listeners,
        })
    }

    pub fn id(&self) -> &str {
        &self.core.id
    }

    pub fn addresses(&self) -> Vec<String> {
        self.listeners
            .iter()
            .filter_map(|listener| listener.address())
            .collect()
    }

    pub fn start(&self) -> BoxFuture<'_, ListenerResult> {
        if self.core.started.load(Ordering::SeqCst) {
            return Box::pin(async {
                Err(Box::new(std::io::Error::other("PiServer is already started")) as ListenerError)
            });
        }
        if self.core.starting.swap(true, Ordering::SeqCst) {
            return Box::pin(async {
                Err(Box::new(std::io::Error::other("PiServer is already starting")) as ListenerError)
            });
        }
        if self.core.closing.load(Ordering::SeqCst) {
            self.core.starting.store(false, Ordering::SeqCst);
            return Box::pin(async {
                Err(Box::new(std::io::Error::other("PiServer is closing or closed")) as ListenerError)
            });
        }
        Box::pin(self.start_internal())
    }

    async fn start_internal(&self) -> Result<(), ListenerError> {
        let core = self.core.clone();
        let accept: ByteConnectionAcceptor = Arc::new(move |connection| core.accept(connection));
        let mut started_listeners: Vec<Arc<dyn PiServerListener>> = Vec::new();
        for listener in &self.listeners {
            if let Err(error) = listener.start(accept.clone()).await {
                self.core.closing.store(true, Ordering::SeqCst);
                for started in &started_listeners {
                    let _ = started.close().await;
                }
                self.core.close_server_state().await;
                self.core.starting.store(false, Ordering::SeqCst);
                return Err(error);
            }
            started_listeners.push(listener.clone());
        }
        self.core.started.store(true, Ordering::SeqCst);
        self.core.starting.store(false, Ordering::SeqCst);
        Ok(())
    }

    pub fn accept(&self, connection: Arc<dyn ByteConnection>) -> Box<dyn ByteConnectionHandler> {
        self.core.accept(connection)
    }

    pub async fn close(&self) -> Result<(), ListenerError> {
        if self.core.closed.load(Ordering::SeqCst) {
            return Ok(());
        }
        self.core.closing.store(true, Ordering::SeqCst);
        let _guard = self.core.close_lock.lock().await;
        if self.core.closed.load(Ordering::SeqCst) {
            return Ok(());
        }
        let mut first_error = None;
        for listener in &self.listeners {
            if let Err(error) = listener.close().await {
                first_error.get_or_insert(error);
            }
        }
        self.core.close_server_state().await;
        self.core.started.store(false, Ordering::SeqCst);
        self.core.closed.store(true, Ordering::SeqCst);
        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

impl ServerCore {
    pub(crate) fn is_closing(&self) -> bool {
        self.closing.load(Ordering::SeqCst)
    }

    pub(crate) fn connection(&self, id: &str) -> Option<Arc<ConnectionState>> {
        self.connections
            .lock()
            .expect("connection map mutex poisoned")
            .get(id)
            .cloned()
    }

    pub(crate) fn report_error(&self, error: &(dyn Error + Send + Sync + 'static)) {
        let Some(observer) = &self.on_error else {
            return;
        };
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| observer(error)));
    }

    pub(crate) fn broadcast_server_snapshot(&self) {
        self.snapshots.broadcast();
    }

    pub(crate) fn accept(self: &Arc<Self>, connection: Arc<dyn ByteConnection>) -> Box<dyn ByteConnectionHandler> {
        if self.is_closing() {
            let core = self.clone();
            tokio::spawn(async move {
                core.close_connection(connection, None).await;
            });
            return Box::new(ClosedHandler { core: self.clone() });
        }
        let (handshake_tx, _) = handshake_signal();
        let state = Arc::new(ConnectionState::new(
            pi_ai::utils::uuid::random_uuid(),
            connection,
            Some(self.max_frame_length),
            handshake_tx,
        ));
        self.connections
            .lock()
            .expect("connection map mutex poisoned")
            .insert(state.id.clone(), state.clone());
        let core = self.clone();
        let timeout_state = state.clone();
        let timeout = tokio::spawn(async move {
            tokio::time::sleep(core.handshake_timeout).await;
            if matches!(
                timeout_state.stage(),
                ConnectionStage::AwaitingHello | ConnectionStage::Handshaking
            ) && !timeout_state.disconnected.load(Ordering::SeqCst)
            {
                core.fail_protocol(
                    timeout_state,
                    ProtocolError::new(ProtocolErrorCode::InvalidRequest, "Handshake timeout".into(), None),
                )
                .await;
            }
        });
        state.set_handshake_timeout(timeout);
        Box::new(ServerConnectionHandler {
            core: self.clone(),
            state,
        })
    }

    fn receive(self: &Arc<Self>, state: &Arc<ConnectionState>, chunk: &[u8]) {
        if state.is_terminal() {
            return;
        }
        let messages = match state.decoder.lock().expect("decoder mutex poisoned").push(chunk) {
            Ok(messages) => messages,
            Err(error) => {
                let protocol_error = self.to_protocol_error(&error);
                let core = self.clone();
                let state = state.clone();
                tokio::spawn(async move { core.fail_protocol(state, protocol_error).await });
                return;
            }
        };
        for message in messages {
            if state.is_terminal() {
                return;
            }
            self.dispatch_message(state.clone(), message);
        }
    }

    fn dispatch_message(self: &Arc<Self>, state: Arc<ConnectionState>, message: ClientMessage) {
        let stage = state.stage();
        if stage == ConnectionStage::AwaitingHello {
            match message {
                ClientMessage::Hello(hello) => {
                    state.set_stage(ConnectionStage::Handshaking);
                    let core = self.clone();
                    tokio::spawn(async move {
                        core.finish_handshake(state.clone(), hello).await;
                        state.signal_handshake_finished();
                    });
                }
                ClientMessage::Request(_) => {
                    self.fail_with_message(state, "The first client message must be hello");
                }
            }
            return;
        }

        if matches!(message, ClientMessage::Hello(_)) {
            self.fail_with_message(state, "hello may only be sent as the first message");
            return;
        }

        let ClientMessage::Request(envelope) = message else {
            return;
        };
        if stage == ConnectionStage::Ready {
            let core = self.clone();
            tokio::spawn(async move { core.handle_request(state, envelope).await });
            return;
        }
        if stage != ConnectionStage::Handshaking {
            return;
        }
        let core = self.clone();
        tokio::spawn(async move {
            let mut receiver = state.handshake_receiver();
            if receiver.wait_for(|finished| *finished).await.is_err() {
                return;
            }
            if state.stage() == ConnectionStage::Ready && !state.disconnected.load(Ordering::SeqCst) {
                core.handle_request(state, envelope).await;
            }
        });
    }

    fn fail_with_message(self: &Arc<Self>, state: Arc<ConnectionState>, message: &str) {
        let error = ProtocolError::new(ProtocolErrorCode::InvalidRequest, message.into(), None);
        let core = self.clone();
        tokio::spawn(async move { core.fail_protocol(state, error).await });
    }

    async fn finish_handshake(self: &Arc<Self>, state: Arc<ConnectionState>, hello: ClientHello) {
        if !is_supported_protocol_version(hello.version()) {
            self.fail_protocol(
                state,
                ProtocolError::new(
                    ProtocolErrorCode::Version,
                    format!(
                        "Unsupported protocol version {}; expected {PROTOCOL_VERSION}",
                        hello.version()
                    ),
                    None,
                ),
            )
            .await;
            return;
        }

        let snapshot = match self.snapshots.get(None).await {
            Ok(snapshot) => snapshot,
            Err(error) => {
                let protocol_error = self.to_protocol_error(error.as_ref());
                self.fail_protocol(state, protocol_error).await;
                return;
            }
        };
        if self.is_closing()
            || state.disconnected.load(Ordering::SeqCst)
            || state.stage() != ConnectionStage::Handshaking
            || state.connection.closed()
        {
            return;
        }
        let hello = ServerHello::new(state.id.clone(), snapshot.clone())
            .expect("server hello fields are validated by the snapshot publisher");
        let sent = self.send_message(state.clone(), ServerMessage::Hello(hello)).await;
        if sent && !state.disconnected.load(Ordering::SeqCst) && state.stage() == ConnectionStage::Handshaking {
            state.handshake_complete.store(true, Ordering::SeqCst);
            state.set_stage(ConnectionStage::Ready);
            state.clear_handshake_timeout();
            self.send_stale_snapshot(state, snapshot.revision()).await;
        }
    }

    async fn send_stale_snapshot(self: &Arc<Self>, state: Arc<ConnectionState>, revision: u64) {
        if revision == self.snapshots.current_revision() {
            return;
        }
        match self.snapshots.get(None).await {
            Ok(current) => {
                self.send_message(
                    state,
                    ServerMessage::Event(EventEnvelope::new(ServerEvent::ServerSnapshot(current))),
                )
                .await;
            }
            Err(error) => {
                let protocol_error = self.to_protocol_error(error.as_ref());
                self.fail_protocol(state, protocol_error).await;
            }
        }
    }

    async fn handle_request(self: &Arc<Self>, state: Arc<ConnectionState>, envelope: RequestEnvelope) {
        let response = match self.sessions.execute_command(state.clone(), envelope.request()).await {
            Ok(result) => pi_protocol::ResponseEnvelope::Success(
                pi_protocol::SuccessResponse::new(envelope.id().to_string(), result)
                    .expect("request ids are non-empty"),
            ),
            Err(error) => pi_protocol::ResponseEnvelope::Error(
                pi_protocol::ErrorResponse::new(envelope.id().to_string(), self.to_protocol_error(error.as_ref()))
                    .expect("request ids are non-empty"),
            ),
        };
        self.send_message(state, ServerMessage::Response(response)).await;
    }

    async fn transport_closed(self: &Arc<Self>, connection: Arc<ConnectionState>) {
        if !connection.disconnected.load(Ordering::SeqCst)
            && connection.stage() != ConnectionStage::Closing
            && let Err(error) = connection.decoder.lock().expect("decoder mutex poisoned").end()
        {
            self.report_error(&error);
        }
        self.disconnect(connection).await;
    }

    pub(crate) fn disconnect(self: &Arc<Self>, connection: Arc<ConnectionState>) -> BoxFuture<'static, ()> {
        let core = self.clone();
        Box::pin(async move {
            if connection.disconnected.swap(true, Ordering::SeqCst) {
                return;
            }
            let handshake_complete = connection.handshake_complete.load(Ordering::SeqCst);
            connection.set_stage(ConnectionStage::Closed);
            connection.clear_handshake_timeout();
            core.connections
                .lock()
                .expect("connection map mutex poisoned")
                .remove(&connection.id);
            core.sessions.disconnect(connection).await.ok();
            if !core.is_closing() && handshake_complete {
                core.broadcast_server_snapshot();
            }
        })
    }

    pub(crate) fn send_message(
        self: &Arc<Self>,
        connection: Arc<ConnectionState>,
        message: ServerMessage,
    ) -> BoxFuture<'static, bool> {
        let core = self.clone();
        Box::pin(async move {
            if connection.disconnected.load(Ordering::SeqCst) || connection.connection.closed() {
                return false;
            }
            let frame = match pi_protocol::encode_server_message(&message, Some(core.max_frame_length)) {
                Ok(frame) => frame,
                Err(error) => {
                    core.report_error(&error);
                    core.close_connection(connection.connection.clone(), None).await;
                    core.disconnect(connection).await;
                    return false;
                }
            };
            match connection.connection.send(frame).await {
                Ok(()) => true,
                Err(error) => {
                    core.report_error(error.as_ref());
                    core.close_connection(connection.connection.clone(), None).await;
                    core.disconnect(connection).await;
                    false
                }
            }
        })
    }

    pub(crate) fn send_message_detached(self: &Arc<Self>, connection: Arc<ConnectionState>, message: ServerMessage) {
        if connection.disconnected.load(Ordering::SeqCst) || connection.connection.closed() {
            return;
        }
        let frame = match pi_protocol::encode_server_message(&message, Some(self.max_frame_length)) {
            Ok(frame) => frame,
            Err(error) => {
                self.report_error(&error);
                let core = self.clone();
                tokio::spawn(async move {
                    core.close_connection(connection.connection.clone(), None).await;
                    core.disconnect(connection).await;
                });
                return;
            }
        };
        let write = connection.connection.send(frame);
        let core = self.clone();
        tokio::spawn(async move {
            if let Err(error) = write.await {
                core.report_error(error.as_ref());
                core.close_connection(connection.connection.clone(), None).await;
                core.disconnect(connection).await;
            }
        });
    }

    pub(crate) fn close_connection(
        self: &Arc<Self>,
        connection: Arc<dyn ByteConnection>,
        final_chunk: Option<Vec<u8>>,
    ) -> BoxFuture<'static, ()> {
        let core = self.clone();
        Box::pin(async move {
            if let Err(error) = connection.close(final_chunk).await {
                core.report_error(error.as_ref());
            }
        })
    }

    pub(crate) async fn fail_protocol(self: &Arc<Self>, connection: Arc<ConnectionState>, error: ProtocolError) {
        if connection.disconnected.load(Ordering::SeqCst) || connection.stage().is_terminal() {
            return;
        }
        connection.set_stage(ConnectionStage::Closing);
        connection.clear_handshake_timeout();
        let message = ServerMessage::HelloError(ServerHelloError::new(error));
        let final_frame = match pi_protocol::encode_server_message(&message, Some(self.max_frame_length)) {
            Ok(frame) => Some(frame),
            Err(encode_error) => {
                self.report_error(&encode_error);
                None
            }
        };
        self.close_connection(connection.connection.clone(), final_frame).await;
        self.disconnect(connection).await;
    }

    pub(crate) async fn close_server_state(self: &Arc<Self>) {
        let connections: Vec<Arc<ConnectionState>> = self
            .connections
            .lock()
            .expect("connection map mutex poisoned")
            .values()
            .cloned()
            .collect();
        for connection in &connections {
            connection.set_stage(ConnectionStage::Closing);
            connection.clear_handshake_timeout();
        }
        for connection in &connections {
            self.close_connection(connection.connection.clone(), None).await;
        }
        for connection in connections {
            self.disconnect(connection).await;
        }
        self.sessions.close().await.ok();
        self.connections.lock().expect("connection map mutex poisoned").clear();
    }

    fn to_protocol_error(&self, error: &(dyn Error + Send + Sync + 'static)) -> ProtocolError {
        let error = unwrap_shared_error(error);
        if let Some(internal) = error.downcast_ref::<InternalServerError>() {
            self.report_error(internal.cause());
            return ProtocolError::new(
                ProtocolErrorCode::InternalError,
                INTERNAL_SERVER_ERROR_MESSAGE.into(),
                None,
            );
        }
        if let Some(server_error) = error.downcast_ref::<PiServerError>() {
            if server_error.code() == PiServerOperationErrorCode::NotImplemented {
                return ProtocolError::new(ProtocolErrorCode::NotImplemented, NOT_IMPLEMENTED_MESSAGE.into(), None);
            }
            return ProtocolError::new(
                server_error.code().protocol_code(),
                server_error.message().into(),
                server_error.details().cloned(),
            );
        }
        if let Some(validation) = error.downcast_ref::<ProtocolValidationError>() {
            return ProtocolError::new(ProtocolErrorCode::InvalidRequest, validation.to_string(), None);
        }
        self.report_error(error);
        ProtocolError::new(
            ProtocolErrorCode::InternalError,
            INTERNAL_SERVER_ERROR_MESSAGE.into(),
            None,
        )
    }
}

struct ServerConnectionHandler {
    core: Arc<ServerCore>,
    state: Arc<ConnectionState>,
}

impl ByteConnectionHandler for ServerConnectionHandler {
    fn on_data(&mut self, chunk: &[u8]) {
        self.core.receive(&self.state, chunk);
    }

    fn on_close(&mut self) {
        let core = self.core.clone();
        let state = self.state.clone();
        tokio::spawn(async move { core.transport_closed(state).await });
    }

    fn on_error(&mut self, error: std::io::Error) {
        self.core.report_error(&error);
        let core = self.core.clone();
        let state = self.state.clone();
        tokio::spawn(async move {
            core.close_connection(state.connection.clone(), None).await;
            core.disconnect(state).await;
        });
    }
}

struct ClosedHandler {
    core: Arc<ServerCore>,
}

impl ByteConnectionHandler for ClosedHandler {
    fn on_data(&mut self, _chunk: &[u8]) {}

    fn on_close(&mut self) {}

    fn on_error(&mut self, error: std::io::Error) {
        self.core.report_error(&error);
    }
}
