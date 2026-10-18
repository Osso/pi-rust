use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::sync::Arc;

use crate::server::{PiServer, PiServerConfigError};
use crate::transports::unix::listener::{UnixListenerConfigError, create_unix_listener};
use crate::transports::unix::types::UnixServerOptions;
use crate::types::PiServerService;

/// Returned when Unix server composition fails validation.
#[derive(Debug)]
pub enum UnixServerConfigError {
    Listener(UnixListenerConfigError),
    Server(PiServerConfigError),
}

impl Display for UnixServerConfigError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Listener(error) => Display::fmt(error, formatter),
            Self::Server(error) => Display::fmt(error, formatter),
        }
    }
}

impl Error for UnixServerConfigError {}

/// Compose [`PiServer`] with one Unix-domain socket listener.
pub fn create_unix_server(
    service: Arc<dyn PiServerService>,
    options: UnixServerOptions,
) -> Result<PiServer, UnixServerConfigError> {
    let listener = create_unix_listener(options.listener_options()).map_err(UnixServerConfigError::Listener)?;
    let mut server_options = options.server_options();
    server_options.listeners = vec![listener];
    PiServer::new(service, server_options).map_err(UnixServerConfigError::Server)
}
