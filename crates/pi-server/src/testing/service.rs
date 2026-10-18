use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use pi_protocol::{
    AssistantContent, AssistantStopReason, AssistantTranscriptItem, AssistantTranscriptState, ModelCost, ModelInput,
    ModelMetadata, ModelRef, SessionMetadata, SessionPhase, SessionSnapshot, TextContent, ThinkingLevel,
    TranscriptItem, TranscriptProgress, UserContent, UserTranscriptItem,
};
use tokio::sync::watch;

use crate::errors::PiServerError;
use crate::sessions::metadata_from_snapshot;
use crate::types::{
    BoxFuture, CreateSessionOptions, PiServerService, PiSessionRuntime, PiSessionRuntimeEvent, PromptInput,
    RuntimeListener, RuntimeSubscription, ServiceError, ServiceResult, SteerInput,
};

pub static TEST_MODEL: LazyLock<ModelMetadata> = LazyLock::new(|| {
    ModelMetadata::new(pi_protocol::ModelMetadataInput {
        provider: "test".into(),
        id: "small".into(),
        name: "Test Small".into(),
        api: "test-api".into(),
        reasoning: true,
        input: vec![ModelInput::Text, ModelInput::Image],
        context_window: 16_000,
        max_tokens: 2_000,
        cost: ModelCost::new(0.0, 0.0, 0.0, 0.0).expect("zero costs are valid"),
        supported_thinking_levels: vec![ThinkingLevel::Off, ThinkingLevel::Medium, ThinkingLevel::High],
        authenticated: true,
    })
    .expect("test model metadata is valid")
});

/// A single-resolution async value.
#[derive(Clone)]
pub struct Deferred<T> {
    sender: watch::Sender<Option<T>>,
}

impl<T: Clone> Deferred<T> {
    pub fn new() -> Self {
        let (sender, _) = watch::channel(None);
        Self { sender }
    }

    pub fn resolve(&self, value: T) {
        let _ = self.sender.send(Some(value));
    }

    pub async fn wait(&self) -> T {
        let mut receiver = self.sender.subscribe();
        let value = receiver
            .wait_for(|value| value.is_some())
            .await
            .expect("deferred sender must outlive waiters");
        value.clone().expect("resolved deferred values are present")
    }
}

impl<T: Clone> Default for Deferred<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PromptOutcome {
    Complete,
    Aborted,
}

struct PendingPrompt {
    done: Deferred<PromptOutcome>,
}

pub(crate) struct StoredSession {
    snapshot: Mutex<SessionSnapshot>,
}

impl StoredSession {
    fn new(snapshot: SessionSnapshot) -> Arc<Self> {
        Arc::new(Self {
            snapshot: Mutex::new(snapshot),
        })
    }
}

pub struct TestSessionRuntime {
    stored: Arc<StoredSession>,
    on_dispose: Arc<dyn Fn() + Send + Sync>,
    listeners: Arc<Mutex<BTreeMap<u64, RuntimeListener>>>,
    next_listener: AtomicU64,
    pending_prompt: Mutex<Option<PendingPrompt>>,
    steers: Mutex<Vec<PromptInput>>,
    dispose_count: AtomicUsize,
    disposed: Deferred<()>,
}

impl TestSessionRuntime {
    fn new(stored: Arc<StoredSession>, on_dispose: Arc<dyn Fn() + Send + Sync>) -> Self {
        Self {
            stored,
            on_dispose,
            listeners: Arc::new(Mutex::new(BTreeMap::new())),
            next_listener: AtomicU64::new(0),
            pending_prompt: Mutex::new(None),
            steers: Mutex::new(Vec::new()),
            dispose_count: AtomicUsize::new(0),
            disposed: Deferred::new(),
        }
    }

    pub fn dispose_count(&self) -> usize {
        self.dispose_count.load(Ordering::SeqCst)
    }

    pub fn steers(&self) -> Vec<PromptInput> {
        self.steers.lock().expect("steers mutex poisoned").clone()
    }

