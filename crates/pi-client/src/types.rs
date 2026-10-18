use std::sync::Arc;

use pi_protocol::{ModelRef, ServerEvent, ServerSnapshot, SessionSnapshot, ThinkingLevel};

use crate::errors::PiError;
use crate::transport::ByteTransportFactory;

/// Lifecycle state of a client connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
}

impl ConnectionState {
    /// Returns the exact source literal.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Disconnected => "disconnected",
            Self::Connecting => "connecting",
            Self::Connected => "connected",
        }
    }
}

/// One connection-state transition.
#[derive(Debug, Clone, PartialEq)]
pub struct ConnectionStateChange {
    pub state: ConnectionState,
    pub error: Option<PiError>,
}

impl ConnectionStateChange {
    pub const fn new(state: ConnectionState, error: Option<PiError>) -> Self {
        Self { state, error }
    }
}

/// Removes a registered listener.
pub type Unsubscribe = Box<dyn Fn() + Send + Sync>;

/// Receives a snapshot notification.
pub type SnapshotListener = Arc<dyn Fn(&ServerSnapshot) + Send + Sync>;
/// Receives a session snapshot notification.
pub type SessionSnapshotListener = Arc<dyn Fn(&SessionSnapshot) + Send + Sync>;
/// Receives a server event notification.
pub type EventListener = Arc<dyn Fn(&ServerEvent) + Send + Sync>;
/// Receives a connection-state notification.
pub type ConnectionStateListener = Arc<dyn Fn(&ConnectionStateChange) + Send + Sync>;
/// Reports listener failures without allowing them to corrupt client state.
pub type ListenerErrorHandler = Arc<dyn Fn(PiError) + Send + Sync>;

/// Options used to construct a client.
#[derive(Clone)]
pub struct PiClientOptions {
    pub transport_factory: Arc<dyn ByteTransportFactory>,
    pub max_frame_length: Option<u32>,
    pub on_listener_error: Option<ListenerErrorHandler>,
}

impl PiClientOptions {
    pub fn new(transport_factory: Arc<dyn ByteTransportFactory>) -> Self {
        Self {
            transport_factory,
            max_frame_length: None,
            on_listener_error: None,
        }
    }

    pub fn with_transport_factory(mut self, transport_factory: Arc<dyn ByteTransportFactory>) -> Self {
        self.transport_factory = transport_factory;
        self
    }

    pub const fn with_max_frame_length(mut self, max_frame_length: u32) -> Self {
        self.max_frame_length = Some(max_frame_length);
        self
    }

    pub fn with_listener_error(mut self, handler: ListenerErrorHandler) -> Self {
        self.on_listener_error = Some(handler);
        self
    }
}

/// Optional values used when creating a session.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CreateSessionOptions {
    pub cwd: Option<String>,
    pub name: Option<String>,
    pub model: Option<ModelRef>,
    pub thinking_level: Option<ThinkingLevel>,
}
