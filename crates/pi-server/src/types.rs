use std::error::Error;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use pi_protocol::{
    ModelMetadata, ModelRef, SessionMetadata, SessionPhase, SessionSnapshot, ThinkingLevel, TranscriptProgress,
};
use tokio::sync::watch;

use crate::errors::PiServerError;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// A service/runtime failure. Typed errors cross the protocol boundary; other errors stay internal.
pub type ServiceError = Box<dyn Error + Send + Sync>;
pub type ServiceResult<T> = Result<T, ServiceError>;

/// Prompt text accepted by a session runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptInput {
    pub text: String,
}

impl PromptInput {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}

/// Steering text accepted while a turn is running.
pub type SteerInput = PromptInput;

/// Values used to create a durable session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateSessionOptions {
    /// A collision-resistant ID assigned by [`crate::PiServer`]. The service must persist this exact ID.
    pub id: String,
    pub cwd: Option<String>,
    pub name: Option<String>,
    pub model: Option<ModelRef>,
    pub thinking_level: Option<ThinkingLevel>,
}

/// An event pushed by a session runtime.
#[derive(Debug, Clone, PartialEq)]
pub enum PiSessionRuntimeEvent {
    Snapshot,
    Progress(Box<TranscriptProgress>),
    Error(PiServerError),
}

/// Listener registered with a session runtime.
pub type RuntimeListener = Arc<dyn Fn(PiSessionRuntimeEvent) + Send + Sync>;

/// Removes a runtime event subscription when dropped.
pub struct RuntimeSubscription {
    unsubscribe: Option<Box<dyn FnOnce() + Send + Sync>>,
}

impl RuntimeSubscription {
    pub fn new(unsubscribe: Box<dyn FnOnce() + Send + Sync>) -> Self {
        Self {
            unsubscribe: Some(unsubscribe),
        }
    }

    pub fn unsubscribe(mut self) {
        if let Some(unsubscribe) = self.unsubscribe.take() {
            unsubscribe();
        }
    }
}

impl Drop for RuntimeSubscription {
    fn drop(&mut self) {
        if let Some(unsubscribe) = self.unsubscribe.take() {
            unsubscribe();
        }
    }
}

/// One acquired durable session. Conflicting operations must reject rather than queue.
pub trait PiSessionRuntime: Send + Sync {
    fn snapshot(&self) -> BoxFuture<'_, ServiceResult<SessionSnapshot>>;
    fn get_phase(&self) -> SessionPhase;
    fn prompt(&self, input: PromptInput) -> BoxFuture<'_, ServiceResult<()>>;
    fn steer(&self, input: SteerInput) -> BoxFuture<'_, ServiceResult<()>>;
    fn abort(&self) -> BoxFuture<'_, ServiceResult<()>>;
    fn set_model(&self, model: ModelRef) -> BoxFuture<'_, ServiceResult<()>>;
    fn set_thinking(&self, thinking_level: ThinkingLevel) -> BoxFuture<'_, ServiceResult<()>>;
    fn subscribe(&self, listener: RuntimeListener) -> RuntimeSubscription;
    fn dispose(&self) -> BoxFuture<'_, ServiceResult<()>>;
}

/// Service boundary for durable sessions and exclusively acquired runtimes.
pub trait PiServerService: Send + Sync {
    fn list_sessions(&self) -> BoxFuture<'_, ServiceResult<Vec<SessionMetadata>>>;
    fn list_models(&self) -> BoxFuture<'_, ServiceResult<Vec<ModelMetadata>>>;
    fn create_session(&self, options: CreateSessionOptions) -> BoxFuture<'_, ServiceResult<Arc<dyn PiSessionRuntime>>>;
    fn open_session(&self, session_id: String) -> BoxFuture<'_, ServiceResult<Arc<dyn PiSessionRuntime>>>;
}

/// Creates the watch channel used to wake requests that arrive during a handshake.
pub(crate) fn handshake_signal() -> (watch::Sender<bool>, watch::Receiver<bool>) {
    watch::channel(false)
}
