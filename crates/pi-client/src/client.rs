use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::Pin;
use std::sync::{Arc, Mutex, Weak};
use std::task::{Context, Poll};

use pi_protocol::{
    Command, CommandResult, ProtocolValidationError, RequestEnvelope, ResponseEnvelope, ServerEvent, ServerMessage,
    ServerSnapshot, SessionSnapshot, encode_client_message,
};
use tokio::sync::Notify;

use crate::connection::{Connection, ConnectionObserver};
use crate::errors::{
    PiClientDisposedError, PiDisconnectedError, PiError, PiServerError, PiSessionDetachedError, PiSessionOwnershipError,
};
use crate::promise::{Promise, Resolver, create_promise_resolvers};
use crate::session_handle::{
    AcquireSessionOptions, ReleaseFuture, SessionHandle, SessionHandleCallbacks, SessionLeaseMode,
};
use crate::state::ClientState;
use crate::types::{
    ConnectionState, ConnectionStateChange, ConnectionStateListener, CreateSessionOptions, EventListener,
    ListenerErrorHandler, PiClientOptions, SessionSnapshotListener, SnapshotListener, Unsubscribe,
};

/// A command response awaiting its correlated server message.
pub struct CommandResponse {
    promise: Promise<CommandResult>,
}

impl Future for CommandResponse {
    type Output = Result<CommandResult, PiError>;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.promise).poll(context)
    }
}

/// Completion of a client connection attempt.
pub struct ConnectFuture {
    promise: Promise<ServerSnapshot>,
}

impl Future for ConnectFuture {
    type Output = Result<ServerSnapshot, PiError>;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.promise).poll(context)
    }
}

/// Completion of a client disposal.
pub struct DisposeFuture {
    ready: bool,
}

impl Future for DisposeFuture {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Self::Output> {
        if self.ready {
            return Poll::Ready(());
        }
        self.ready = true;
        Poll::Ready(())
    }
}

/// Completion of a session acquisition.
pub struct AttachFuture {
    inner: Pin<Box<dyn Future<Output = Result<SessionHandle, PiError>> + Send>>,
}

impl Future for AttachFuture {
    type Output = Result<SessionHandle, PiError>;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        self.inner.as_mut().poll(context)
    }
}

/// Completion of session creation.
pub struct CreateSessionFuture {
    inner: Pin<Box<dyn Future<Output = Result<SessionHandle, PiError>> + Send>>,
}

impl Future for CreateSessionFuture {
    type Output = Result<SessionHandle, PiError>;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        self.inner.as_mut().poll(context)
    }
}

struct PendingRequest {
    command: Command,
    resolver: Resolver<CommandResult>,
}

struct LeaseToken;

pub(crate) struct SharedOperation {
    notify: Notify,
    result: Mutex<Option<Result<(), PiError>>>,
}

impl SharedOperation {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            notify: Notify::new(),
            result: Mutex::new(None),
        })
    }

    fn complete(&self, result: Result<(), PiError>) {
        *self.result.lock().unwrap() = Some(result);
        self.notify.notify_waiters();
    }

    pub(crate) async fn wait(&self) -> Result<(), PiError> {
        loop {
            let notified = self.notify.notified();
            if let Some(result) = self.result.lock().unwrap().clone() {
                return result;
            }
            notified.await;
        }
    }
}

struct AttachmentGate {
    notify: Notify,
    result: Mutex<Option<Result<(), PiError>>>,
}

impl AttachmentGate {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            notify: Notify::new(),
            result: Mutex::new(None),
        })
    }

    fn complete(&self, result: Result<(), PiError>) {
        *self.result.lock().unwrap() = Some(result);
        self.notify.notify_waiters();
    }

    async fn wait(&self) -> Result<(), PiError> {
        loop {
            let notified = self.notify.notified();
            if let Some(result) = self.result.lock().unwrap().clone() {
                return result;
            }
            notified.await;
        }
    }
}

