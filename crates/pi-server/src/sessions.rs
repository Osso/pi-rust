use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Weak};

use pi_protocol::{
    Command, CommandResult, DetachResult, EventEnvelope, ListResult, ServerEvent, ServerMessage, SessionMetadata,
    SessionPhase, SessionProgress, SessionSnapshot,
};

use crate::connection::{ConnectionStage, ConnectionState};
use crate::errors::PiServerError;
use crate::server::ServerCore;
use crate::types::{
    BoxFuture, CreateSessionOptions, PiServerService, PiSessionRuntime, PiSessionRuntimeEvent, PromptInput,
    RuntimeSubscription, ServiceError, ServiceResult, SteerInput,
};

struct BroadcastRequest {
    result: tokio::sync::oneshot::Sender<ServiceResult<SessionSnapshot>>,
}

pub(crate) struct LiveSession {
    pub id: String,
    pub runtime: Arc<dyn PiSessionRuntime>,
    pub connections: Mutex<BTreeSet<String>>,
    subscription: Mutex<Option<RuntimeSubscription>>,
    broadcasts: tokio::sync::mpsc::UnboundedSender<BroadcastRequest>,
    operation_count: AtomicUsize,
    ready: AtomicBool,
    terminal: AtomicBool,
    disposing: AtomicBool,
    dispose_lock: tokio::sync::Mutex<()>,
}

impl LiveSession {
    fn new(
        id: String,
        runtime: Arc<dyn PiSessionRuntime>,
        broadcasts: tokio::sync::mpsc::UnboundedSender<BroadcastRequest>,
    ) -> Self {
        Self {
            id,
            runtime,
            connections: Mutex::new(BTreeSet::new()),
            subscription: Mutex::new(None),
            broadcasts,
            operation_count: AtomicUsize::new(0),
            ready: AtomicBool::new(false),
            terminal: AtomicBool::new(false),
            disposing: AtomicBool::new(false),
            dispose_lock: tokio::sync::Mutex::new(()),
        }
    }

    fn is_terminal(&self) -> bool {
        self.terminal.load(Ordering::SeqCst)
    }

    fn is_disposing(&self) -> bool {
        self.disposing.load(Ordering::SeqCst)
    }

    fn connection_count(&self) -> usize {
        self.connections.lock().expect("live connection mutex poisoned").len()
    }
}

pub(crate) struct LiveSessionManager {
    core: Weak<ServerCore>,
    service: Arc<dyn PiServerService>,
    live_sessions: Mutex<Vec<(String, Arc<LiveSession>)>>,
    acquire_lock: tokio::sync::Mutex<()>,
}

impl LiveSessionManager {
    pub(crate) fn new(core: Weak<ServerCore>, service: Arc<dyn PiServerService>) -> Self {
        Self {
            core,
            service,
            live_sessions: Mutex::new(Vec::new()),
            acquire_lock: tokio::sync::Mutex::new(()),
        }
    }