    pub async fn disposed(&self) {
        self.disposed.wait().await;
    }

    pub fn set_phase(&self, phase: SessionPhase) {
        let mut stored = self.stored.snapshot.lock().expect("snapshot mutex poisoned");
        let mut input = snapshot_input(&stored);
        input.phase = phase;
        *stored = SessionSnapshot::new(input).expect("snapshot retains its validated fields");
    }

    pub fn finish_prompt(&self) {
        let pending = self.pending_prompt.lock().expect("pending prompt mutex poisoned");
        let pending = pending.as_ref().expect("No prompt is pending");
        pending.done.resolve(PromptOutcome::Complete);
    }

    pub fn emit_progress(&self, progress: TranscriptProgress) {
        for listener in self.listeners.lock().expect("listener mutex poisoned").values() {
            listener(PiSessionRuntimeEvent::Progress(Box::new(progress.clone())));
        }
    }

    pub fn emit_error(&self, error: PiServerError) {
        for listener in self.listeners.lock().expect("listener mutex poisoned").values() {
            listener(PiSessionRuntimeEvent::Error(error.clone()));
        }
    }

    pub fn emit_snapshot(&self) {
        for listener in self.listeners.lock().expect("listener mutex poisoned").values() {
            listener(PiSessionRuntimeEvent::Snapshot);
        }
    }

    fn update(&self, update: impl FnOnce(&mut pi_protocol::SessionSnapshotInput)) {
        let mut stored = self.stored.snapshot.lock().expect("snapshot mutex poisoned");
        let mut input = snapshot_input(&stored);
        update(&mut input);
        input.revision = stored.revision() + 1;
        input.updated_at = stored.updated_at() + 1;
        *stored = SessionSnapshot::new(input).expect("snapshot retains its validated fields");
        drop(stored);
        self.emit_snapshot();
    }

    fn begin_prompt(&self, text: &str) -> ServiceResult<Deferred<PromptOutcome>> {
        if self.get_phase() != SessionPhase::Idle {
            return Err(server_error(PiServerError::session_busy(
                "A prompt is already running",
                None,
            )));
        }
        let done = Deferred::new();
        *self.pending_prompt.lock().expect("pending prompt mutex poisoned") =
            Some(PendingPrompt { done: done.clone() });
        let revision = self.stored.snapshot.lock().expect("snapshot mutex poisoned").revision();
        self.update(|snapshot| {
            snapshot.phase = SessionPhase::Turn;
            snapshot.transcript.push(TranscriptItem::User(
                UserTranscriptItem::new(
                    format!("user-{}", revision + 1),
                    vec![UserContent::Text(TextContent::new(text.to_string()))],
                    revision + 1,
                )
                .expect("generated user items are valid"),
            ));
        });
        Ok(done)
    }

    fn finish_prompt_transcript(&self, outcome: PromptOutcome, prompt_text: &str) {
        let revision = self.stored.snapshot.lock().expect("snapshot mutex poisoned").revision();
        let (text, state) = match outcome {
            PromptOutcome::Complete => (
                format!("reply:{prompt_text}"),
                AssistantTranscriptState::Complete {
                    stop_reason: AssistantStopReason::Stop,
                },
            ),
            PromptOutcome::Aborted => (String::new(), AssistantTranscriptState::Aborted { error_message: None }),
        };
        let model = self
            .stored
            .snapshot
            .lock()
            .expect("snapshot mutex poisoned")
            .model()
            .clone();
        let assistant = AssistantTranscriptItem::new_runtime(pi_protocol::AssistantTranscriptItemInput {
            id: format!("assistant-{}", revision + 1),
            content: vec![AssistantContent::Text(TextContent::new(text))],
            model,
            response_model: None,
            usage: None,
            timestamp: revision + 1,
            state,
        })
        .expect("generated assistant items are valid");
        self.update(|snapshot| {
            snapshot.phase = SessionPhase::Idle;
            snapshot.transcript.push(TranscriptItem::Assistant(assistant));
        });
        *self.pending_prompt.lock().expect("pending prompt mutex poisoned") = None;
    }
}