#[derive(Default)]
struct Runtime {
    pending: HashMap<String, PendingRequest>,
    lease_counts: HashMap<String, usize>,
    exclusive_leases: HashMap<String, Arc<LeaseToken>>,
    lease_generations: HashMap<String, u64>,
    attachments: HashMap<String, Arc<AttachmentGate>>,
    detachments: HashMap<String, Arc<SharedOperation>>,
    cleanup_required: HashSet<String>,
    reconciliations: HashMap<String, Arc<SharedOperation>>,
    connection_listeners: Vec<(u64, ConnectionStateListener)>,
    next_listener_id: u64,
    request_sequence: u64,
    disposed: bool,
}

struct ClientInner {
    options: PiClientOptions,
    connection: Connection,
    state: ClientState,
    runtime: Mutex<Runtime>,
}

/// A protocol client that owns one connection and its session leases.
#[derive(Clone)]
pub struct PiClient {
    inner: Arc<ClientInner>,
}

impl std::fmt::Debug for PiClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PiClient")
            .field("connection_state", &self.connection_state())
            .field("disposed", &self.disposed())
            .finish()
    }
}

impl PiClient {
    pub fn new(options: PiClientOptions) -> Result<Self, PiError> {
        let max_frame_length = options
            .max_frame_length
            .unwrap_or(pi_protocol::DEFAULT_MAX_FRAME_LENGTH);
        if max_frame_length == 0 {
            return Err(PiError::InvalidArgument(format!(
                "PiClient maxFrameLength must be an integer between 1 and {}",
                u32::MAX
            )));
        }
        let state = ClientState::new(options.on_listener_error.clone());
        let inner = Arc::new_cyclic(|weak| {
            let observer = Arc::new(ClientObserver { inner: weak.clone() });
            let connection = Connection::new(options.transport_factory.clone(), max_frame_length, observer);
            ClientInner {
                options,
                connection,
                state,
                runtime: Mutex::new(Runtime::default()),
            }
        });
        Ok(Self { inner })
    }

    pub async fn connect_new(options: PiClientOptions) -> Result<Self, PiError> {
        let client = Self::new(options)?;
        match client.connect()?.await {
            Ok(_) => Ok(client),
            Err(error) => {
                client.dispose().await;
                Err(error)
            }
        }
    }

    pub fn disposed(&self) -> bool {
        self.inner.runtime.lock().unwrap().disposed
    }

    pub fn connection_state(&self) -> ConnectionState {
        self.inner.connection.state()
    }

    pub fn connected(&self) -> bool {
        self.connection_state() == ConnectionState::Connected
    }

    pub fn snapshot(&self) -> Option<ServerSnapshot> {
        self.inner.state.snapshot()
    }

    pub fn connect(&self) -> Result<ConnectFuture, PiError> {
        self.assert_not_disposed()?;
        if self.connection_state() == ConnectionState::Disconnected {
            self.inner.state.reset();
        }
        self.inner.connection.connect().map(|promise| ConnectFuture { promise })
    }

    pub fn reconnect(&self) -> Result<ConnectFuture, PiError> {
        self.connect()
    }

    pub fn disconnect(&self) {
        self.disconnect_with_reason("Client disconnected");
    }

    pub fn disconnect_with_reason(&self, reason: impl Into<String>) {
        self.inner
            .connection
            .disconnect(PiError::Disconnected(PiDisconnectedError::new(reason)));
    }

    pub fn subscribe(&self, listener: SnapshotListener) -> Result<Unsubscribe, PiError> {
        self.assert_not_disposed()?;
        Ok(self.inner.state.subscribe(listener))
    }

    pub fn on_event(&self, listener: EventListener) -> Result<Unsubscribe, PiError> {
        self.assert_not_disposed()?;
        Ok(self.inner.state.on_event(listener))
    }

    pub fn on_connection_state_change(&self, listener: ConnectionStateListener) -> Result<Unsubscribe, PiError> {
        self.assert_not_disposed()?;
        let id = {
            let mut runtime = self.inner.runtime.lock().unwrap();
            runtime.next_listener_id += 1;
            let id = runtime.next_listener_id;
            runtime.connection_listeners.push((id, listener));
            id
        };
        let weak = Arc::downgrade(&self.inner);
        Ok(Box::new(move || {
            let Some(inner) = weak.upgrade() else {
                return;
            };
            inner
                .runtime
                .lock()
                .unwrap()
                .connection_listeners
                .retain(|(entry_id, _)| *entry_id != id);
        }))
    }

