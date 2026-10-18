//! In-memory protocol server for `RemoteSession` behavioral tests.
//!
//! Mirrors `packages/coding-agent/test/client/support.ts` on top of the Rust client/protocol types.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use pi_client::{
    ByteTransport, ByteTransportHandlers, PiClient, PiClientOptions, PiError, TransportFuture, TransportSendFuture,
};
use pi_protocol::{
    ClientMessage, ClientMessageDecoder, Command, CommandResult, EventEnvelope, ModelRef, ServerEvent, ServerHello,
    ServerMessage, ServerSnapshot, SessionPhase, SessionSnapshot, SessionSnapshotInput, SuccessResponse, ThinkingLevel,
    TranscriptItem, encode_server_message,
};

pub struct MemoryServer {
    inner: Mutex<ServerInner>,
}

struct ServerInner {
    handlers: Option<ByteTransportHandlers>,
    decoder: ClientMessageDecoder,
    listeners: Vec<Arc<dyn Fn(&ClientMessage) + Send + Sync>>,
}

struct MemoryTransport {
    server: Arc<MemoryServer>,
    closed: AtomicBool,
}

impl MemoryServer {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(ServerInner {
                handlers: None,
                decoder: ClientMessageDecoder::new(None),
                listeners: Vec::new(),
            }),
        })
    }

    pub fn connect(self: &Arc<Self>, handlers: ByteTransportHandlers) -> Arc<dyn ByteTransport> {
        self.inner.lock().unwrap().handlers = Some(handlers);
        Arc::new(MemoryTransport {
            server: self.clone(),
            closed: AtomicBool::new(false),
        })
    }

    pub fn on_message(&self, listener: Arc<dyn Fn(&ClientMessage) + Send + Sync>) {
        self.inner.lock().unwrap().listeners.push(listener);
    }

    pub fn send(&self, message: ServerMessage) {
        let frame = encode_server_message(&message, None).expect("server message encodes");
        let handlers = self.inner.lock().unwrap().handlers.clone();
        if let Some(handlers) = handlers {
            (handlers.on_data)(frame);
        }
    }

    pub fn send_result(&self, id: &str, result: CommandResult) {
        self.send(ServerMessage::Response(pi_protocol::ResponseEnvelope::Success(
            SuccessResponse::new(id.to_owned(), result).expect("success response"),
        )));
    }

    pub fn send_event(&self, event: ServerEvent) {
        self.send(ServerMessage::Event(EventEnvelope::new(event)));
    }

    fn deliver(&self, chunk: Vec<u8>) {
        let messages = {
            let mut inner = self.inner.lock().unwrap();
            inner.decoder.push(&chunk).expect("client frame decodes")
        };
        let listeners = self.inner.lock().unwrap().listeners.clone();
        for message in messages {
            for listener in &listeners {
                listener(&message);
            }
        }
    }
}

impl ByteTransport for MemoryTransport {
    fn send(&self, chunk: Vec<u8>) -> TransportSendFuture {
        if self.closed.load(Ordering::SeqCst) {
            return Box::pin(async { Err(PiError::Transport("Transport is closed".to_owned())) });
        }
        self.server.deliver(chunk);
        Box::pin(async { Ok(()) })
    }

    fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
    }
}

pub fn server_snapshot() -> ServerSnapshot {
    ServerSnapshot::new("server-1".to_owned(), 1, Vec::new(), Vec::new()).expect("server snapshot")
}

pub fn session_snapshot(id: &str, overrides: SnapshotOverrides) -> SessionSnapshot {
    SessionSnapshot::new(SessionSnapshotInput {
        id: id.to_owned(),
        name: None,
        cwd: "/workspace".to_owned(),
        created_at: 1,
        updated_at: 1,
        phase: overrides.phase.unwrap_or(SessionPhase::Idle),
        model: ModelRef::new("faux".to_owned(), "model".to_owned()).expect("model"),
        thinking_level: ThinkingLevel::Off,
        attached: true,
        locked: true,
        revision: overrides.revision.unwrap_or(1),
        transcript: overrides.transcript.unwrap_or_default(),
        queued_steer: Vec::new(),
        queued_steer_count: 0,
    })
    .expect("session snapshot")
}

#[derive(Default)]
pub struct SnapshotOverrides {
    pub phase: Option<SessionPhase>,
    pub revision: Option<u64>,
    pub transcript: Option<Vec<TranscriptItem>>,
}

pub async fn connect_client(server: &Arc<MemoryServer>) -> PiClient {
    let factory_server = server.clone();
    let client = PiClient::new(PiClientOptions {
        transport_factory: Arc::new(move |handlers| {
            let server = factory_server.clone();
            Box::pin(async move { Ok(server.connect(handlers)) }) as TransportFuture
        }),
        max_frame_length: None,
        on_listener_error: None,
    })
    .expect("client");
    let hello_server = server.clone();
    server.on_message(Arc::new(move |message| {
        if matches!(message, ClientMessage::Hello(_)) {
            hello_server.send(ServerMessage::Hello(
                ServerHello::new("connection-1".to_owned(), server_snapshot()).expect("hello"),
            ));
        }
    }));
    client
        .connect()
        .expect("connect starts")
        .await
        .expect("connect completes");
    client
}

#[derive(Clone)]
pub struct CollectedRequest {
    pub id: String,
    pub command: Command,
}

pub fn collect_requests(server: &Arc<MemoryServer>) -> Arc<Mutex<Vec<CollectedRequest>>> {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let recorded = requests.clone();
    server.on_message(Arc::new(move |message| {
        if let ClientMessage::Request(envelope) = message {
            recorded.lock().unwrap().push(CollectedRequest {
                id: envelope.id().to_owned(),
                command: envelope.request().clone(),
            });
        }
    }));
    requests
}

pub async fn wait_for_request(requests: &Arc<Mutex<Vec<CollectedRequest>>>, command: &str) -> CollectedRequest {
    for _ in 0..10_000 {
        if let Some(request) = requests
            .lock()
            .unwrap()
            .iter()
            .find(|request| request.command.as_str() == command)
            .cloned()
        {
            return request;
        }
        tokio::task::yield_now().await;
    }
    panic!("missing {command} request");
}
