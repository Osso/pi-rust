use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use pi_protocol::{
    ClientMessage, Command, PROTOCOL_VERSION, RequestEnvelope, ResponseEnvelope, ServerMessage, ServerMessageDecoder,
    encode_client_message,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::types::BoxFuture;

pub type ClientResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

struct ClientInner {
    messages: Mutex<Vec<ServerMessage>>,
    changed: watch::Sender<u64>,
    closed: AtomicBool,
}

/// Wire-level protocol client used by transport conformance tests.
pub struct ProtocolTestClient {
    inner: Arc<ClientInner>,
    writer: Arc<tokio::sync::Mutex<OwnedWriteHalf>>,
    reader: JoinHandle<()>,
}

impl ProtocolTestClient {
    pub fn messages(&self) -> Vec<ServerMessage> {
        self.inner.messages.lock().expect("message mutex poisoned").clone()
    }

    pub fn message_count(&self) -> usize {
        self.inner.messages.lock().expect("message mutex poisoned").len()
    }

    pub fn closed(&self) -> bool {
        self.inner.closed.load(Ordering::SeqCst)
    }

    pub async fn hello(&self, version: u64) -> ClientResult<ServerMessage> {
        let response = self.next(|message| matches!(message, ServerMessage::Hello(_) | ServerMessage::HelloError(_)));
        self.send_message(&ClientMessage::Hello(pi_protocol::ClientHello::new(version)))
            .await?;
        response.await
    }

    pub async fn hello_default(&self) -> ClientResult<ServerMessage> {
        self.hello(PROTOCOL_VERSION).await
    }

    pub async fn request(&self, command: &Command, id: &str) -> ClientResult<ResponseEnvelope> {
        let response = self.next(|message| match message {
            ServerMessage::Response(response) => response_id(response) == id,
            _ => false,
        });
        let envelope = RequestEnvelope::new(id.to_string(), command.clone())
            .map_err(|error| Box::new(error) as Box<dyn std::error::Error + Send + Sync>)?;
        self.send_message(&ClientMessage::Request(envelope)).await?;
        match response.await? {
            ServerMessage::Response(response) => Ok(response),
            _ => unreachable!("request waiter only resolves responses"),
        }
    }

    pub async fn send_message(&self, message: &ClientMessage) -> ClientResult<()> {
        let frame = encode_client_message(message, None)
            .map_err(|error| Box::new(error) as Box<dyn std::error::Error + Send + Sync>)?;
        self.send_bytes(&frame).await
    }

    pub async fn send_bytes(&self, chunk: &[u8]) -> ClientResult<()> {
        let mut writer = self.writer.lock().await;
        writer.write_all(chunk).await?;
        writer.flush().await?;
        Ok(())
    }

    pub async fn send_fragmented_message(&self, message: &ClientMessage, split_at: usize) -> ClientResult<()> {
        let frame = encode_client_message(message, None)
            .map_err(|error| Box::new(error) as Box<dyn std::error::Error + Send + Sync>)?;
        let split_at = split_at.min(frame.len());
        self.send_bytes(&frame[..split_at]).await?;
        self.send_bytes(&frame[split_at..]).await
    }

    pub fn next<'a, F>(&'a self, predicate: F) -> BoxFuture<'a, ClientResult<ServerMessage>>
    where
        F: Fn(&ServerMessage) -> bool + Send + 'a,
    {
        self.next_from(0, predicate)
    }

    pub fn next_from<'a, F>(&'a self, index: usize, predicate: F) -> BoxFuture<'a, ClientResult<ServerMessage>>
    where
        F: Fn(&ServerMessage) -> bool + Send + 'a,
    {
        Box::pin(async move {
            let mut changed = self.inner.changed.subscribe();
            loop {
                if let Some(message) = self
                    .inner
                    .messages
                    .lock()
                    .expect("message mutex poisoned")
                    .iter()
                    .skip(index)
                    .find(|message| predicate(message))
                {
                    return Ok(message.clone());
                }
                if self.closed() {
                    return Err(Box::new(std::io::Error::other("Wire client is closed"))
                        as Box<dyn std::error::Error + Send + Sync>);
                }
                changed
                    .changed()
                    .await
                    .map_err(|error| Box::new(error) as Box<dyn std::error::Error + Send + Sync>)?;
            }
        })
    }

    pub async fn wait_for_close(&self) {
        let mut changed = self.inner.changed.subscribe();
        while !self.closed() {
            if changed.changed().await.is_err() {
                return;
            }
        }
    }

    pub async fn close(&self) {
        mark_closed(&self.inner);
        self.reader.abort();
        let mut writer = self.writer.lock().await;
        let _ = writer.shutdown().await;
    }
}

impl Drop for ProtocolTestClient {
    fn drop(&mut self) {
        mark_closed(&self.inner);
        self.reader.abort();
    }
}

fn response_id(response: &ResponseEnvelope) -> &str {
    match response {
        ResponseEnvelope::Success(response) => response.id(),
        ResponseEnvelope::Error(response) => response.id(),
    }
}

pub async fn connect_unix_test_client(path: &str) -> ClientResult<ProtocolTestClient> {
    let stream = UnixStream::connect(path).await?;
    let (mut reader, writer) = stream.into_split();
    let (changed, _) = watch::channel(0);
    let inner = Arc::new(ClientInner {
        messages: Mutex::new(Vec::new()),
        changed,
        closed: AtomicBool::new(false),
    });
    let reader_inner = inner.clone();
    let writer = Arc::new(tokio::sync::Mutex::new(writer));
    let reader_writer = writer.clone();
    let reader = tokio::spawn(async move {
        read_loop(reader_inner, &mut reader, reader_writer).await;
    });
    Ok(ProtocolTestClient { inner, writer, reader })
}

async fn read_loop(
    inner: Arc<ClientInner>,
    reader: &mut OwnedReadHalf,
    writer: Arc<tokio::sync::Mutex<OwnedWriteHalf>>,
) {
    let mut decoder = ServerMessageDecoder::new(None);
    let mut buffer = [0_u8; 8 * 1024];
    loop {
        match reader.read(&mut buffer).await {
            Ok(0) => {
                let mut writer = writer.lock().await;
                let _ = writer.shutdown().await;
                mark_closed(&inner);
                return;
            }
            Ok(bytes) => match decoder.push(&buffer[..bytes]) {
                Ok(messages) => store_messages(&inner, messages),
                Err(_) => {
                    mark_closed(&inner);
                    return;
                }
            },
            Err(_) => {
                mark_closed(&inner);
                return;
            }
        }
    }
}

fn store_messages(inner: &ClientInner, messages: Vec<ServerMessage>) {
    if messages.is_empty() {
        return;
    }
    inner.messages.lock().expect("message mutex poisoned").extend(messages);
    bump(inner);
}

fn mark_closed(inner: &ClientInner) {
    if inner.closed.swap(true, Ordering::SeqCst) {
        return;
    }
    bump(inner);
}

fn bump(inner: &ClientInner) {
    let next = *inner.changed.borrow() + 1;
    let _ = inner.changed.send(next);
}