    pub fn list_sessions(&self) -> Result<CommandResponse, PiError> {
        self.start_request(Command::List(pi_protocol::ListCommand::new()))
    }

    pub fn create_session(&self, options: CreateSessionOptions) -> Result<CreateSessionFuture, PiError> {
        let command = pi_protocol::CreateCommand::new(pi_protocol::CreateCommandInput {
            cwd: options.cwd,
            name: options.name,
            model: options.model,
            thinking_level: options.thinking_level,
        })
        .map_err(|error| PiError::InvalidArgument(error.to_string()))?;
        let response = self.start_request(Command::Create(command))?;
        let client = self.clone();
        Ok(CreateSessionFuture {
            inner: Box::pin(async move {
                let result = response.await?;
                let snapshot = match result {
                    CommandResult::Create(snapshot) => snapshot,
                    other => {
                        return Err(PiError::Protocol(ProtocolValidationError::new(format!(
                            "Response command {} does not match create",
                            other.as_str()
                        ))));
                    }
                };
                let token = client.reserve_session_lease(snapshot.id(), SessionLeaseMode::Exclusive)?;
                Ok(client.create_session_handle(snapshot.id().to_string(), token))
            }),
        })
    }

    pub fn attach_session(&self, session_id: impl Into<String>) -> Result<AttachFuture, PiError> {
        self.acquire_session(session_id, SessionLeaseMode::Shared)
    }

    pub fn acquire_session(
        &self,
        session_id: impl Into<String>,
        options: impl Into<AcquireSessionOptions>,
    ) -> Result<AttachFuture, PiError> {
        self.assert_not_disposed()?;
        let session_id = session_id.into();
        let mode = options.into().mode;
        let token = self.reserve_session_lease(&session_id, mode)?;
        let start = match self.begin_attach(&session_id) {
            Ok(start) => start,
            Err(error) => {
                self.release_session_lease(&session_id, &token);
                return Err(error);
            }
        };
        let client = self.clone();
        Ok(AttachFuture {
            inner: Box::pin(async move {
                let result = client.complete_attach(&session_id, token.clone(), start).await;
                if result.is_err() {
                    client.release_session_lease(&session_id, &token);
                }
                result
            }),
        })
    }

    pub fn dispose(&self) -> DisposeFuture {
        let already_disposed = {
            let mut runtime = self.inner.runtime.lock().unwrap();
            let disposed = runtime.disposed;
            runtime.disposed = true;
            disposed
        };
        if !already_disposed {
            self.reject_pending_requests(PiError::Disposed(PiClientDisposedError));
            self.inner
                .connection
                .disconnect(PiError::Disposed(PiClientDisposedError));
            self.inner.state.dispose();
            self.invalidate_all_session_leases();
            self.inner.runtime.lock().unwrap().connection_listeners.clear();
        }
        DisposeFuture { ready: true }
    }

    fn start_request(&self, command: Command) -> Result<CommandResponse, PiError> {
        self.assert_not_disposed()?;
        if !self.connected() {
            return Err(PiError::Disconnected(PiDisconnectedError::default()));
        }
        let id = {
            let mut runtime = self.inner.runtime.lock().unwrap();
            runtime.request_sequence += 1;
            format!("request-{}", runtime.request_sequence)
        };
        let (resolver, promise) = create_promise_resolvers();
        self.inner.runtime.lock().unwrap().pending.insert(
            id.clone(),
            PendingRequest {
                command: command.clone(),
                resolver,
            },
        );
        let envelope = RequestEnvelope::new(id.clone(), command).map_err(|error| {
            self.take_pending_request(&id);
            PiError::InvalidArgument(error.to_string())
        })?;
        let frame = match encode_client_message(
            &pi_protocol::ClientMessage::Request(envelope),
            Some(self.inner.connection.max_frame_length()),
        ) {
            Ok(frame) => frame,
            Err(error) => {
                self.take_pending_request(&id);
                return Err(PiError::Protocol(error));
            }
        };
        if let Err(error) = self.inner.connection.send(frame) {
            self.take_pending_request(&id);
            return Err(error);
        }
        Ok(CommandResponse { promise })
    }