    pub(crate) fn execute_command(
        self: &Arc<Self>,
        connection: Arc<ConnectionState>,
        command: &Command,
    ) -> BoxFuture<'static, ServiceResult<CommandResult>> {
        let manager = self.clone();
        let command = command.clone();
        Box::pin(async move { manager.execute(connection, command).await })
    }

    async fn execute(
        self: &Arc<Self>,
        connection: Arc<ConnectionState>,
        command: Command,
    ) -> ServiceResult<CommandResult> {
        match command {
            Command::List(_) => Ok(CommandResult::List(ListResult::new(self.list_metadata().await?))),
            Command::Create(command) => {
                let id = pi_ai::utils::uuid::random_uuid();
                let options = CreateSessionOptions {
                    id: id.clone(),
                    cwd: command.cwd().map(str::to_string),
                    name: command.name().map(str::to_string),
                    model: command.model().cloned(),
                    thinking_level: command.thinking_level(),
                };
                let service = self.service.clone();
                let runtime = Box::pin(async move { service.create_session(options).await });
                let snapshot = self.acquire_and_attach(&connection, id, runtime).await?;
                Ok(CommandResult::Create(for_connection(&snapshot, &connection)))
            }
            Command::Attach(command) => {
                let session_id = command.session_id().to_string();
                let service = self.service.clone();
                let requested = session_id.clone();
                let runtime = Box::pin(async move { service.open_session(requested).await });
                let snapshot = self.acquire_and_attach(&connection, session_id, runtime).await?;
                Ok(CommandResult::Attach(for_connection(&snapshot, &connection)))
            }
            Command::Detach(command) => self.detach(&connection, command.session_id()).await,
            Command::Prompt(command) => {
                let live = self.require_attached(&connection, command.session_id())?;
                let runtime = live.runtime.clone();
                let operation = Box::pin(async move { runtime.prompt(PromptInput::new(command.text())).await });
                let snapshot = self.run_operation(&live, operation).await?;
                Ok(CommandResult::Prompt(for_connection(&snapshot, &connection)))
            }
            Command::Steer(command) => {
                let live = self.require_attached(&connection, command.session_id())?;
                let runtime = live.runtime.clone();
                let operation = Box::pin(async move { runtime.steer(SteerInput::new(command.text())).await });
                let snapshot = self.run_operation(&live, operation).await?;
                Ok(CommandResult::Steer(for_connection(&snapshot, &connection)))
            }
            Command::Abort(command) => {
                let live = self.require_attached(&connection, command.session_id())?;
                let runtime = live.runtime.clone();
                let operation = Box::pin(async move { runtime.abort().await });
                let snapshot = self.run_operation(&live, operation).await?;
                Ok(CommandResult::Abort(for_connection(&snapshot, &connection)))
            }
            Command::SetModel(command) => {
                let live = self.require_attached(&connection, command.session_id())?;
                let runtime = live.runtime.clone();
                let model = command.model().clone();
                let operation = Box::pin(async move { runtime.set_model(model).await });
                let snapshot = self.run_operation(&live, operation).await?;
                Ok(CommandResult::SetModel(for_connection(&snapshot, &connection)))
            }
            Command::SetThinking(command) => {
                let live = self.require_attached(&connection, command.session_id())?;
                let runtime = live.runtime.clone();
                let thinking_level = command.thinking_level();
                let operation = Box::pin(async move { runtime.set_thinking(thinking_level).await });
                let snapshot = self.run_operation(&live, operation).await?;
                Ok(CommandResult::SetThinking(for_connection(&snapshot, &connection)))
            }
        }
    }

    async fn acquire_and_attach(
        self: &Arc<Self>,
        connection: &Arc<ConnectionState>,
        session_id: String,
        runtime: BoxFuture<'static, ServiceResult<Arc<dyn PiSessionRuntime>>>,
    ) -> ServiceResult<SessionSnapshot> {
        let live = self.acquire(session_id, runtime).await?;
        self.attach(connection, &live).await?;
        let snapshot = self.broadcast_snapshot(&live).await?;
        self.broadcast_server_snapshot();
        Ok(snapshot)
    }

    async fn detach(
        self: &Arc<Self>,
        connection: &Arc<ConnectionState>,
        session_id: &str,
    ) -> ServiceResult<CommandResult> {
        let attached = connection
            .session_ids
            .lock()
            .expect("session id mutex poisoned")
            .remove(session_id);
        if attached {
            let live = self
                .live_sessions
                .lock()
                .expect("live session mutex poisoned")
                .iter()
                .find(|(id, _)| id == session_id)
                .map(|(_, live)| live.clone());
            if let Some(live) = live {
                live.connections
                    .lock()
                    .expect("live connection mutex poisoned")
                    .remove(&connection.id);
                if live.connection_count() > 0 && !live.is_terminal() && !live.is_disposing() {
                    self.broadcast_snapshot(&live).await?;
                }
                self.maybe_dispose(live).await?;
            }
            self.broadcast_server_snapshot();
        }
        Ok(CommandResult::Detach(
            DetachResult::new(session_id.to_string()).map_err(|error| Box::new(error) as ServiceError)?,
        ))
    }

    async fn run_operation(
        self: &Arc<Self>,
        live: &Arc<LiveSession>,
        operation: BoxFuture<'static, ServiceResult<()>>,
    ) -> ServiceResult<SessionSnapshot> {
        live.operation_count.fetch_add(1, Ordering::SeqCst);
        let result = operation.await;
        let snapshot = match result {
            Ok(()) => self.broadcast_snapshot(live).await,
            Err(error) => Err(error),
        };
        live.operation_count.fetch_sub(1, Ordering::SeqCst);
        self.schedule_maybe_dispose(live.clone());
        snapshot
    }

    async fn acquire(
        self: &Arc<Self>,
        id: String,
        acquire_runtime: BoxFuture<'static, ServiceResult<Arc<dyn PiSessionRuntime>>>,
    ) -> ServiceResult<Arc<LiveSession>> {
        let _guard = self.acquire_lock.lock().await;
        let mut acquire_runtime = Some(acquire_runtime);
        loop {
            let existing = self
                .live_sessions
                .lock()
                .expect("live session mutex poisoned")
                .iter()
                .find(|(session_id, _)| session_id == &id)
                .map(|(_, live)| live.clone());
            if let Some(existing) = existing {
                if existing.is_terminal() {
                    return Err(Box::new(PiServerError::session_locked(
                        format!("Session runtime is terminating: {id}"),
                        None,
                    )));
                }
                if existing.is_disposing() {
                    let _dispose_guard = existing.dispose_lock.lock().await;
                    continue;
                }
                return Ok(existing);
            }
            let runtime = acquire_runtime
                .take()
                .expect("runtime acquisition is consumed once")
                .await?;
            return self.create(id, runtime).await;
        }
    }

    async fn create(
        self: &Arc<Self>,
        id: String,
        runtime: Arc<dyn PiSessionRuntime>,
    ) -> ServiceResult<Arc<LiveSession>> {
        let Some(core) = self.core.upgrade() else {
            return Err(dropped_error());
        };
        if core.is_closing() {
            dispose_runtime(&core, runtime.as_ref()).await;
            return Err(Box::new(std::io::Error::other(
                "PiServer closed while acquiring a session runtime",
            )));
        }
        let snapshot = match runtime.snapshot().await {
            Ok(snapshot) => snapshot,
            Err(error) => {
                dispose_runtime(&core, runtime.as_ref()).await;
                return Err(error);
            }
        };
        if snapshot.id() != id {
            dispose_runtime(&core, runtime.as_ref()).await;
            return Err(Box::new(PiServerError::invalid_request(
                format!(
                    "Service returned session {} for server-assigned session {id}",
                    snapshot.id()
                ),
                None,
            )));
        }
        let (broadcasts, receiver) = tokio::sync::mpsc::unbounded_channel();
        let live = Arc::new(LiveSession::new(id.clone(), runtime.clone(), broadcasts));
        tokio::spawn(broadcast_loop(Arc::downgrade(self), Arc::downgrade(&live), receiver));
        let core_weak = self.core.clone();
        let live_weak = Arc::downgrade(&live);
        let subscription = runtime.subscribe(Arc::new(move |event| {
            let (Some(core), Some(live)) = (core_weak.upgrade(), live_weak.upgrade()) else {
                return;
            };
            core.sessions.handle_runtime_event(live, event);
        }));
        *live.subscription.lock().expect("subscription mutex poisoned") = Some(subscription);
        self.live_sessions
            .lock()
            .expect("live session mutex poisoned")
            .push((id, live.clone()));
        live.ready.store(true, Ordering::SeqCst);
        Ok(live)
    }

    pub(crate) fn handle_runtime_event(self: &Arc<Self>, live: Arc<LiveSession>, event: PiSessionRuntimeEvent) {
        match event {
            PiSessionRuntimeEvent::Error(error) => {
                let manager = self.clone();
                tokio::spawn(async move {
                    if let Err(error) = manager.terminate(live, error).await
                        && let Some(core) = manager.core.upgrade()
                    {
                        core.report_error(error.as_ref());
                    }
                });
            }
            PiSessionRuntimeEvent::Progress(progress) => {
                let Some(core) = self.core.upgrade() else {
                    return;
                };
                let Ok(progress) = SessionProgress::new(live.id.clone(), *progress) else {
                    return;
                };
                let envelope = EventEnvelope::new(ServerEvent::SessionProgress(progress));
                let connection_ids: Vec<String> = live
                    .connections
                    .lock()
                    .expect("live connection mutex poisoned")
                    .iter()
                    .cloned()
                    .collect();
                for connection_id in connection_ids {
                    if let Some(connection) = core.connection(&connection_id) {
                        core.send_message_detached(connection, ServerMessage::Event(envelope.clone()));
                    }
                }
                self.schedule_maybe_dispose(live);
            }
            PiSessionRuntimeEvent::Snapshot => {
                let manager = self.clone();
                let (result, completion) = tokio::sync::oneshot::channel();
                if live.broadcasts.send(BroadcastRequest { result }).is_err() {
                    return;
                }
                tokio::spawn(async move {
                    if let Ok(Err(error)) = completion.await
                        && let Some(core) = manager.core.upgrade()
                    {
                        core.report_error(error.as_ref());
                    }
                    manager.schedule_maybe_dispose(live);
                });
            }
        }
    }

    async fn terminate(&self, live: Arc<LiveSession>, error: PiServerError) -> ServiceResult<()> {
        if live.terminal.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        if let Some(core) = self.core.upgrade() {
            core.report_error(&error);
        }
        if let Some(subscription) = live.subscription.lock().expect("subscription mutex poisoned").take() {
            subscription.unsubscribe();
        }
        let connection_ids: Vec<String> = live
            .connections
            .lock()
            .expect("live connection mutex poisoned")
            .iter()
            .cloned()
            .collect();
        if let Some(core) = self.core.upgrade() {
            let mut connections = Vec::new();
            for connection_id in &connection_ids {
                if let Some(connection) = core.connection(connection_id) {
                    connections.push(connection);
                }
            }
            for connection in &connections {
                core.close_connection(connection.connection.clone(), None).await;
            }
            for connection in connections {
                core.disconnect(connection).await;
            }
        }
        self.maybe_dispose(live).await
    }

    async fn normalized_snapshot(&self, live: &Arc<LiveSession>) -> ServiceResult<SessionSnapshot> {
        let snapshot = live.runtime.snapshot().await?;
        if snapshot.id() != live.id {
            return Err(Box::new(PiServerError::invalid_request(
                format!("Runtime session ID changed from {} to {}", live.id, snapshot.id()),
                None,
            )));
        }
        Ok(rebuild_snapshot(
            &snapshot,
            Some(live.runtime.get_phase()),
            live.connection_count() > 0,
            true,
        ))
    }

    /// Enqueues a snapshot broadcast behind earlier broadcasts for this session, matching the
    /// TypeScript runtime event order.
    async fn broadcast_snapshot(&self, live: &Arc<LiveSession>) -> ServiceResult<SessionSnapshot> {
        let (result, completion) = tokio::sync::oneshot::channel();
        live.broadcasts
            .send(BroadcastRequest { result })
            .map_err(|_| Box::new(PiServerError::session_not_found("Session runtime is gone", None)) as ServiceError)?;
        completion.await.unwrap_or_else(|_| {
            Err(Box::new(PiServerError::session_not_found(
                "Session runtime is gone",
                None,
            )))
        })
    }

    async fn broadcast_snapshot_now(&self, live: &Arc<LiveSession>) -> ServiceResult<SessionSnapshot> {
        let snapshot = self.normalized_snapshot(live).await?;
        let envelope = EventEnvelope::new(ServerEvent::SessionSnapshot(snapshot.clone()));
        if let Some(core) = self.core.upgrade() {
            let connection_ids: Vec<String> = live
                .connections
                .lock()
                .expect("live connection mutex poisoned")
                .iter()
                .cloned()
                .collect();
            for connection_id in connection_ids {
                if let Some(connection) = core.connection(&connection_id) {
                    core.send_message_detached(connection, ServerMessage::Event(envelope.clone()));
                }
            }
        }
        Ok(snapshot)
    }

    async fn attach(&self, connection: &Arc<ConnectionState>, live: &Arc<LiveSession>) -> ServiceResult<()> {
        if connection.disconnected.load(Ordering::SeqCst)
            || connection.stage() != ConnectionStage::Ready
            || connection.connection.closed()
        {
            self.maybe_dispose(live.clone()).await?;
            return Err(Box::new(PiServerError::invalid_request(
                "Connection closed while attaching to a session",
                None,
            )));
        }
        connection
            .session_ids
            .lock()
            .expect("session id mutex poisoned")
            .insert(live.id.clone());
        live.connections
            .lock()
            .expect("live connection mutex poisoned")
            .insert(connection.id.clone());
        Ok(())
    }

    fn require_attached(&self, connection: &Arc<ConnectionState>, session_id: &str) -> ServiceResult<Arc<LiveSession>> {
        if !connection
            .session_ids
            .lock()
            .expect("session id mutex poisoned")
            .contains(session_id)
        {
            return Err(Box::new(PiServerError::invalid_request(
                format!("Connection is not attached to session {session_id}"),
                None,
            )));
        }
        let live = self
            .live_sessions
            .lock()
            .expect("live session mutex poisoned")
            .iter()
            .find(|(id, _)| id == session_id)
            .map(|(_, live)| live.clone());
        match live {
            Some(live) if !live.is_terminal() && !live.is_disposing() => Ok(live),
            _ => Err(Box::new(PiServerError::session_not_found(
                format!("Session is not live: {session_id}"),
                None,
            ))),
        }
    }

    fn schedule_maybe_dispose(self: &Arc<Self>, live: Arc<LiveSession>) {
        let manager = self.clone();
        tokio::spawn(async move {
            if let Err(error) = manager.maybe_dispose(live).await
                && let Some(core) = manager.core.upgrade()
            {
                core.report_error(error.as_ref());
            }
        });
    }

    async fn maybe_dispose(&self, live: Arc<LiveSession>) -> ServiceResult<()> {
        if !disposable(self, &live) {
            return Ok(());
        }
        let guard = live.dispose_lock.lock().await;
        if live.is_disposing() || !disposable(self, &live) {
            return Ok(());
        }
        live.disposing.store(true, Ordering::SeqCst);
        if let Some(subscription) = live.subscription.lock().expect("subscription mutex poisoned").take() {
            subscription.unsubscribe();
        }
        let result = live.runtime.dispose().await;
        self.live_sessions
            .lock()
            .expect("live session mutex poisoned")
            .retain(|(id, _)| id != &live.id);
        drop(guard);
        if let Some(core) = self.core.upgrade()
            && !core.is_closing()
        {
            self.broadcast_server_snapshot();
        }
        result
    }

    pub(crate) fn disconnect(
        self: &Arc<Self>,
        connection: Arc<ConnectionState>,
    ) -> BoxFuture<'static, ServiceResult<()>> {
        let manager = self.clone();
        Box::pin(async move {
            let session_ids: Vec<String> = {
                let mut ids = connection.session_ids.lock().expect("session id mutex poisoned");
                let collected = ids.iter().cloned().collect();
                ids.clear();
                collected
            };
            let mut live_sessions = Vec::new();
            for session_id in session_ids {
                let live = manager
                    .live_sessions
                    .lock()
                    .expect("live session mutex poisoned")
                    .iter()
                    .find(|(id, _)| id == &session_id)
                    .map(|(_, live)| live.clone());
                if let Some(live) = live {
                    live.connections
                        .lock()
                        .expect("live connection mutex poisoned")
                        .remove(&connection.id);
                    live_sessions.push(live);
                }
            }
            for live in live_sessions {
                if let Err(error) = manager.maybe_dispose(live).await
                    && let Some(core) = manager.core.upgrade()
                {
                    core.report_error(error.as_ref());
                }
            }
            Ok(())
        })
    }

    pub(crate) fn list_metadata(self: &Arc<Self>) -> BoxFuture<'static, ServiceResult<Vec<SessionMetadata>>> {
        let manager = self.clone();
        Box::pin(async move { manager.list_metadata_inner().await })
    }

    async fn list_metadata_inner(&self) -> ServiceResult<Vec<SessionMetadata>> {
        let stored = self.service.list_sessions().await?;
        let live_sessions: Vec<Arc<LiveSession>> = self
            .live_sessions
            .lock()
            .expect("live session mutex poisoned")
            .iter()
            .filter(|(_, live)| !live.is_disposing())
            .map(|(_, live)| live.clone())
            .collect();
        let mut live_by_id = BTreeMap::new();
        for session in live_sessions {
            live_by_id.insert(session.id.clone(), self.normalized_snapshot(&session).await?);
        }
        let mut metadata = Vec::new();
        for item in stored {
            match live_by_id.remove(item.id()) {
                Some(snapshot) => metadata.push(merge_metadata(&item, &snapshot)),
                None => metadata.push(item),
            }
        }
        for snapshot in live_by_id.values() {
            metadata.push(metadata_from_snapshot(snapshot));
        }
        Ok(metadata)
    }

    pub(crate) fn close(&self) -> BoxFuture<'_, ServiceResult<()>> {
        Box::pin(async move {
            let _guard = self.acquire_lock.lock().await;
            let sessions: Vec<Arc<LiveSession>> = {
                let mut map = self.live_sessions.lock().expect("live session mutex poisoned");
                std::mem::take(&mut *map).into_iter().map(|(_, live)| live).collect()
            };
            for live in sessions {
                if live.is_disposing() {
                    let _dispose_guard = live.dispose_lock.lock().await;
                    continue;
                }
                if let Some(subscription) = live.subscription.lock().expect("subscription mutex poisoned").take() {
                    subscription.unsubscribe();
                }
                if let Err(error) = live.runtime.dispose().await
                    && let Some(core) = self.core.upgrade()
                {
                    core.report_error(error.as_ref());
                }
            }
            Ok(())
        })
    }

    fn broadcast_server_snapshot(&self) {
        if let Some(core) = self.core.upgrade() {
            core.broadcast_server_snapshot();
        }
    }
}

