use std::error::Error;

use crate::connection::ByteConnectionAcceptor;
use crate::types::BoxFuture;

pub type ListenerError = Box<dyn Error + Send + Sync>;
pub type ListenerResult = Result<(), ListenerError>;

/// Supplies established byte connections after any required transport authentication.
pub trait PiServerListener: Send + Sync {
    /// Human-readable bound address after startup, when the transport has one.
    fn address(&self) -> Option<String>;
    /// Starts listening and passes authorized connections to `accept`.
    fn start(&self, accept: ByteConnectionAcceptor) -> BoxFuture<'_, ListenerResult>;
    fn close(&self) -> BoxFuture<'_, ListenerResult>;
}