    fn take_pending_request(&self, id: &str) -> Option<PendingRequest> {
        self.inner.runtime.lock().unwrap().pending.remove(id)
    }

    fn reject_pending_requests(&self, error: PiError) {
        let pending: Vec<PendingRequest> = {
            let mut runtime = self.inner.runtime.lock().unwrap();
            runtime.pending.drain().map(|(_, request)| request).collect()
        };
        for request in pending {
            request.resolver.reject(error.clone());
        }
    }

    fn assert_not_disposed(&self) -> Result<(), PiError> {
        if self.disposed() {
            return Err(PiError::Disposed(PiClientDisposedError));
        }
        Ok(())
    }

    fn reserve_session_lease(&self, session_id: &str, mode: SessionLeaseMode) -> Result<Arc<LeaseToken>, PiError> {
        let mut runtime = self.inner.runtime.lock().unwrap();
        let count = runtime.lease_counts.get(session_id).copied().unwrap_or(0);
        if mode == SessionLeaseMode::Exclusive && count > 0 {
            return Err(PiError::SessionOwnership(PiSessionOwnershipError::new(
                session_id,
                format!("Session {session_id} already has an active lease"),
            )));
        }
        if mode == SessionLeaseMode::Shared && runtime.exclusive_leases.contains_key(session_id) {
            return Err(PiError::SessionOwnership(PiSessionOwnershipError::new(
                session_id,
                format!("Session {session_id} has an exclusive lease"),
            )));
        }
        let token = Arc::new(LeaseToken);
        runtime.lease_counts.insert(session_id.to_string(), count + 1);
        if mode == SessionLeaseMode::Exclusive {
            runtime.exclusive_leases.insert(session_id.to_string(), token.clone());
        }
        Ok(token)
    }

    fn release_session_lease(&self, session_id: &str, token: &Arc<LeaseToken>) {
        let mut runtime = self.inner.runtime.lock().unwrap();
        let count = runtime.lease_counts.get(session_id).copied().unwrap_or(0);
        if count <= 1 {
            runtime.lease_counts.remove(session_id);
        } else {
            runtime.lease_counts.insert(session_id.to_string(), count - 1);
        }
        if runtime
            .exclusive_leases
            .get(session_id)
            .is_some_and(|current| Arc::ptr_eq(current, token))
        {
            runtime.exclusive_leases.remove(session_id);
        }
    }

    fn lease_count(&self, session_id: &str) -> usize {
        self.inner
            .runtime
            .lock()
            .unwrap()
            .lease_counts
            .get(session_id)
            .copied()
            .unwrap_or(0)
    }

    fn lease_generation(&self, session_id: &str) -> u64 {
        self.inner
            .runtime
            .lock()
            .unwrap()
            .lease_generations
            .get(session_id)
            .copied()
            .unwrap_or(0)
    }

    fn invalidate_session_leases(&self, session_id: &str) {
        let mut runtime = self.inner.runtime.lock().unwrap();
        runtime.lease_counts.remove(session_id);
        runtime.exclusive_leases.remove(session_id);
        runtime.cleanup_required.remove(session_id);
        let generation = runtime.lease_generations.get(session_id).copied().unwrap_or(0);
        runtime.lease_generations.insert(session_id.to_string(), generation + 1);
    }

    fn invalidate_all_session_leases(&self) {
        let session_ids: Vec<String> = {
            let runtime = self.inner.runtime.lock().unwrap();
            runtime.lease_counts.keys().cloned().collect()
        };
        for session_id in session_ids {
            self.invalidate_session_leases(&session_id);
        }
        self.inner.runtime.lock().unwrap().cleanup_required.clear();
    }

    fn create_session_handle(&self, session_id: String, token: Arc<LeaseToken>) -> SessionHandle {
        let generation = {
            let mut runtime = self.inner.runtime.lock().unwrap();
            let generation = runtime.lease_generations.get(&session_id).copied().unwrap_or(0);
            runtime.lease_generations.insert(session_id.clone(), generation);
            generation
        };
        let callbacks = Arc::new_cyclic(|weak| LeaseCallbacks {
            client: self.clone(),
            session_id: session_id.clone(),
            token,
            generation,
            self_ref: weak.clone(),
            state: Mutex::new(LeaseState::Active),
            release: Mutex::new(None),
        });
        SessionHandle::new(session_id, callbacks)
    }

