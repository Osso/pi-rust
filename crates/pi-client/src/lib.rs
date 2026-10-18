//! Transport-neutral Pi protocol client.

mod client;
mod connection;
mod errors;
mod promise;
mod session_handle;
mod state;
mod transport;
mod types;
pub mod unix;

pub use client::{AttachFuture, CommandResponse, ConnectFuture, CreateSessionFuture, DisposeFuture, PiClient};
pub use errors::{
    PiClientDisposedError, PiDisconnectedError, PiError, PiServerError, PiSessionDetachedError,
    PiSessionOwnershipError, to_disconnected_error,
};
pub use session_handle::{AcquireSessionOptions, ReleaseFuture, SessionHandle, SessionLeaseMode, SnapshotFuture};
pub use transport::{ByteTransport, ByteTransportFactory, ByteTransportHandlers, TransportFuture, TransportSendFuture};
pub use types::{
    ConnectionState, ConnectionStateChange, ConnectionStateListener, CreateSessionOptions, EventListener,
    ListenerErrorHandler, PiClientOptions, SessionSnapshotListener, SnapshotListener, Unsubscribe,
};
pub use unix::{UnixTransportOptions, create_unix_transport_factory};