fn disposable(manager: &LiveSessionManager, live: &Arc<LiveSession>) -> bool {
    let Some(core) = manager.core.upgrade() else {
        return false;
    };
    !core.is_closing()
        && live.ready.load(Ordering::SeqCst)
        && !live.is_disposing()
        && live.connection_count() == 0
        && live.operation_count.load(Ordering::SeqCst) == 0
        && (live.is_terminal() || live.runtime.get_phase() == SessionPhase::Idle)
}

pub(crate) fn rebuild_snapshot(
    snapshot: &SessionSnapshot,
    phase: Option<SessionPhase>,
    attached: bool,
    locked: bool,
) -> SessionSnapshot {
    SessionSnapshot::new(pi_protocol::SessionSnapshotInput {
        id: snapshot.id().to_string(),
        name: snapshot.name().map(str::to_string),
        cwd: snapshot.cwd().to_string(),
        created_at: snapshot.created_at(),
        updated_at: snapshot.updated_at(),
        phase: phase.unwrap_or_else(|| snapshot.phase()),
        model: snapshot.model().clone(),
        thinking_level: snapshot.thinking_level(),
        attached,
        locked,
        revision: snapshot.revision(),
        transcript: snapshot.transcript().to_vec(),
        queued_steer: snapshot.queued_steer().to_vec(),
        queued_steer_count: snapshot.queued_steer_count(),
    })
    .expect("snapshots retain their validated fields")
}