    fn begin_attach(&self, session_id: &str) -> Result<AttachStart, PiError> {
        enum Plan {
            AlreadyAttached,
            WaitDetachment(Arc<SharedOperation>),
            WaitGate(Arc<AttachmentGate>),
            WaitReconcile(Arc<SharedOperation>),
            StartReconcile(Arc<SharedOperation>),
            Send(Arc<AttachmentGate>, Option<Box<SessionSnapshot>>),
        }

        let plan = {
            let mut runtime = self.inner.runtime.lock().unwrap();
            if let Some(detachment) = runtime.detachments.get(session_id) {
                Plan::WaitDetachment(detachment.clone())
            } else if runtime.cleanup_required.contains(session_id) {
                match runtime.reconciliations.get(session_id) {
                    Some(reconciliation) => Plan::WaitReconcile(reconciliation.clone()),
                    None => {
                        let reconciliation = SharedOperation::new();
                        runtime
                            .reconciliations
                            .insert(session_id.to_string(), reconciliation.clone());
                        Plan::StartReconcile(reconciliation)
                    }
                }
            } else if self.inner.state.is_session_attached(session_id) {
                Plan::AlreadyAttached
            } else if let Some(gate) = runtime.attachments.get(session_id) {
                Plan::WaitGate(gate.clone())
            } else {
                let gate = AttachmentGate::new();
                runtime.attachments.insert(session_id.to_string(), gate.clone());
                let previous = self.inner.state.forget_session_snapshot(session_id).map(Box::new);
                Plan::Send(gate, previous)
            }
        };

        match plan {
            Plan::AlreadyAttached => Ok(AttachStart::AlreadyAttached),
            Plan::WaitDetachment(operation) => Ok(AttachStart::WaitDetachment(operation)),
            Plan::WaitGate(gate) => Ok(AttachStart::WaitGate(gate)),
            Plan::WaitReconcile(operation) => Ok(AttachStart::WaitReconcile(operation)),
            Plan::StartReconcile(operation) => {
                let response = match self.start_request(Command::Detach(
                    pi_protocol::DetachCommand::new(session_id.to_string())
                        .map_err(|error| PiError::InvalidArgument(error.to_string()))?,
                )) {
                    Ok(response) => response,
                    Err(error) => {
                        self.remove_reconciliation_if(session_id, &operation);
                        operation.complete(Err(error.clone()));
                        return Err(error);
                    }
                };
                let client = self.clone();
                let id = session_id.to_string();
                let spawned_operation = operation.clone();
                tokio::spawn(async move {
                    let result = response.await.map(|_| ());
                    if result.is_ok() {
                        client.inner.runtime.lock().unwrap().cleanup_required.remove(&id);
                    }
                    client.remove_reconciliation_if(&id, &spawned_operation);
                    spawned_operation.complete(result);
                });
                Ok(AttachStart::WaitReconcile(operation))
            }
            Plan::Send(gate, previous) => {
                let response = match self.start_request(Command::Attach(
                    pi_protocol::AttachCommand::new(session_id.to_string())
                        .map_err(|error| PiError::InvalidArgument(error.to_string()))?,
                )) {
                    Ok(response) => response,
                    Err(error) => {
                        if let Some(previous) = previous {
                            self.inner.state.restore_session_snapshot(*previous);
                        }
                        self.remove_attachment_if(session_id, &gate);
                        return Err(error);
                    }
                };
                Ok(AttachStart::Send {
                    response,
                    gate,
                    previous,
                })
            }
        }
    }

