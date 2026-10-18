use std::sync::{Arc, Mutex};

use pi_protocol::{
    ClientHello, ClientMessage, PROTOCOL_VERSION, ProtocolValidationError, ServerMessage, ServerMessageDecoder,
    ServerSnapshot, encode_client_message,
};

use crate::errors::{PiDisconnectedError, PiError, PiServerError, to_disconnected_error};
use crate::promise::{Promise, Resolver, create_promise_resolvers};
use crate::transport::{ByteTransport, ByteTransportFactory, ByteTransportHandlers};
use crate::types::{ConnectionState, ConnectionStateChange};
/// Receives connection lifecycle callbacks.
pub(crate) trait ConnectionObserver: Send + Sync {
    fn on_handshake(&self, snapshot: &ServerSnapshot);
    fn on_message(&self, message: &ServerMessage);
    fn on_state_change(&self, change: &ConnectionStateChange);
}

enum Lifecycle {
    Disconnected,
    Connecting {
        id: u64,
        decoder: Option<ServerMessageDecoder>,
        handshake: Option<Resolver<ServerSnapshot>>,
        transport: Option<Arc<dyn ByteTransport>>,
    },
    Connected {
        id: u64,
        decoder: Option<ServerMessageDecoder>,
        handshake: Option<Resolver<ServerSnapshot>>,
        transport: Arc<dyn ByteTransport>,
    },
}

impl Lifecycle {
    const fn state(&self) -> ConnectionState {
        match self {
            Self::Disconnected => ConnectionState::Disconnected,
            Self::Connecting { .. } => ConnectionState::Connecting,
            Self::Connected { .. } => ConnectionState::Connected,
        }
    }
}

struct ConnectionInner {
    factory: Arc<dyn ByteTransportFactory>,
    max_frame_length: u32,
    observer: Arc<dyn ConnectionObserver>,
    lifecycle: Lifecycle,
    sequence: u64,
}

/// One framed protocol connection with handshake lifecycle management.
#[derive(Clone)]
pub struct Connection {
    inner: Arc<Mutex<ConnectionInner>>,
}

impl Connection {
    pub(crate) fn new(
        factory: Arc<dyn ByteTransportFactory>,
        max_frame_length: u32,
        observer: Arc<dyn ConnectionObserver>,
    ) -> Self {
        Self {
            inner: Arc::new(Mutex::new(ConnectionInner {
                factory,
                max_frame_length,
                observer,
                lifecycle: Lifecycle::Disconnected,
                sequence: 0,
            })),
        }
    }

    pub(crate) fn state(&self) -> ConnectionState {
        self.inner.lock().unwrap().lifecycle.state()
    }

    pub(crate) fn max_frame_length(&self) -> u32 {
        self.inner.lock().unwrap().max_frame_length
    }

    pub(crate) fn connect(&self) -> Result<Promise<ServerSnapshot>, PiError> {
        let (id, promise) = {
            let mut inner = self.inner.lock().unwrap();
            if inner.lifecycle.state() != ConnectionState::Disconnected {
                return Err(PiError::Disconnected(PiDisconnectedError::new(format!(
                    "PiClient is already {}",
                    inner.lifecycle.state().as_str()
                ))));
            }
            inner.sequence += 1;
            let id = inner.sequence;
            let (resolver, promise) = create_promise_resolvers();
            inner.lifecycle = Lifecycle::Connecting {
                id,
                decoder: Some(ServerMessageDecoder::new(Some(inner.max_frame_length))),
                handshake: Some(resolver),
                transport: None,
            };
            (id, promise)
        };
        self.observer()
            .on_state_change(&ConnectionStateChange::new(ConnectionState::Connecting, None));
        let handlers = self.handlers_for(id);
        let connection = self.clone();
        tokio::spawn(async move {
            connection.open_transport(id, handlers).await;
        });
        Ok(promise)
    }

    pub(crate) fn disconnect(&self, error: PiError) {
        self.fail_and_close(error);
    }

    pub(crate) fn fail(&self, error: PiError) {
        self.fail_with(error, false);
    }

    pub(crate) fn send(&self, frame: Vec<u8>) -> Result<(), PiError> {
        let (id, transport) = {
            let inner = self.inner.lock().unwrap();
            match &inner.lifecycle {
                Lifecycle::Connected { id, transport, .. } => (*id, transport.clone()),
                _ => return Err(PiError::Disconnected(PiDisconnectedError::default())),
            }
        };
        let sending = transport.send(frame);
        let connection = self.clone();
        tokio::spawn(async move {
            if let Err(error) = sending.await {
                connection.fail_after_send_error(id, transport, to_disconnected_error(error));
            }
        });
        Ok(())
    }

