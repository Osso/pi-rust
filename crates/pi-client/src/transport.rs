use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::errors::PiError;

/// Completion of one transport write.
pub type TransportSendFuture = Pin<Box<dyn Future<Output = Result<(), PiError>> + Send>>;

/// Completion of one transport connection attempt.
pub type TransportFuture = Pin<Box<dyn Future<Output = Result<Arc<dyn ByteTransport>, PiError>> + Send>>;

/// Sends framed byte chunks to a connected server.
pub trait ByteTransport: Send + Sync {
    /// Enqueues one byte chunk. Calls must be delivered in invocation order.
    fn send(&self, chunk: Vec<u8>) -> TransportSendFuture;
    /// Closes the transport. Repeated calls must be harmless.
    fn close(&self);
}

/// Receives transport events.
#[derive(Clone)]
pub struct ByteTransportHandlers {
    /// Delivers an arbitrary inbound byte chunk.
    pub on_data: Arc<dyn Fn(Vec<u8>) + Send + Sync>,
    /// Reports an orderly terminal close.
    pub on_close: Arc<dyn Fn() + Send + Sync>,
    /// Reports a terminal transport failure.
    pub on_error: Arc<dyn Fn(PiError) + Send + Sync>,
}

/// Creates a fresh connected, authenticated transport.
pub trait ByteTransportFactory: Send + Sync {
    fn create(&self, handlers: ByteTransportHandlers) -> TransportFuture;
}

impl<F> ByteTransportFactory for F
where
    F: Fn(ByteTransportHandlers) -> TransportFuture + Send + Sync,
{
    fn create(&self, handlers: ByteTransportHandlers) -> TransportFuture {
        self(handlers)
    }
}