    async fn complete_attach(
        &self,
        session_id: &str,
        token: Arc<LeaseToken>,
        mut start: AttachStart,
    ) -> Result<SessionHandle, PiError> {
        loop {
            match start {
                AttachStart::AlreadyAttached => {
                    return Ok(self.create_session_handle(session_id.to_string(), token));
                }
                AttachStart::WaitDetachment(operation) => {
                    let _ = operation.wait().await;
                    start = self.begin_attach(session_id)?;
                }
                AttachStart::WaitGate(gate) => {
                    gate.wait().await?;
                    start = self.begin_attach(session_id)?;
                }
                AttachStart::WaitReconcile(operation) => {
                    operation.wait().await?;
                    start = self.begin_attach(session_id)?;
                }
                AttachStart::Send {
                    response,
                    gate,
                    previous,
                } => {
                    let result = response.await;
                    self.remove_attachment_if(session_id, &gate);
                    match result {
                        Ok(CommandResult::Attach(_)) => {
                            gate.complete(Ok(()));
                        }
                        Ok(other) => {
                            if let Some(previous) = previous {
                                self.inner.state.restore_session_snapshot(*previous);
                            }
                            let error = PiError::Protocol(ProtocolValidationError::new(format!(
                                "Response command {} does not match attach",
                                other.as_str()
                            )));
                            gate.complete(Err(error.clone()));
                            return Err(error);
                        }
                        Err(error) => {
                            if let Some(previous) = previous {
                                self.inner.state.restore_session_snapshot(*previous);
                            }
                            gate.complete(Err(error.clone()));
                            return Err(error);
                        }
                    }
                    start = self.begin_attach(session_id)?;
                }
            }
        }
    }

    fn remove_attachment_if(&self, session_id: &str, gate: &Arc<AttachmentGate>) {
        let mut runtime = self.inner.runtime.lock().unwrap();
        if runtime
            .attachments
            .get(session_id)
            .is_some_and(|current| Arc::ptr_eq(current, gate))
        {
            runtime.attachments.remove(session_id);
        }
    }

    fn remove_reconciliation_if(&self, session_id: &str, operation: &Arc<SharedOperation>) {
        let mut runtime = self.inner.runtime.lock().unwrap();
        if runtime
            .reconciliations
            .get(session_id)
            .is_some_and(|current| Arc::ptr_eq(current, operation))
        {
            runtime.reconciliations.remove(session_id);
        }
    }

    fn remove_detachment_if(&self, session_id: &str, operation: &Arc<SharedOperation>) {
        let mut runtime = self.inner.runtime.lock().unwrap();
        if runtime
            .detachments
            .get(session_id)
            .is_some_and(|current| Arc::ptr_eq(current, operation))
        {
            runtime.detachments.remove(session_id);
        }
    }

    fn insert_detachment(&self, session_id: &str, operation: &Arc<SharedOperation>) {
        self.inner
            .runtime
            .lock()
            .unwrap()
            .detachments
            .insert(session_id.to_string(), operation.clone());
    }

    fn mark_cleanup_required(&self, session_id: &str) {
        self.inner
            .runtime
            .lock()
            .unwrap()
            .cleanup_required
            .insert(session_id.to_string());
    }

    fn handle_message(&self, message: &ServerMessage) {
        match message {
            ServerMessage::Event(envelope) => {
                let event = envelope.event();
                if let ServerEvent::SessionRemoved(removed) = event {
                    self.invalidate_session_leases(removed.session_id());
                }
                self.inner.state.apply_event(event);
            }
            ServerMessage::Response(ResponseEnvelope::Success(response)) => {
                let Some(pending) = self.take_pending_request(response.id()) else {
                    self.inner
                        .connection
                        .fail(PiError::Protocol(ProtocolValidationError::new(
                            "Response has no matching request",
                        )));
                    return;
                };
                let result = response.result();
                if result.as_str() != pending.command.as_str() {
                    let error = PiError::Protocol(ProtocolValidationError::new(format!(
                        "Response command {} does not match {}",
                        result.as_str(),
                        pending.command.as_str()
                    )));
                    pending.resolver.reject(error.clone());
                    self.inner.connection.fail(error);
                    return;
                }
                self.inner.state.apply_result(result);
                pending.resolver.resolve(result.clone());
            }
            ServerMessage::Response(ResponseEnvelope::Error(response)) => {
                let Some(pending) = self.take_pending_request(response.id()) else {
                    self.inner
                        .connection
                        .fail(PiError::Protocol(ProtocolValidationError::new(
                            "Response has no matching request",
                        )));
                    return;
                };
                pending
                    .resolver
                    .reject(PiError::Server(PiServerError::new(response.error().clone())));
            }
            ServerMessage::Hello(_) | ServerMessage::HelloError(_) => {}
        }
    }

