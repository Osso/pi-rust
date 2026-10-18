use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use pi_client::{
    ByteTransport, ByteTransportHandlers, PiClient, PiClientOptions, PiError, SessionHandle, TransportFuture,
    TransportSendFuture,
};
use pi_protocol::{
    ClientMessage, ClientMessageDecoder, CommandResult, DetachResult, EventEnvelope, ModelRef, RequestEnvelope,
    ServerEvent, ServerHello, ServerMessage, ServerSnapshot, SessionPhase, SessionSnapshot, SuccessResponse,
    ThinkingLevel, TranscriptDeltaKind, TranscriptProgress, encode_server_message,
};

pub const PROTOCOL_VERSION: u64 = pi_protocol::PROTOCOL_VERSION;

pub struct MemoryByteServer {
    inner: Mutex<ServerInner>,
}

type MessageListener = Arc<dyn Fn(&ClientMessage) + Send + Sync>;

struct ServerInner {
    handlers: Option<ByteTransportHandlers>,
    decoder: ClientMessageDecoder,
    message_listeners: Vec<MessageListener>,
    sent_by_client: Vec<Vec<u8>>,
    client_close_count: usize,
}

pub struct MemoryTransport {
    server: Arc<MemoryByteServer>,
    closed: AtomicBool,
}

impl MemoryByteServer {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(ServerInner {
                handlers: None,
                decoder: ClientMessageDecoder::new(None),
                message_listeners: Vec::new(),
                sent_by_client: Vec::new(),
                client_close_count: 0,
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
        self.inner.lock().unwrap().message_listeners.push(listener);
    }

    pub fn sent_by_client(&self) -> Vec<Vec<u8>> {
        self.inner.lock().unwrap().sent_by_client.clone()
    }

    pub fn client_close_count(&self) -> usize {
        self.inner.lock().unwrap().client_close_count
    }

    pub fn send(&self, message: ServerMessage) {
        let frame = encode_server_message(&message, None).expect("server message must encode");
        self.send_raw(frame);
    }

    pub fn send_split(&self, message: ServerMessage, split_at: usize) {
        let frame = encode_server_message(&message, None).expect("server message must encode");
        self.send_raw(frame[..split_at].to_vec());
        self.send_raw(frame[split_at..].to_vec());
    }

    pub fn send_together(&self, messages: Vec<ServerMessage>) {
        let mut chunk = Vec::new();
        for message in messages {
            chunk.extend(encode_server_message(&message, None).expect("server message must encode"));
        }
        self.send_raw(chunk);
    }

    pub fn send_raw(&self, chunk: Vec<u8>) {
        let handlers = self.inner.lock().unwrap().handlers.clone();
        if let Some(handlers) = handlers {
            (handlers.on_data)(chunk);
        }
    }

    pub fn close(&self) {
        let handlers = self.inner.lock().unwrap().handlers.clone();
        if let Some(handlers) = handlers {
            (handlers.on_close)();
        }
    }

    pub fn error(&self, error: PiError) {
        let handlers = self.inner.lock().unwrap().handlers.clone();
        if let Some(handlers) = handlers {
            (handlers.on_error)(error);
        }
    }
}

impl ByteTransport for MemoryTransport {
    fn send(&self, chunk: Vec<u8>) -> TransportSendFuture {
        let result = self.server.deliver(&self.closed, chunk);
        Box::pin(async move { result })
    }