fn for_connection(snapshot: &SessionSnapshot, connection: &Arc<ConnectionState>) -> SessionSnapshot {
    let attached = connection
        .session_ids
        .lock()
        .expect("session id mutex poisoned")
        .contains(snapshot.id());
    rebuild_snapshot(snapshot, None, attached, snapshot.locked())
}

pub(crate) fn metadata_from_snapshot(snapshot: &SessionSnapshot) -> SessionMetadata {
    SessionMetadata::new(pi_protocol::SessionMetadataInput {
        id: snapshot.id().to_string(),
        created_at: snapshot.created_at(),
        updated_at: Some(snapshot.updated_at()),
        parent_session_id: None,
        session_name: snapshot.name().map(str::to_string),
        cwd: Some(snapshot.cwd().to_string()),
    })
    .expect("snapshot metadata retains its validated fields")
}

fn merge_metadata(stored: &SessionMetadata, snapshot: &SessionSnapshot) -> SessionMetadata {
    SessionMetadata::new(pi_protocol::SessionMetadataInput {
        id: snapshot.id().to_string(),
        created_at: snapshot.created_at(),
        updated_at: Some(snapshot.updated_at()),
        parent_session_id: stored.parent_session_id().map(str::to_string),
        session_name: snapshot.name().map(str::to_string),
        cwd: Some(snapshot.cwd().to_string()),
    })
    .expect("merged metadata retains its validated fields")
}

fn dropped_error() -> ServiceError {
    Box::new(std::io::Error::other("PiServer was dropped"))
}

async fn broadcast_loop(
    manager: Weak<LiveSessionManager>,
    live: Weak<LiveSession>,
    mut requests: tokio::sync::mpsc::UnboundedReceiver<BroadcastRequest>,
) {
    while let Some(request) = requests.recv().await {
        let result = match (manager.upgrade(), live.upgrade()) {
            (Some(manager), Some(live)) => manager.broadcast_snapshot_now(&live).await,
            _ => Err(Box::new(PiServerError::session_not_found("Session runtime is gone", None)) as ServiceError),
        };
        let _ = request.result.send(result);
    }
}

async fn dispose_runtime(core: &Arc<ServerCore>, runtime: &dyn PiSessionRuntime) {
    if let Err(error) = runtime.dispose().await {
        core.report_error(error.as_ref());
    }
}