impl PiSessionRuntime for TestSessionRuntime {
    fn snapshot(&self) -> BoxFuture<'_, ServiceResult<SessionSnapshot>> {
        Box::pin(async move { Ok(self.stored.snapshot.lock().expect("snapshot mutex poisoned").clone()) })
    }

    fn get_phase(&self) -> SessionPhase {
        self.stored.snapshot.lock().expect("snapshot mutex poisoned").phase()
    }

    fn prompt(&self, input: PromptInput) -> BoxFuture<'_, ServiceResult<()>> {
        Box::pin(async move {
            let done = self.begin_prompt(&input.text)?;
            let outcome = done.wait().await;
            self.finish_prompt_transcript(outcome, &input.text);
            Ok(())
        })
    }

    fn steer(&self, input: SteerInput) -> BoxFuture<'_, ServiceResult<()>> {
        Box::pin(async move {
            if self.get_phase() == SessionPhase::Idle {
                return Err(server_error(PiServerError::session_busy(
                    "There is no active prompt to steer",
                    None,
                )));
            }
            self.steers.lock().expect("steers mutex poisoned").push(input.clone());
            let revision = self.stored.snapshot.lock().expect("snapshot mutex poisoned").revision();
            self.update(|snapshot| {
                snapshot.queued_steer_count += 1;
                snapshot.queued_steer.push(
                    UserTranscriptItem::new(
                        format!("steer-{}", revision + 1),
                        vec![UserContent::Text(TextContent::new(input.text.clone()))],
                        revision + 1,
                    )
                    .expect("generated steer items are valid"),
                );
            });
            Ok(())
        })
    }

    fn abort(&self) -> BoxFuture<'_, ServiceResult<()>> {
        Box::pin(async move {
            let pending = self.pending_prompt.lock().expect("pending prompt mutex poisoned");
            let pending = pending.as_ref().ok_or_else(|| {
                Box::new(PiServerError::session_busy("There is no active prompt to abort", None))
                    as crate::types::ServiceError
            })?;
            pending.done.resolve(PromptOutcome::Aborted);
            Ok(())
        })
    }

    fn set_model(&self, model: ModelRef) -> BoxFuture<'_, ServiceResult<()>> {
        Box::pin(async move {
            if self.get_phase() != SessionPhase::Idle {
                return Err(server_error(PiServerError::session_busy("Session is busy", None)));
            }
            self.update(|snapshot| snapshot.model = model);
            Ok(())
        })
    }

    fn set_thinking(&self, thinking_level: ThinkingLevel) -> BoxFuture<'_, ServiceResult<()>> {
        Box::pin(async move {
            if self.get_phase() != SessionPhase::Idle {
                return Err(server_error(PiServerError::session_busy("Session is busy", None)));
            }
            self.update(|snapshot| snapshot.thinking_level = thinking_level);
            Ok(())
        })
    }

    fn subscribe(&self, listener: RuntimeListener) -> RuntimeSubscription {
        let id = self.next_listener.fetch_add(1, Ordering::SeqCst) + 1;
        self.listeners
            .lock()
            .expect("listener mutex poisoned")
            .insert(id, listener);
        let listeners = self.listeners.clone();
        RuntimeSubscription::new(Box::new(move || {
            listeners.lock().expect("listener mutex poisoned").remove(&id);
        }))
    }

    fn dispose(&self) -> BoxFuture<'_, ServiceResult<()>> {
        Box::pin(async move {
            self.dispose_count.fetch_add(1, Ordering::SeqCst);
            (self.on_dispose)();
            self.disposed.resolve(());
            Ok(())
        })
    }
}

fn server_error(error: PiServerError) -> ServiceError {
    Box::new(error)
}