    fn handlers_for(&self, id: u64) -> ByteTransportHandlers {
        let data_connection = self.clone();
        let close_connection = self.clone();
        let error_connection = self.clone();
        ByteTransportHandlers {
            on_data: Arc::new(move |chunk| data_connection.handle_data(id, chunk)),
            on_close: Arc::new(move || {
                if close_connection.is_current(id) {
                    close_connection.handle_close(id);
                }
            }),
            on_error: Arc::new(move |error| {
                if error_connection.is_current(id) {
                    error_connection.fail_and_close(to_disconnected_error(error));
                }
            }),
        }
    }

    async fn open_transport(&self, id: u64, handlers: ByteTransportHandlers) {
        let factory = self.inner.lock().unwrap().factory.clone();
        let transport = match factory.create(handlers).await {
            Ok(transport) => transport,
            Err(error) => {
                if self.is_current(id) {
                    self.fail(to_disconnected_error(error));
                }
                return;
            }
        };
        let accepted = {
            let mut inner = self.inner.lock().unwrap();
            match &mut inner.lifecycle {
                Lifecycle::Connecting {
                    id: current,
                    transport: slot,
                    ..
                } if *current == id => {
                    *slot = Some(transport.clone());
                    true
                }
                _ => false,
            }
        };
        if !accepted {
            transport.close();
            return;
        }
        let frame = match encode_client_message(
            &ClientMessage::Hello(ClientHello::new(PROTOCOL_VERSION)),
            Some(self.max_frame_length()),
        ) {
            Ok(frame) => frame,
            Err(error) => {
                if self.is_current(id) {
                    self.fail_and_close(PiError::Protocol(error));
                }
                return;
            }
        };
        if let Err(error) = transport.send(frame).await
            && self.is_current(id)
        {
            self.fail_and_close(to_disconnected_error(error));
        }
    }

    fn handle_data(&self, id: u64, chunk: Vec<u8>) {
        let decoded = {
            let mut inner = self.inner.lock().unwrap();
            match &mut inner.lifecycle {
                Lifecycle::Disconnected => return,
                Lifecycle::Connecting {
                    id: current,
                    decoder,
                    transport,
                    ..
                } => {
                    if *current != id {
                        return;
                    }
                    if transport.is_none() {
                        Err(PiError::Protocol(ProtocolValidationError::new(
                            "Received server data before the client hello was sent",
                        )))
                    } else {
                        decoder
                            .as_mut()
                            .expect("connecting lifecycle decoder")
                            .push(&chunk)
                            .map_err(PiError::Protocol)
                    }
                }
                Lifecycle::Connected {
                    id: current, decoder, ..
                } => {
                    if *current != id {
                        return;
                    }
                    decoder
                        .as_mut()
                        .expect("connected lifecycle decoder")
                        .push(&chunk)
                        .map_err(PiError::Protocol)
                }
            }
        };
        match decoded {
            Err(error) => self.fail_and_close(error),
            Ok(messages) => {
                for message in messages {
                    if self.state() == ConnectionState::Disconnected {
                        return;
                    }
                    self.handle_message(message);
                }
            }
        }
    }