    fn handle_connection_state_change(&self, change: &ConnectionStateChange) {
        if change.state == ConnectionState::Disconnected {
            self.inner.state.clear_attachments();
            self.invalidate_all_session_leases();
            let error = change
                .error
                .clone()
                .unwrap_or_else(|| PiError::Disconnected(PiDisconnectedError::default()));
            self.reject_pending_requests(error);
        }
        self.notify_connection_listeners(change);
    }

    fn notify_connection_listeners(&self, change: &ConnectionStateChange) {
        let listeners: Vec<ConnectionStateListener> = {
            let runtime = self.inner.runtime.lock().unwrap();
            runtime
                .connection_listeners
                .iter()
                .map(|(_, listener)| listener.clone())
                .collect()
        };
        for listener in listeners {
            let result = catch_unwind(AssertUnwindSafe(|| listener(change)));
            if let Err(payload) = result {
                report_listener_error(self.inner.options.on_listener_error.as_ref(), payload);
            }
        }
    }

    fn session_snapshot(&self, session_id: &str) -> Option<SessionSnapshot> {
        self.inner.state.get_session_snapshot(session_id)
    }

    fn session_is_attached(&self, session_id: &str) -> bool {
        self.inner.state.is_session_attached(session_id)
    }
}

struct ClientObserver {
    inner: Weak<ClientInner>,
}

impl ClientObserver {
    fn client(&self) -> Option<PiClient> {
        self.inner.upgrade().map(|inner| PiClient { inner })
    }
}

impl ConnectionObserver for ClientObserver {
    fn on_handshake(&self, snapshot: &ServerSnapshot) {
        if let Some(client) = self.client() {
            client.inner.state.apply_server_snapshot(snapshot);
        }
    }

    fn on_message(&self, message: &ServerMessage) {
        if let Some(client) = self.client() {
            client.handle_message(message);
        }
    }

    fn on_state_change(&self, change: &ConnectionStateChange) {
        if let Some(client) = self.client() {
            client.handle_connection_state_change(change);
        }
    }
}

