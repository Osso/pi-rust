/// Current protocol version used by compatible clients and servers.
pub const PROTOCOL_VERSION: u64 = 1;

/// First protocol message sent by a client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientHello {
    version: u64,
}

impl ClientHello {
    /// Creates a client hello with a non-negative protocol version.
    pub const fn new(version: u64) -> Self {
        Self { version }
    }

    /// Client-proposed protocol version.
    pub const fn version(self) -> u64 {
        self.version
    }
}