    fn handle_message(&self, message: ServerMessage) {
        enum Action {
            FailAndClose(PiError),
            PrepareHandshake { id: u64, snapshot: ServerSnapshot },
            Notify(Box<ServerMessage>),
            Ignore,
        }

        let action = {
            let inner = self.inner.lock().unwrap();
            match &inner.lifecycle {
                Lifecycle::Disconnected => Action::Ignore,
                Lifecycle::Connecting { transport, .. } => match message {
                    ServerMessage::HelloError(error) => {
                        Action::FailAndClose(PiError::Server(PiServerError::new(error.error().clone())))
                    }
                    ServerMessage::Hello(hello) => {
                        if transport.is_none() {
                            Action::FailAndClose(PiError::Protocol(ProtocolValidationError::new(
                                "Received server hello before the client hello was sent",
                            )))
                        } else {
                            let id = match &inner.lifecycle {
                                Lifecycle::Connecting { id, .. } => *id,
                                _ => unreachable!("lifecycle checked as connecting"),
                            };
                            Action::PrepareHandshake {
                                id,
                                snapshot: hello.snapshot().clone(),
                            }
                        }
                    }
                    _ => Action::FailAndClose(PiError::Protocol(ProtocolValidationError::new(
                        "Expected server hello as first message",
                    ))),
                },
                Lifecycle::Connected { .. } => match message {
                    ServerMessage::Hello(_) | ServerMessage::HelloError(_) => Action::FailAndClose(PiError::Protocol(
                        ProtocolValidationError::new("Unexpected handshake message"),
                    )),
                    other => Action::Notify(Box::new(other)),
                },
            }
        };

        match action {
            Action::Ignore => {}
            Action::FailAndClose(error) => self.fail_and_close(error),
            Action::Notify(message) => self.observer().on_message(&message),
            Action::PrepareHandshake { id, snapshot } => {
                let transitioned = {
                    let mut inner = self.inner.lock().unwrap();
                    match std::mem::replace(&mut inner.lifecycle, Lifecycle::Disconnected) {
                        Lifecycle::Connecting {
                            id: current,
                            decoder,
                            handshake,
                            transport: Some(transport),
                        } if current == id => {
                            inner.lifecycle = Lifecycle::Connected {
                                id,
                                decoder,
                                handshake,
                                transport,
                            };
                            true
                        }
                        other => {
                            inner.lifecycle = other;
                            false
                        }
                    }
                };
                if !transitioned {
                    return;
                }
                self.observer().on_handshake(&snapshot);
                if !self.is_connected(id) {
                    return;
                }
                self.observer()
                    .on_state_change(&ConnectionStateChange::new(ConnectionState::Connected, None));
                if !self.is_connected(id) {
                    return;
                }
                let handshake = {
                    let mut inner = self.inner.lock().unwrap();
                    match &mut inner.lifecycle {
                        Lifecycle::Connected {
                            id: current, handshake, ..
                        } if *current == id => handshake.take(),
                        _ => None,
                    }
                };
                if let Some(handshake) = handshake {
                    handshake.resolve(snapshot);
                }
            }
        }
    }

    fn handle_close(&self, id: u64) {
        let decoder = {
            let mut inner = self.inner.lock().unwrap();
            match &mut inner.lifecycle {
                Lifecycle::Disconnected => return,
                Lifecycle::Connecting {
                    id: current, decoder, ..
                }
                | Lifecycle::Connected {
                    id: current, decoder, ..
                } if *current == id => decoder.take(),
                _ => return,
            }
        };
        let error = match decoder.and_then(|mut decoder| decoder.end().err()) {
            Some(error) => PiError::Protocol(error),
            None => PiError::Disconnected(PiDisconnectedError::new("Byte transport closed")),
        };
        self.fail(error);
    }

    fn fail_and_close(&self, error: PiError) {
        self.fail_with(error, true);
    }

    fn fail_with(&self, error: PiError, close_transport: bool) {
        let (handshake, transport) = {
            let mut inner = self.inner.lock().unwrap();
            if matches!(inner.lifecycle, Lifecycle::Disconnected) {
                return;
            }
            let lifecycle = std::mem::replace(&mut inner.lifecycle, Lifecycle::Disconnected);
            let (handshake, transport) = match lifecycle {
                Lifecycle::Disconnected => (None, None),
                Lifecycle::Connecting {
                    handshake, transport, ..
                } => (handshake, transport),
                Lifecycle::Connected {
                    handshake, transport, ..
                } => (handshake, Some(transport)),
            };
            (handshake, transport)
        };
        if let Some(handshake) = handshake {
            handshake.reject(error.clone());
        }
        self.observer()
            .on_state_change(&ConnectionStateChange::new(ConnectionState::Disconnected, Some(error)));
        if close_transport && let Some(transport) = transport {
            transport.close();
        }
    }

    fn fail_after_send_error(&self, id: u64, transport: Arc<dyn ByteTransport>, error: PiError) {
        let should_fail = {
            let inner = self.inner.lock().unwrap();
            match &inner.lifecycle {
                Lifecycle::Disconnected => false,
                Lifecycle::Connected {
                    id: current,
                    transport: current_transport,
                    ..
                } => *current == id && Arc::ptr_eq(current_transport, &transport),
                Lifecycle::Connecting { .. } => false,
            }
        };
        if should_fail {
            self.fail_and_close(error);
        }
    }

    fn observer(&self) -> Arc<dyn ConnectionObserver> {
        self.inner.lock().unwrap().observer.clone()
    }

    fn is_current(&self, id: u64) -> bool {
        let inner = self.inner.lock().unwrap();
        match &inner.lifecycle {
            Lifecycle::Disconnected => false,
            Lifecycle::Connecting { id: current, .. } | Lifecycle::Connected { id: current, .. } => *current == id,
        }
    }

    fn is_connected(&self, id: u64) -> bool {
        let inner = self.inner.lock().unwrap();
        matches!(&inner.lifecycle, Lifecycle::Connected { id: current, .. } if *current == id)
    }
}