    fn close(&self) {
        if self.closed.swap(true, Ordering::SeqCst) {
            return;
        }
        self.server.inner.lock().unwrap().client_close_count += 1;
    }
}

impl MemoryByteServer {
    fn deliver(&self, closed: &AtomicBool, chunk: Vec<u8>) -> Result<(), PiError> {
        if closed.load(Ordering::SeqCst) {
            return Err(PiError::Transport("Transport is closed".into()));
        }
        let (messages, listeners) = {
            let mut inner = self.inner.lock().unwrap();
            inner.sent_by_client.push(chunk.clone());
            let messages = inner.decoder.push(&chunk).map_err(PiError::Protocol)?;
            (messages, inner.message_listeners.clone())
        };
        for message in &messages {
            for listener in &listeners {
                listener(message);
            }
        }
        Ok(())
    }
}

pub fn transport_factory(server: &Arc<MemoryByteServer>) -> Arc<dyn pi_client::ByteTransportFactory> {
    let server = server.clone();
    Arc::new(move |handlers: ByteTransportHandlers| -> TransportFuture {
        let server = server.clone();
        Box::pin(async move { Ok(server.connect(handlers)) })
    })
}

pub fn base_server_snapshot() -> ServerSnapshot {
    ServerSnapshot::new("server-1".into(), 1, Vec::new(), Vec::new()).unwrap()
}

pub fn server_snapshot_revision(revision: u64) -> ServerSnapshot {
    ServerSnapshot::new("server-1".into(), revision, Vec::new(), Vec::new()).unwrap()
}

pub fn session_snapshot(id: &str) -> SessionSnapshot {
    session_snapshot_with(id, 1, SessionPhase::Idle, ThinkingLevel::Off, true)
}

pub fn session_snapshot_with(
    id: &str,
    revision: u64,
    phase: SessionPhase,
    thinking_level: ThinkingLevel,
    attached: bool,
) -> SessionSnapshot {
    SessionSnapshot::new(pi_protocol::SessionSnapshotInput {
        id: id.into(),
        name: None,
        cwd: "/workspace".into(),
        created_at: 1,
        updated_at: 1,
        phase,
        model: ModelRef::new("faux".into(), "model".into()).unwrap(),
        thinking_level,
        attached,
        locked: true,
        revision,
        transcript: Vec::new(),
        queued_steer: Vec::new(),
        queued_steer_count: 0,
    })
    .unwrap()
}

pub fn create_client(server: &Arc<MemoryByteServer>) -> PiClient {
    PiClient::new(PiClientOptions::new(transport_factory(server))).unwrap()
}

pub async fn connect_client(server: &Arc<MemoryByteServer>) -> PiClient {
    let client = create_client(server);
    install_hello_responder(server);
    client.connect().unwrap().await.unwrap();
    client
}

pub fn install_hello_responder(server: &Arc<MemoryByteServer>) {
    let responder = server.clone();
    server.on_message(Arc::new(move |message| {
        if let ClientMessage::Hello(_) = message {
            responder.send(server_hello(base_server_snapshot()));
        }
    }));
}

pub fn server_hello(snapshot: ServerSnapshot) -> ServerMessage {
    ServerMessage::Hello(ServerHello::new("connection-1".into(), snapshot).unwrap())
}

pub fn success_response(id: &str, result: CommandResult) -> ServerMessage {
    ServerMessage::Response(pi_protocol::ResponseEnvelope::Success(
        SuccessResponse::new(id.into(), result).unwrap(),
    ))
}

pub fn error_response(id: &str, code: pi_protocol::ProtocolErrorCode, message: &str) -> ServerMessage {
    ServerMessage::Response(pi_protocol::ResponseEnvelope::Error(
        pi_protocol::ErrorResponse::new(id.into(), pi_protocol::ProtocolError::new(code, message.into(), None))
            .unwrap(),
    ))
}

pub fn detach_result(session_id: &str) -> CommandResult {
    CommandResult::Detach(DetachResult::new(session_id.into()).unwrap())
}

pub fn session_snapshot_event(snapshot: SessionSnapshot) -> ServerMessage {
    ServerMessage::Event(EventEnvelope::new(ServerEvent::SessionSnapshot(snapshot)))
}

pub fn server_snapshot_event(snapshot: ServerSnapshot) -> ServerMessage {
    ServerMessage::Event(EventEnvelope::new(ServerEvent::ServerSnapshot(snapshot)))
}

pub fn session_progress_event(session_id: &str, message_id: &str, delta: &str) -> ServerMessage {
    let progress =
        TranscriptProgress::assistant_delta(message_id.into(), 0, TranscriptDeltaKind::Text, delta.into()).unwrap();
    ServerMessage::Event(EventEnvelope::new(ServerEvent::SessionProgress(
        pi_protocol::SessionProgress::new(session_id.into(), progress).unwrap(),
    )))
}

#[derive(Clone, Default)]
pub struct RequestLog(Arc<Mutex<Vec<RequestEnvelope>>>);

impl RequestLog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.0.lock().unwrap().len()
    }

    pub fn commands(&self) -> Vec<&'static str> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .map(|request| request.request().as_str())
            .collect()
    }

    pub fn get(&self, index: usize) -> Option<RequestEnvelope> {
        self.0.lock().unwrap().get(index).cloned()
    }

    pub fn find_command(&self, command: &str) -> Option<RequestEnvelope> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .find(|request| request.request().as_str() == command)
            .cloned()
    }

    pub fn last(&self) -> Option<RequestEnvelope> {
        self.0.lock().unwrap().last().cloned()
    }
}

pub fn collect_requests(server: &Arc<MemoryByteServer>) -> RequestLog {
    let log = RequestLog::new();
    let target = log.clone();
    server.on_message(Arc::new(move |message| {
        if let ClientMessage::Request(request) = message {
            target.0.lock().unwrap().push(request.clone());
        }
    }));
    log
}

pub async fn attach_session(
    client: &PiClient,
    server: &Arc<MemoryByteServer>,
    snapshot: SessionSnapshot,
) -> SessionHandle {
    let requests = collect_requests(server);
    let attaching = client.attach_session(snapshot.id().to_string()).unwrap();
    let request = requests.find_command("attach").expect("missing attach request");
    let id = snapshot.id().to_string();
    server.send(success_response(request.id(), CommandResult::Attach(snapshot)));
    let _ = id;
    attaching.await.unwrap()
}