fn snapshot_input(snapshot: &SessionSnapshot) -> pi_protocol::SessionSnapshotInput {
    pi_protocol::SessionSnapshotInput {
        id: snapshot.id().to_string(),
        name: snapshot.name().map(str::to_string),
        cwd: snapshot.cwd().to_string(),
        created_at: snapshot.created_at(),
        updated_at: snapshot.updated_at(),
        phase: snapshot.phase(),
        model: snapshot.model().clone(),
        thinking_level: snapshot.thinking_level(),
        attached: snapshot.attached(),
        locked: snapshot.locked(),
        revision: snapshot.revision(),
        transcript: snapshot.transcript().to_vec(),
        queued_steer: snapshot.queued_steer().to_vec(),
        queued_steer_count: snapshot.queued_steer_count(),
    }
}

#[derive(Clone)]
pub struct ListDelay {
    pub entered: Deferred<()>,
    pub release: Deferred<()>,
}

#[derive(Default)]
struct ServiceState {
    sessions: Mutex<Vec<(String, Arc<StoredSession>)>>,
    runtimes: Mutex<BTreeMap<String, Vec<Arc<TestSessionRuntime>>>>,
    locked: Mutex<BTreeSet<String>>,
    last_created_id: Mutex<Option<String>>,
    next_list_delay: Mutex<Option<ListDelay>>,
}

/// Deterministic in-memory service used by transport conformance tests.
#[derive(Clone, Default)]
pub struct TestServerService {
    inner: Arc<ServiceState>,
}

impl TestServerService {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn last_created_id(&self) -> Option<String> {
        self.inner
            .last_created_id
            .lock()
            .expect("last created id mutex poisoned")
            .clone()
    }

    pub fn latest_runtime(&self, id: &str) -> Arc<TestSessionRuntime> {
        self.inner
            .runtimes
            .lock()
            .expect("runtimes mutex poisoned")
            .get(id)
            .and_then(|runtimes| runtimes.last().cloned())
            .unwrap_or_else(|| panic!("No runtime for {id}"))
    }

    pub fn runtime_count(&self, id: &str) -> usize {
        self.inner
            .runtimes
            .lock()
            .expect("runtimes mutex poisoned")
            .get(id)
            .map_or(0, Vec::len)
    }

    pub fn is_locked(&self, id: &str) -> bool {
        self.inner
            .locked
            .lock()
            .expect("locked sessions mutex poisoned")
            .contains(id)
    }

    pub fn lock(&self, id: &str) {
        self.inner
            .locked
            .lock()
            .expect("locked sessions mutex poisoned")
            .insert(id.to_string());
    }

    pub fn delay_next_list(&self) -> ListDelay {
        let delay = ListDelay {
            entered: Deferred::new(),
            release: Deferred::new(),
        };
        *self.inner.next_list_delay.lock().expect("list delay mutex poisoned") = Some(delay.clone());
        delay
    }

    pub fn seed(&self, id: &str) {
        self.seed_with(id, None, None, None, None);
    }

    pub fn seed_named(&self, id: &str, name: &str) {
        self.seed_with(id, Some(name.to_string()), None, None, None);
    }

    pub fn seed_with(
        &self,
        id: &str,
        name: Option<String>,
        cwd: Option<String>,
        model: Option<ModelRef>,
        thinking_level: Option<ThinkingLevel>,
    ) {
        let model = model.unwrap_or_else(|| {
            ModelRef::new(TEST_MODEL.provider().to_string(), TEST_MODEL.id().to_string())
                .expect("test model reference is valid")
        });
        let snapshot = SessionSnapshot::new(pi_protocol::SessionSnapshotInput {
            id: id.to_string(),
            name: Some(name.unwrap_or_else(|| format!("Session {id}"))),
            cwd: cwd.unwrap_or_else(|| "/tmp/pi-server-conformance".to_string()),
            created_at: 1,
            updated_at: 1,
            phase: SessionPhase::Idle,
            model,
            thinking_level: thinking_level.unwrap_or(ThinkingLevel::Off),
            attached: false,
            locked: false,
            revision: 0,
            transcript: Vec::new(),
            queued_steer: Vec::new(),
            queued_steer_count: 0,
        })
        .expect("seeded snapshots are valid");
        let mut sessions = self.inner.sessions.lock().expect("sessions mutex poisoned");
        let stored = StoredSession::new(snapshot);
        match sessions.iter_mut().find(|(session_id, _)| session_id == id) {
            Some((_, existing)) => *existing = stored,
            None => sessions.push((id.to_string(), stored)),
        }
    }

