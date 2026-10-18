use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use pi_protocol::{Command, CommandResult, ModelRef, SessionSnapshot, ThinkingLevel};

use crate::client::CommandResponse;
use crate::errors::PiError;
use crate::types::{EventListener, SessionSnapshotListener, Unsubscribe};

/// How a caller shares a session attachment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionLeaseMode {
    Shared,
    Exclusive,
}

/// Options used to acquire a session lease.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AcquireSessionOptions {
    pub mode: SessionLeaseMode,
}

impl AcquireSessionOptions {
    pub const fn new(mode: SessionLeaseMode) -> Self {
        Self { mode }
    }
}

impl From<SessionLeaseMode> for AcquireSessionOptions {
    fn from(mode: SessionLeaseMode) -> Self {
        Self { mode }
    }
}

/// Completion of a lease release.
pub struct ReleaseFuture {
    inner: Pin<Box<dyn Future<Output = Result<(), PiError>> + Send>>,
}

impl ReleaseFuture {
    pub(crate) fn new(operation: Arc<crate::client::SharedOperation>) -> Self {
        Self {
            inner: Box::pin(async move { operation.wait().await }),
        }
    }

    pub(crate) fn ready(result: Result<(), PiError>) -> Self {
        Self {
            inner: Box::pin(async move { result }),
        }
    }
}

impl Future for ReleaseFuture {
    type Output = Result<(), PiError>;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        self.inner.as_mut().poll(context)
    }
}

/// Completion of a session command that returns a snapshot.
pub struct SnapshotFuture {
    response: CommandResponse,
}

impl SnapshotFuture {
    pub(crate) const fn new(response: CommandResponse) -> Self {
        Self { response }
    }
}

impl Future for SnapshotFuture {
    type Output = Result<SessionSnapshot, PiError>;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        match Pin::new(&mut self.response).poll(context) {
            Poll::Ready(result) => Poll::Ready(result.and_then(snapshot_from_result)),
            Poll::Pending => Poll::Pending,
        }
    }
}

fn snapshot_from_result(result: CommandResult) -> Result<SessionSnapshot, PiError> {
    match result {
        CommandResult::Create(snapshot)
        | CommandResult::Attach(snapshot)
        | CommandResult::Prompt(snapshot)
        | CommandResult::Steer(snapshot)
        | CommandResult::Abort(snapshot)
        | CommandResult::SetModel(snapshot)
        | CommandResult::SetThinking(snapshot) => Ok(snapshot),
        CommandResult::List(_) | CommandResult::Detach(_) => Err(PiError::Protocol(
            pi_protocol::ProtocolValidationError::new("Response does not contain a session snapshot"),
        )),
    }
}

pub(crate) trait SessionHandleCallbacks: Send + Sync {
    fn is_attached(&self) -> bool;
    fn snapshot(&self) -> Option<SessionSnapshot>;
    fn subscribe(&self, listener: SessionSnapshotListener) -> Result<Unsubscribe, PiError>;
    fn on_event(&self, listener: EventListener) -> Result<Unsubscribe, PiError>;
    fn detach(&self) -> Result<ReleaseFuture, PiError>;
    fn dispose(&self) -> Result<ReleaseFuture, PiError>;
    fn request(&self, command: Command) -> Result<CommandResponse, PiError>;
}

/// A leased handle to one attached session.
#[derive(Clone)]
pub struct SessionHandle {
    id: String,
    callbacks: Arc<dyn SessionHandleCallbacks>,
}

impl std::fmt::Debug for SessionHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SessionHandle")
            .field("id", &self.id)
            .field("attached", &self.attached())
            .finish()
    }
}

impl SessionHandle {
    pub(crate) fn new(id: String, callbacks: Arc<dyn SessionHandleCallbacks>) -> Self {
        Self { id, callbacks }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn attached(&self) -> bool {
        self.callbacks.is_attached()
    }

    pub fn active(&self) -> bool {
        self.attached()
    }

    pub fn snapshot(&self) -> Option<SessionSnapshot> {
        self.callbacks.snapshot()
    }

    pub fn subscribe(&self, listener: SessionSnapshotListener) -> Result<Unsubscribe, PiError> {
        self.callbacks.subscribe(listener)
    }

    pub fn on_event(&self, listener: EventListener) -> Result<Unsubscribe, PiError> {
        self.callbacks.on_event(listener)
    }

    pub fn detach(&self) -> Result<ReleaseFuture, PiError> {
        self.callbacks.detach()
    }

    pub fn dispose(&self) -> Result<ReleaseFuture, PiError> {
        self.callbacks.dispose()
    }

    pub fn prompt(&self, text: impl Into<String>) -> Result<SnapshotFuture, PiError> {
        let command = Command::Prompt(
            pi_protocol::PromptCommand::new(self.id.clone(), text.into())
                .map_err(|error| PiError::InvalidArgument(error.to_string()))?,
        );
        self.request_snapshot(command)
    }

    pub fn steer(&self, text: impl Into<String>) -> Result<SnapshotFuture, PiError> {
        let command = Command::Steer(
            pi_protocol::SteerCommand::new(self.id.clone(), text.into())
                .map_err(|error| PiError::InvalidArgument(error.to_string()))?,
        );
        self.request_snapshot(command)
    }

    pub fn abort(&self) -> Result<SnapshotFuture, PiError> {
        let command = Command::Abort(
            pi_protocol::AbortCommand::new(self.id.clone())
                .map_err(|error| PiError::InvalidArgument(error.to_string()))?,
        );
        self.request_snapshot(command)
    }

    pub fn set_model(&self, model: ModelRef) -> Result<SnapshotFuture, PiError> {
        let command = Command::SetModel(
            pi_protocol::SetModelCommand::new(self.id.clone(), model)
                .map_err(|error| PiError::InvalidArgument(error.to_string()))?,
        );
        self.request_snapshot(command)
    }

    pub fn set_thinking(&self, thinking_level: ThinkingLevel) -> Result<SnapshotFuture, PiError> {
        let command = Command::SetThinking(
            pi_protocol::SetThinkingCommand::new(self.id.clone(), thinking_level)
                .map_err(|error| PiError::InvalidArgument(error.to_string()))?,
        );
        self.request_snapshot(command)
    }

    fn request_snapshot(&self, command: Command) -> Result<SnapshotFuture, PiError> {
        self.callbacks.request(command).map(SnapshotFuture::new)
    }
}