enum AttachStart {
    AlreadyAttached,
    WaitDetachment(Arc<SharedOperation>),
    WaitGate(Arc<AttachmentGate>),
    WaitReconcile(Arc<SharedOperation>),
    Send {
        response: CommandResponse,
        gate: Arc<AttachmentGate>,
        previous: Option<Box<SessionSnapshot>>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LeaseState {
    Active,
    Releasing,
    Released,
    Invalidated,
}

struct LeaseCallbacks {
    client: PiClient,
    session_id: String,
    token: Arc<LeaseToken>,
    generation: u64,
    self_ref: Weak<LeaseCallbacks>,
    state: Mutex<LeaseState>,
    release: Mutex<Option<Arc<SharedOperation>>>,
}

impl LeaseCallbacks {
    fn refresh(&self) {
        let mut state = self.state.lock().unwrap();
        if matches!(*state, LeaseState::Active | LeaseState::Releasing)
            && self.client.lease_generation(&self.session_id) != self.generation
        {
            *state = LeaseState::Invalidated;
        }
    }

    fn is_active(&self) -> bool {
        self.refresh();
        let state = *self.state.lock().unwrap();
        state == LeaseState::Active && self.client.session_is_attached(&self.session_id)
    }

    fn assert_active(&self) -> Result<(), PiError> {
        self.client.assert_not_disposed()?;
        if !self.client.connected() {
            return Err(PiError::Disconnected(PiDisconnectedError::default()));
        }
        if !self.is_active() {
            return Err(PiError::SessionDetached(PiSessionDetachedError::new(
                self.session_id.clone(),
            )));
        }
        Ok(())
    }

    fn release(&self, relinquish_on_failure: bool) -> Result<ReleaseFuture, PiError> {
        self.refresh();
        let current = *self.state.lock().unwrap();
        if matches!(current, LeaseState::Released | LeaseState::Invalidated) {
            return Ok(ReleaseFuture::ready(Ok(())));
        }
        if let Some(operation) = self.release.lock().unwrap().clone() {
            return Ok(ReleaseFuture::new(operation));
        }
        self.assert_active()?;
        *self.state.lock().unwrap() = LeaseState::Releasing;
        let operation = SharedOperation::new();
        *self.release.lock().unwrap() = Some(operation.clone());
        if self.client.lease_count(&self.session_id) <= 1 {
            let response = match self.client.start_request(Command::Detach(
                pi_protocol::DetachCommand::new(self.session_id.clone())
                    .map_err(|error| PiError::InvalidArgument(error.to_string()))?,
            )) {
                Ok(response) => response,
                Err(error) => {
                    *self.state.lock().unwrap() = LeaseState::Active;
                    *self.release.lock().unwrap() = None;
                    return Err(error);
                }
            };
            self.client.insert_detachment(&self.session_id, &operation);
            let callbacks = self
                .self_ref
                .upgrade()
                .expect("lease callbacks must outlive their release operation");
            let detachment = operation.clone();
            tokio::spawn(async move {
                let result = response.await.map(|_| ());
                callbacks
                    .client
                    .remove_detachment_if(&callbacks.session_id, &detachment);
                match result {
                    Ok(()) => {
                        callbacks
                            .client
                            .release_session_lease(&callbacks.session_id, &callbacks.token);
                        *callbacks.state.lock().unwrap() = LeaseState::Released;
                        detachment.complete(Ok(()));
                    }
                    Err(error) => {
                        callbacks.refresh();
                        if *callbacks.state.lock().unwrap() == LeaseState::Invalidated {
                            detachment.complete(Ok(()));
                            return;
                        }
                        if relinquish_on_failure {
                            callbacks
                                .client
                                .release_session_lease(&callbacks.session_id, &callbacks.token);
                            callbacks.client.mark_cleanup_required(&callbacks.session_id);
                            *callbacks.state.lock().unwrap() = LeaseState::Released;
                        } else {
                            *callbacks.state.lock().unwrap() = LeaseState::Active;
                            *callbacks.release.lock().unwrap() = None;
                        }
                        detachment.complete(Err(error));
                    }
                }
            });
            Ok(ReleaseFuture::new(operation))
        } else {
            self.client.release_session_lease(&self.session_id, &self.token);
            *self.state.lock().unwrap() = LeaseState::Released;
            operation.complete(Ok(()));
            Ok(ReleaseFuture::new(operation))
        }
    }
}

impl SessionHandleCallbacks for LeaseCallbacks {
    fn is_attached(&self) -> bool {
        self.is_active()
    }

    fn snapshot(&self) -> Option<SessionSnapshot> {
        if self.is_active() {
            self.client.session_snapshot(&self.session_id)
        } else {
            None
        }
    }

    fn subscribe(&self, listener: SessionSnapshotListener) -> Result<Unsubscribe, PiError> {
        self.assert_active()?;
        let weak = self.self_ref.clone();
        Ok(self.client.inner.state.subscribe_session(
            &self.session_id,
            Arc::new(move |snapshot| {
                if let Some(callbacks) = weak.upgrade()
                    && callbacks.is_active()
                {
                    listener(snapshot);
                }
            }),
        ))
    }

    fn on_event(&self, listener: EventListener) -> Result<Unsubscribe, PiError> {
        self.assert_active()?;
        let weak = self.self_ref.clone();
        Ok(self.client.inner.state.on_session_event(
            &self.session_id,
            Arc::new(move |event| {
                if let Some(callbacks) = weak.upgrade()
                    && (callbacks.is_active() || event.as_str() == "session_removed")
                {
                    listener(event);
                }
            }),
        ))
    }

    fn detach(&self) -> Result<ReleaseFuture, PiError> {
        self.release(false)
    }

    fn dispose(&self) -> Result<ReleaseFuture, PiError> {
        self.release(true)
    }

    fn request(&self, command: Command) -> Result<CommandResponse, PiError> {
        self.assert_active()?;
        self.client.start_request(command)
    }
}

fn report_listener_error(on_listener_error: Option<&ListenerErrorHandler>, payload: Box<dyn Any + Send>) {
    let Some(handler) = on_listener_error else {
        return;
    };
    let message = if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "listener panicked".to_string()
    };
    let _ = catch_unwind(AssertUnwindSafe(|| handler(PiError::Listener(message))));
}
