use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use pi_protocol::ClientMessageDecoder;
use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::types::BoxFuture;

pub type ConnectionResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

/// An established, authorized ordered byte connection.
pub trait ByteConnection: Send + Sync {
    fn closed(&self) -> bool;
    fn send(&self, chunk: Vec<u8>) -> BoxFuture<'_, ConnectionResult>;
    fn close(&self, final_chunk: Option<Vec<u8>>) -> BoxFuture<'_, ConnectionResult>;
}

pub trait ByteConnectionHandler: Send {
    fn on_data(&mut self, chunk: &[u8]);
    fn on_close(&mut self);
    fn on_error(&mut self, error: std::io::Error);
}

pub type ByteConnectionAcceptor = Arc<dyn Fn(Arc<dyn ByteConnection>) -> Box<dyn ByteConnectionHandler> + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionStage {
    AwaitingHello,
    Handshaking,
    Ready,
    Closing,
    Closed,
}

impl ConnectionStage {
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Closing | Self::Closed)
    }
}

/// State retained for one accepted connection.
pub struct ConnectionState {
    pub id: String,
    pub connection: Arc<dyn ByteConnection>,
    pub decoder: Mutex<ClientMessageDecoder>,
    pub session_ids: Mutex<BTreeSet<String>>,
    stage: Mutex<ConnectionStage>,
    pub disconnected: AtomicBool,
    pub handshake_complete: AtomicBool,
    handshake_timeout: Mutex<Option<JoinHandle<()>>>,
    handshake_signal: watch::Sender<bool>,
}

impl ConnectionState {
    pub fn new(
        id: String,
        connection: Arc<dyn ByteConnection>,
        max_frame_length: Option<u32>,
        handshake_signal: watch::Sender<bool>,
    ) -> Self {
        Self {
            id,
            connection,
            decoder: Mutex::new(ClientMessageDecoder::new(max_frame_length)),
            session_ids: Mutex::new(BTreeSet::new()),
            stage: Mutex::new(ConnectionStage::AwaitingHello),
            disconnected: AtomicBool::new(false),
            handshake_complete: AtomicBool::new(false),
            handshake_timeout: Mutex::new(None),
            handshake_signal,
        }
    }

    pub fn stage(&self) -> ConnectionStage {
        *self.stage.lock().expect("connection stage mutex poisoned")
    }

    pub fn set_stage(&self, stage: ConnectionStage) {
        *self.stage.lock().expect("connection stage mutex poisoned") = stage;
    }

    pub fn is_terminal(&self) -> bool {
        self.disconnected.load(Ordering::SeqCst) || self.stage().is_terminal()
    }

    pub fn set_handshake_timeout(&self, handle: JoinHandle<()>) {
        let mut timeout = self.handshake_timeout.lock().expect("handshake timeout mutex poisoned");
        if let Some(previous) = timeout.replace(handle) {
            previous.abort();
        }
    }

    pub fn clear_handshake_timeout(&self) {
        if let Some(handle) = self
            .handshake_timeout
            .lock()
            .expect("handshake timeout mutex poisoned")
            .take()
        {
            handle.abort();
        }
    }

    pub fn handshake_receiver(&self) -> watch::Receiver<bool> {
        self.handshake_signal.subscribe()
    }

    pub fn signal_handshake_finished(&self) {
        let _ = self.handshake_signal.send(true);
    }
}

pub fn is_terminal_connection(state: &ConnectionState) -> bool {
    state.is_terminal()
}
