//! Port of `packages/coding-agent/src/client/remote-session.ts`.
//!
//! Lifecycle wrapper over one exclusive `SessionHandle`: attachment replacement, operation
//! serialization, transcript projection, and disposal.

use std::fmt;
use std::future::Future;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Weak};

use pi_client::{
    ConnectionState, ConnectionStateListener, CreateSessionOptions, PiClient, PiError, SessionHandle, SessionLeaseMode,
    Unsubscribe,
};
use pi_protocol::{
    ModelMetadata, ModelRef, ServerEvent, SessionMetadata, SessionPhase, SessionSnapshot, ThinkingLevel, TranscriptItem,
};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use crate::client::transcript::{
    TranscriptState, apply_transcript_progress, apply_transcript_snapshot, create_transcript_state, select_transcript,
};

/// Operations that occupy the busy lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteSessionOperation {
    Open,
    Create,
    Submit,
    Abort,
    SetModel,
    SetThinking,
    Reconnect,
}

impl RemoteSessionOperation {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Create => "create",
            Self::Submit => "submit",
            Self::Abort => "abort",
            Self::SetModel => "setModel",
            Self::SetThinking => "setThinking",
            Self::Reconnect => "reconnect",
        }
    }
}

/// Lifecycle published to subscribers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteSessionLifecycle {
    Unbound,
    Ready,
    Busy(RemoteSessionOperation),
    Disposed,
}

/// Snapshot plus projected transcript.
#[derive(Debug, Clone)]
pub struct RemoteSessionState {
    pub lifecycle: RemoteSessionLifecycle,
    pub snapshot: Option<SessionSnapshot>,
    pub transcript: Vec<TranscriptItem>,
}

/// Options for [`RemoteSession::create_session`].
#[derive(Debug, Clone, Default)]
pub struct CreateRemoteSessionOptions {
    pub cwd: String,
    pub model: Option<ModelRef>,
    pub thinking_level: Option<ThinkingLevel>,
}

/// Options for [`RemoteSession`].
#[derive(Clone, Default)]
pub struct RemoteSessionOptions {
    pub on_listener_error: Option<Arc<dyn Fn(String) + Send + Sync>>,
}

/// Failure raised by remote-session operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteSessionError {
    /// An ordinary message error.
    Message(String),
    /// Cleanup observed a disposed session; ignored by disposal aggregation.
    Disposed,
    /// More than one disposal failure.
    Aggregate(Vec<RemoteSessionError>),
}

impl RemoteSessionError {
    fn message(message: impl Into<String>) -> Self {
        Self::Message(message.into())
    }

    fn from_client(error: PiError) -> Self {
        Self::Message(error.to_string())
    }
}

impl fmt::Display for RemoteSessionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Message(message) => formatter.write_str(message),
            Self::Disposed => formatter.write_str("Remote session is disposed"),
            Self::Aggregate(_) => formatter.write_str("Failed to dispose remote session"),
        }
    }
}

impl std::error::Error for RemoteSessionError {}

type Listener = Arc<dyn Fn(&RemoteSessionState) + Send + Sync>;

struct Inner {
    lifecycle: RemoteSessionLifecycle,
    handle: Option<SessionHandle>,
    transcript: Option<TranscriptState>,
    unsubscribe_snapshot: Option<Unsubscribe>,
    unsubscribe_events: Option<Unsubscribe>,
    active_operation_states: Vec<RemoteSessionLifecycle>,
    listeners: Vec<(u64, Listener)>,
    dispose_started: bool,
}

struct Core {
    client: Arc<PiClient>,
    options: RemoteSessionOptions,
    inner: Mutex<Inner>,
    dispose_signal: CancellationToken,
    pending_attachments: AtomicUsize,
    attachments_idle: Notify,
    next_listener_id: AtomicU64,
}

/// One remote session attachment with projected transcript state.
pub struct RemoteSession {
    core: Arc<Core>,
}