    fn acquire(&self, id: &str) -> ServiceResult<Arc<TestSessionRuntime>> {
        let stored = self
            .inner
            .sessions
            .lock()
            .expect("sessions mutex poisoned")
            .iter()
            .find(|(session_id, _)| session_id == id)
            .map(|(_, stored)| stored.clone())
            .ok_or_else(|| {
                Box::new(std::io::Error::other(format!("Unknown session: {id}"))) as crate::types::ServiceError
            })?;
        self.inner
            .locked
            .lock()
            .expect("locked sessions mutex poisoned")
            .insert(id.to_string());
        let state = self.inner.clone();
        let session_id = id.to_string();
        let runtime = Arc::new(TestSessionRuntime::new(
            stored,
            Arc::new(move || {
                state
                    .locked
                    .lock()
                    .expect("locked sessions mutex poisoned")
                    .remove(&session_id);
            }),
        ));
        self.inner
            .runtimes
            .lock()
            .expect("runtimes mutex poisoned")
            .entry(id.to_string())
            .or_default()
            .push(runtime.clone());
        Ok(runtime)
    }
}

impl PiServerService for TestServerService {
    fn list_sessions(&self) -> BoxFuture<'_, ServiceResult<Vec<SessionMetadata>>> {
        Box::pin(async move {
            let delay = self
                .inner
                .next_list_delay
                .lock()
                .expect("list delay mutex poisoned")
                .take();
            if let Some(delay) = delay {
                delay.entered.resolve(());
                delay.release.wait().await;
            }
            let sessions = self.inner.sessions.lock().expect("sessions mutex poisoned");
            Ok(sessions
                .iter()
                .map(|(_, stored)| metadata_from_snapshot(&stored.snapshot.lock().expect("snapshot mutex poisoned")))
                .collect())
        })
    }

    fn list_models(&self) -> BoxFuture<'_, ServiceResult<Vec<ModelMetadata>>> {
        Box::pin(async move { Ok(vec![TEST_MODEL.clone()]) })
    }

    fn create_session(&self, options: CreateSessionOptions) -> BoxFuture<'_, ServiceResult<Arc<dyn PiSessionRuntime>>> {
        Box::pin(async move {
            *self
                .inner
                .last_created_id
                .lock()
                .expect("last created id mutex poisoned") = Some(options.id.clone());
            if self
                .inner
                .sessions
                .lock()
                .expect("sessions mutex poisoned")
                .iter()
                .any(|(session_id, _)| session_id == &options.id)
            {
                return Err(server_error(PiServerError::session_locked(
                    "Session already exists",
                    None,
                )));
            }
            self.seed_with(
                &options.id,
                options.name,
                options.cwd,
                options.model,
                options.thinking_level,
            );
            let runtime = self.acquire(&options.id)?;
            Ok(runtime as Arc<dyn PiSessionRuntime>)
        })
    }

    fn open_session(&self, session_id: String) -> BoxFuture<'_, ServiceResult<Arc<dyn PiSessionRuntime>>> {
        Box::pin(async move {
            if !self
                .inner
                .sessions
                .lock()
                .expect("sessions mutex poisoned")
                .iter()
                .any(|(existing, _)| existing == &session_id)
            {
                return Err(server_error(PiServerError::session_not_found(
                    format!("Unknown session: {session_id}"),
                    None,
                )));
            }
            if self
                .inner
                .locked
                .lock()
                .expect("locked sessions mutex poisoned")
                .contains(&session_id)
            {
                return Err(server_error(PiServerError::session_locked(
                    format!("Session is locked: {session_id}"),
                    None,
                )));
            }
            let runtime = self.acquire(&session_id)?;
            Ok(runtime as Arc<dyn PiSessionRuntime>)
        })
    }
}