impl RemoteSession {
    fn new(client: Arc<PiClient>, options: RemoteSessionOptions) -> Self {
        Self {
            core: Arc::new(Core {
                client,
                options,
                inner: Mutex::new(Inner {
                    lifecycle: RemoteSessionLifecycle::Unbound,
                    handle: None,
                    transcript: None,
                    unsubscribe_snapshot: None,
                    unsubscribe_events: None,
                    active_operation_states: Vec::new(),
                    listeners: Vec::new(),
                    dispose_started: false,
                }),
                dispose_signal: CancellationToken::new(),
                pending_attachments: AtomicUsize::new(0),
                attachments_idle: Notify::new(),
                next_listener_id: AtomicU64::new(0),
            }),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        lock_core(&self.core)
    }

    pub fn id(&self) -> Option<String> {
        self.lock().handle.as_ref().map(|handle| handle.id().to_owned())
    }

    pub fn state(&self) -> RemoteSessionState {
        state_of(&self.core)
    }

    pub fn snapshot(&self) -> Option<SessionSnapshot> {
        self.lock().transcript.as_ref().map(|state| state.snapshot.clone())
    }

    pub fn phase(&self) -> Option<SessionPhase> {
        self.snapshot().map(|snapshot| snapshot.phase())
    }

    pub fn operation(&self) -> Option<RemoteSessionOperation> {
        match self.lock().lifecycle {
            RemoteSessionLifecycle::Busy(operation) => Some(operation),
            _ => None,
        }
    }

    pub fn models(&self) -> Vec<ModelMetadata> {
        self.core
            .client
            .snapshot()
            .map(|snapshot| snapshot.models().to_vec())
            .unwrap_or_default()
    }

    pub fn sessions(&self) -> Vec<SessionMetadata> {
        self.core
            .client
            .snapshot()
            .map(|snapshot| snapshot.sessions().to_vec())
            .unwrap_or_default()
    }

    pub fn connection_state(&self) -> ConnectionState {
        self.core.client.connection_state()
    }

    pub fn disposed(&self) -> bool {
        self.lock().lifecycle == RemoteSessionLifecycle::Disposed
    }

    pub fn subscribe(&self, listener: Listener) -> Result<Unsubscribe, RemoteSessionError> {
        assert_not_disposed(&self.core)?;
        let id = self.core.next_listener_id.fetch_add(1, Ordering::SeqCst) + 1;
        self.lock().listeners.push((id, Arc::clone(&listener)));
        call_listener(&self.core.options, &listener, &self.state());
        let weak = Arc::downgrade(&self.core);
        Ok(Box::new(move || {
            if let Some(core) = weak.upgrade() {
                lock_core(&core).listeners.retain(|(candidate, _)| *candidate != id);
            }
        }))
    }

    pub fn on_connection_state_change(
        &self,
        listener: ConnectionStateListener,
    ) -> Result<Unsubscribe, RemoteSessionError> {
        assert_not_disposed(&self.core)?;
        self.core
            .client
            .on_connection_state_change(listener)
            .map_err(RemoteSessionError::from_client)
    }

    /// `RemoteSession.open`.
    pub async fn open(
        client: Arc<PiClient>,
        session_id: &str,
        options: RemoteSessionOptions,
    ) -> Result<Self, RemoteSessionError> {
        let session = Self::new(client, options);
        match session.open_session(session_id).await {
            Ok(()) => Ok(session),
            Err(error) => {
                session.dispose().await?;
                Err(error)
            }
        }
    }

    /// `RemoteSession.create`.
    pub async fn create(
        client: Arc<PiClient>,
        create_options: CreateRemoteSessionOptions,
        options: RemoteSessionOptions,
    ) -> Result<Self, RemoteSessionError> {
        let session = Self::new(client, options);
        match session.create_session(create_options).await {
            Ok(()) => Ok(session),
            Err(error) => {
                session.dispose().await?;
                Err(error)
            }
        }
    }

    pub async fn open_session(&self, session_id: &str) -> Result<(), RemoteSessionError> {
        let already_open = {
            let inner = self.lock();
            inner.handle.as_ref().is_some_and(|handle| handle.id() == session_id)
                && inner.lifecycle == RemoteSessionLifecycle::Ready
        };
        if already_open {
            return Ok(());
        }
        let session_id = session_id.to_owned();
        let client = Arc::clone(&self.core.client);
        self.replace(RemoteSessionOperation::Open, move || async move {
            client
                .acquire_session(session_id, SessionLeaseMode::Exclusive)
                .map_err(RemoteSessionError::from_client)?
                .await
                .map_err(RemoteSessionError::from_client)
        })
        .await
    }

    pub async fn create_session(&self, options: CreateRemoteSessionOptions) -> Result<(), RemoteSessionError> {
        let client = Arc::clone(&self.core.client);
        self.replace(RemoteSessionOperation::Create, move || async move {
            client
                .create_session(CreateSessionOptions {
                    cwd: Some(options.cwd),
                    name: None,
                    model: options.model,
                    thinking_level: options.thinking_level,
                })
                .map_err(RemoteSessionError::from_client)?
                .await
                .map_err(RemoteSessionError::from_client)
        })
        .await
    }

    pub async fn submit(&self, text: &str) -> Result<(), RemoteSessionError> {
        let normalized = text.trim();
        if normalized.is_empty() {
            return Ok(());
        }
        assert_available(&self.core)?;
        let handle = require_handle(&self.core)?;
        let phase = self.phase();
        if phase != Some(SessionPhase::Idle) && phase != Some(SessionPhase::Turn) {
            return Err(RemoteSessionError::message(format!(
                "Session cannot accept input during {} phase",
                phase.map_or("unknown", SessionPhase::as_str)
            )));
        }
        let prompt = phase == Some(SessionPhase::Idle);
        let text = normalized.to_owned();
        self.run_operation(RemoteSessionOperation::Submit, false, move || async move {
            let future = if prompt {
                handle.prompt(text)
            } else {
                handle.steer(text)
            };
            future
                .map_err(RemoteSessionError::from_client)?
                .await
                .map(|_| ())
                .map_err(RemoteSessionError::from_client)
        })
        .await
    }

    pub async fn abort(&self) -> Result<(), RemoteSessionError> {
        let preempting_submit = matches!(
            self.lock().lifecycle,
            RemoteSessionLifecycle::Busy(RemoteSessionOperation::Submit)
        );
        if preempting_submit {
            assert_not_disposed(&self.core)?;
        } else {
            assert_available(&self.core)?;
        }
        let handle = require_handle(&self.core)?;
        if self.phase() == Some(SessionPhase::Idle) && !preempting_submit {
            return Ok(());
        }
        self.run_operation(RemoteSessionOperation::Abort, preempting_submit, move || async move {
            handle
                .abort()
                .map_err(RemoteSessionError::from_client)?
                .await
                .map(|_| ())
                .map_err(RemoteSessionError::from_client)
        })
        .await
    }

    pub async fn set_model(&self, model: ModelRef) -> Result<(), RemoteSessionError> {
        self.run_idle_operation(
            RemoteSessionOperation::SetModel,
            "change model",
            move |handle| async move {
                handle
                    .set_model(model)
                    .map_err(RemoteSessionError::from_client)?
                    .await
                    .map(|_| ())
                    .map_err(RemoteSessionError::from_client)
            },
        )
        .await
    }

    pub async fn set_thinking(&self, thinking_level: ThinkingLevel) -> Result<(), RemoteSessionError> {
        self.run_idle_operation(
            RemoteSessionOperation::SetThinking,
            "change thinking level",
            move |handle| async move {
                handle
                    .set_thinking(thinking_level)
                    .map_err(RemoteSessionError::from_client)?
                    .await
                    .map(|_| ())
                    .map_err(RemoteSessionError::from_client)
            },
        )
        .await
    }

    pub async fn reconnect(&self) -> Result<(), RemoteSessionError> {
        assert_available(&self.core)?;
        let session_id = require_handle(&self.core)?.id().to_owned();
        let client = Arc::clone(&self.core.client);
        let core = Arc::clone(&self.core);
        let attachment_core = Arc::clone(&self.core);
        self.run_operation(RemoteSessionOperation::Reconnect, false, move || async move {
            track_attachment_operation(&core, async move {
                client
                    .reconnect()
                    .map_err(RemoteSessionError::from_client)?
                    .await
                    .map_err(RemoteSessionError::from_client)?;
                let handle = client
                    .acquire_session(session_id, SessionLeaseMode::Exclusive)
                    .map_err(RemoteSessionError::from_client)?
                    .await
                    .map_err(RemoteSessionError::from_client)?;
                assert_not_disposed_after_await(&attachment_core, &handle).await?;
                bind(&attachment_core, handle, None)
            })
            .await
        })
        .await
    }

    /// `dispose`: idempotent; waits for in-flight attachments, then releases the lease.
    pub async fn dispose(&self) -> Result<(), RemoteSessionError> {
        let core = &self.core;
        let already_started = {
            let mut inner = lock_core(core);
            let started = inner.dispose_started;
            inner.dispose_started = true;
            inner.lifecycle = RemoteSessionLifecycle::Disposed;
            started
        };
        if already_started {
            return Ok(());
        }
        core.dispose_signal.cancel();
        clear_subscriptions(core);
        let handle = lock_core(core).handle.take();
        lock_core(core).transcript = None;
        while core.pending_attachments.load(Ordering::SeqCst) > 0 {
            core.attachments_idle.notified().await;
        }
        let mut errors = Vec::new();
        if let Some(handle) = handle {
            match handle.dispose().map_err(RemoteSessionError::from_client) {
                Ok(future) => {
                    if let Err(error) = future.await {
                        errors.push(RemoteSessionError::from_client(error));
                    }
                }
                Err(error) => errors.push(error),
            }
        }
        let errors: Vec<RemoteSessionError> = errors
            .into_iter()
            .filter(|error| !matches!(error, RemoteSessionError::Disposed))
            .collect();
        lock_core(core).listeners.clear();
        notify(core);
        match errors.len() {
            0 => Ok(()),
            1 => Err(errors.into_iter().next().expect("one error")),
            _ => Err(RemoteSessionError::Aggregate(errors)),
        }
    }

    async fn replace<Prepare, PrepareFuture>(
        &self,
        operation: RemoteSessionOperation,
        prepare: Prepare,
    ) -> Result<(), RemoteSessionError>
    where
        Prepare: FnOnce() -> PrepareFuture + Send,
        PrepareFuture: Future<Output = Result<SessionHandle, RemoteSessionError>> + Send,
    {
        assert_available(&self.core)?;
        let phase = self.phase();
        if self.lock().handle.is_some() && phase != Some(SessionPhase::Idle) {
            return Err(RemoteSessionError::message(format!(
                "Cannot {} a session while session is {}",
                operation.as_str(),
                phase.map_or("unavailable", SessionPhase::as_str)
            )));
        }
        let core = Arc::clone(&self.core);
        self.run_operation(operation, false, move || async move {
            track_attachment_operation(&core, prepare_replacement(&core, operation, prepare)).await
        })
        .await
    }

    async fn run_idle_operation<Run, RunFuture>(
        &self,
        operation: RemoteSessionOperation,
        description: &'static str,
        run: Run,
    ) -> Result<(), RemoteSessionError>
    where
        Run: FnOnce(SessionHandle) -> RunFuture + Send,
        RunFuture: Future<Output = Result<(), RemoteSessionError>> + Send,
    {
        assert_available(&self.core)?;
        let handle = require_handle(&self.core)?;
        let phase = self.phase();
        if phase != Some(SessionPhase::Idle) {
            return Err(RemoteSessionError::message(format!(
                "Cannot {description} while session is {}",
                phase.map_or("unavailable", SessionPhase::as_str)
            )));
        }
        self.run_operation(operation, false, move || run(handle)).await
    }

    async fn run_operation<Run, RunFuture>(
        &self,
        operation: RemoteSessionOperation,
        preempt: bool,
        run: Run,
    ) -> Result<(), RemoteSessionError>
    where
        Run: FnOnce() -> RunFuture,
        RunFuture: Future<Output = Result<(), RemoteSessionError>>,
    {
        if preempt {
            assert_not_disposed(&self.core)?;
        } else {
            assert_available(&self.core)?;
        }
        let core = &self.core;
        let previous = {
            let mut inner = lock_core(core);
            let previous = inner.lifecycle;
            inner.lifecycle = RemoteSessionLifecycle::Busy(operation);
            inner
                .active_operation_states
                .push(RemoteSessionLifecycle::Busy(operation));
            previous
        };
        notify(core);
        let running = run();
        tokio::pin!(running);
        let result = tokio::select! {
            result = &mut running => result,
            () = core.dispose_signal.cancelled() => Err(RemoteSessionError::Disposed),
        };
        {
            let mut inner = lock_core(core);
            inner
                .active_operation_states
                .retain(|state| *state != RemoteSessionLifecycle::Busy(operation));
            if inner.lifecycle != RemoteSessionLifecycle::Disposed
                && inner.lifecycle == RemoteSessionLifecycle::Busy(operation)
            {
                inner.lifecycle = if preempt && inner.active_operation_states.contains(&previous) {
                    previous
                } else if inner.handle.is_some() {
                    RemoteSessionLifecycle::Ready
                } else {
                    RemoteSessionLifecycle::Unbound
                };
            }
        }
        notify(core);
        result
    }
}

fn lock_core(core: &Core) -> std::sync::MutexGuard<'_, Inner> {
    core.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn state_of(core: &Core) -> RemoteSessionState {
    let inner = lock_core(core);
    RemoteSessionState {
        lifecycle: inner.lifecycle,
        snapshot: inner.transcript.as_ref().map(|state| state.snapshot.clone()),
        transcript: inner.transcript.as_ref().map(select_transcript).unwrap_or_default(),
    }
}

async fn prepare_replacement<Prepare, PrepareFuture>(
    core: &Arc<Core>,
    operation: RemoteSessionOperation,
    prepare: Prepare,
) -> Result<(), RemoteSessionError>
where
    Prepare: FnOnce() -> PrepareFuture,
    PrepareFuture: Future<Output = Result<SessionHandle, RemoteSessionError>>,
{
    let previous = lock_core(core).handle.clone();
    let next = prepare().await?;
    assert_not_disposed_after_await(core, &next).await?;
    let Some(snapshot) = next.snapshot() else {
        let id = next.id().to_owned();
        detach_handle(next).await;
        return Err(RemoteSessionError::message(format!(
            "Session {id} did not provide a snapshot"
        )));
    };
    let phase = state_of(core).snapshot.as_ref().map(SessionSnapshot::phase);
    if let Some(previous) = &previous
        && previous.id() != next.id()
        && previous.attached()
        && phase != Some(SessionPhase::Idle)
    {
        detach_handle(next).await;
        return Err(RemoteSessionError::message(format!(
            "Cannot {} a session while session is {}",
            operation.as_str(),
            phase.map_or("unavailable", SessionPhase::as_str)
        )));
    }
    if let Some(previous) = &previous
        && previous.id() != next.id()
        && previous.attached()
        && let Err(error) = previous.detach().map_err(RemoteSessionError::from_client)?.await
    {
        let cleanup_error = detach_handle(next).await;
        if let Some(cleanup_error) = cleanup_error {
            return Err(RemoteSessionError::Aggregate(vec![
                RemoteSessionError::from_client(error),
                cleanup_error,
            ]));
        }
        return Err(RemoteSessionError::from_client(error));
    }
    assert_not_disposed_after_await(core, &next).await?;
    bind(core, next, Some(snapshot))
}

async fn track_attachment_operation<RunFuture>(core: &Core, run: RunFuture) -> Result<(), RemoteSessionError>
where
    RunFuture: Future<Output = Result<(), RemoteSessionError>>,
{
    core.pending_attachments.fetch_add(1, Ordering::SeqCst);
    let result = run.await;
    if core.pending_attachments.fetch_sub(1, Ordering::SeqCst) == 1 {
        core.attachments_idle.notify_waiters();
    }
    result
}

fn bind(
    core: &Arc<Core>,
    handle: SessionHandle,
    known_snapshot: Option<SessionSnapshot>,
) -> Result<(), RemoteSessionError> {
    let Some(snapshot) = known_snapshot.or_else(|| handle.snapshot()) else {
        return Err(RemoteSessionError::message(format!(
            "Session {} did not provide a snapshot",
            handle.id()
        )));
    };
    clear_subscriptions(core);
    let weak: Weak<Core> = Arc::downgrade(core);
    let snapshot_listener = {
        let weak = weak.clone();
        Arc::new(move |next: &SessionSnapshot| {
            if let Some(core) = weak.upgrade() {
                apply_snapshot(&core, next);
            }
        })
    };
    let event_listener = Arc::new(move |event: &ServerEvent| {
        if let Some(core) = weak.upgrade() {
            handle_event(&core, event);
        }
    });
    let unsubscribe_snapshot = handle
        .subscribe(snapshot_listener)
        .map_err(RemoteSessionError::from_client)?;
    let unsubscribe_events = handle
        .on_event(event_listener)
        .map_err(RemoteSessionError::from_client)?;
    {
        let mut inner = lock_core(core);
        inner.handle = Some(handle);
        inner.transcript = Some(create_transcript_state(&snapshot));
        inner.unsubscribe_snapshot = Some(unsubscribe_snapshot);
        inner.unsubscribe_events = Some(unsubscribe_events);
    }
    notify(core);
    Ok(())
}

fn apply_snapshot(core: &Core, snapshot: &SessionSnapshot) {
    {
        let mut inner = lock_core(core);
        let Some(transcript) = inner.transcript.take() else {
            return;
        };
        inner.transcript = Some(apply_transcript_snapshot(transcript, snapshot));
    }
    notify(core);
}

fn handle_event(core: &Core, event: &ServerEvent) {
    if let ServerEvent::SessionRemoved(_) = event {
        clear_subscriptions(core);
        let mut inner = lock_core(core);
        inner.handle = None;
        inner.transcript = None;
        if !matches!(inner.lifecycle, RemoteSessionLifecycle::Busy(_)) {
            inner.lifecycle = RemoteSessionLifecycle::Unbound;
        }
        drop(inner);
        notify(core);
        return;
    }
    let ServerEvent::SessionProgress(progress) = event else {
        return;
    };
    {
        let mut inner = lock_core(core);
        let Some(transcript) = inner.transcript.take() else {
            return;
        };
        inner.transcript = Some(apply_transcript_progress(transcript, progress.progress()));
    }
    notify(core);
}

fn notify(core: &Core) {
    let state = state_of(core);
    let listeners: Vec<Listener> = lock_core(core)
        .listeners
        .iter()
        .map(|(_, listener)| Arc::clone(listener))
        .collect();
    for listener in listeners {
        call_listener(&core.options, &listener, &state);
    }
}

fn clear_subscriptions(core: &Core) {
    let (unsubscribe_snapshot, unsubscribe_events) = {
        let mut inner = lock_core(core);
        (inner.unsubscribe_snapshot.take(), inner.unsubscribe_events.take())
    };
    if let Some(unsubscribe) = unsubscribe_snapshot {
        unsubscribe();
    }
    if let Some(unsubscribe) = unsubscribe_events {
        unsubscribe();
    }
}

fn require_handle(core: &Core) -> Result<SessionHandle, RemoteSessionError> {
    lock_core(core)
        .handle
        .clone()
        .ok_or_else(|| RemoteSessionError::message("No remote session is attached"))
}

fn assert_available(core: &Core) -> Result<(), RemoteSessionError> {
    assert_not_disposed(core)?;
    let lifecycle = lock_core(core).lifecycle;
    if let RemoteSessionLifecycle::Busy(operation) = lifecycle {
        return Err(RemoteSessionError::message(format!(
            "Remote session is busy with {}",
            operation.as_str()
        )));
    }
    Ok(())
}

fn assert_not_disposed(core: &Core) -> Result<(), RemoteSessionError> {
    if lock_core(core).lifecycle == RemoteSessionLifecycle::Disposed {
        return Err(RemoteSessionError::message("Remote session is disposed"));
    }
    Ok(())
}

async fn assert_not_disposed_after_await(core: &Core, handle: &SessionHandle) -> Result<(), RemoteSessionError> {
    if lock_core(core).lifecycle != RemoteSessionLifecycle::Disposed {
        return Ok(());
    }
    let _ = detach_handle(handle.clone()).await;
    Err(RemoteSessionError::Disposed)
}

async fn detach_handle(handle: SessionHandle) -> Option<RemoteSessionError> {
    match handle.dispose().map_err(RemoteSessionError::from_client) {
        Ok(future) => future.await.map_err(RemoteSessionError::from_client).err(),
        Err(error) => Some(error),
    }
}

fn call_listener(options: &RemoteSessionOptions, listener: &Listener, state: &RemoteSessionState) {
    let Err(payload) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| listener(state))) else {
        return;
    };
    let Some(handler) = &options.on_listener_error else {
        return;
    };
    let message = payload
        .downcast_ref::<&str>()
        .map(|message| (*message).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "Remote session listener failed".to_owned());
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| handler(message)));
}
