use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use pi_protocol::DEFAULT_MAX_FRAME_LENGTH;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use crate::errors::PiError;
use crate::transport::{
    ByteTransport, ByteTransportFactory, ByteTransportHandlers, TransportFuture, TransportSendFuture,
};

const MAX_UNIX_SOCKET_PATH_BYTES: usize = 107;
const READ_BUFFER_BYTES: usize = 64 * 1024;

/// Options used to create Unix-domain socket transports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnixTransportOptions {
    pub path: String,
    pub max_pending_bytes: Option<usize>,
}

/// Creates fresh Unix-domain socket transports for client connection attempts.
pub fn create_unix_transport_factory(options: UnixTransportOptions) -> Result<Arc<dyn ByteTransportFactory>, PiError> {
    if options.path.is_empty() {
        return Err(PiError::InvalidArgument("Unix transport path must not be empty".into()));
    }
    if options.path.len() > MAX_UNIX_SOCKET_PATH_BYTES {
        return Err(PiError::InvalidArgument(format!(
            "Unix transport path is too long; maximum is {MAX_UNIX_SOCKET_PATH_BYTES} UTF-8 bytes"
        )));
    }
    let max_pending_bytes = options
        .max_pending_bytes
        .unwrap_or(DEFAULT_MAX_FRAME_LENGTH as usize * 4);
    if max_pending_bytes == 0 {
        return Err(PiError::InvalidArgument(
            "Unix transport maxPendingBytes must be a positive safe integer".into(),
        ));
    }
    Ok(Arc::new(UnixTransportFactory {
        path: options.path,
        max_pending_bytes,
    }))
}

struct UnixTransportFactory {
    path: String,
    max_pending_bytes: usize,
}

impl ByteTransportFactory for UnixTransportFactory {
    fn create(&self, handlers: ByteTransportHandlers) -> TransportFuture {
        let path = self.path.clone();
        let max_pending_bytes = self.max_pending_bytes;
        Box::pin(async move { connect_unix_socket(&path, max_pending_bytes, handlers).await })
    }
}

async fn connect_unix_socket(
    path: &str,
    max_pending_bytes: usize,
    handlers: ByteTransportHandlers,
) -> Result<Arc<dyn ByteTransport>, PiError> {
    let stream = UnixStream::connect(path).await?;
    let (mut read_half, write_half) = stream.into_split();
    let (jobs, job_receiver) = mpsc::unbounded_channel();
    let inner = Arc::new(UnixInner {
        jobs,
        closed: AtomicBool::new(false),
        pending_bytes: Mutex::new(0),
        max_pending_bytes,
        write_half: tokio::sync::Mutex::new(Some(write_half)),
        cancellation: CancellationToken::new(),
    });
    let writer_inner = inner.clone();
    tokio::spawn(async move { run_writer(writer_inner, job_receiver).await });
    let reader_inner = inner.clone();
    tokio::spawn(async move {
        let mut buffer = vec![0u8; READ_BUFFER_BYTES];
        loop {
            let read = tokio::select! {
                _ = reader_inner.cancellation.cancelled() => return,
                read = read_half.read(&mut buffer) => read,
            };
            match read {
                Ok(0) => {
                    if !reader_inner.closed.load(Ordering::SeqCst) {
                        (handlers.on_close)();
                    }
                    return;
                }
                Ok(count) => (handlers.on_data)(buffer[..count].to_vec()),
                Err(error) => {
                    if !reader_inner.closed.load(Ordering::SeqCst) {
                        (handlers.on_error)(PiError::from(error));
                    }
                    return;
                }
            }
        }
    });
    Ok(Arc::new(UnixByteTransport { inner }))
}

enum WriteJob {
    Chunk {
        bytes: Vec<u8>,
        done: oneshot::Sender<Result<(), PiError>>,
    },
    Close,
}

struct UnixInner {
    jobs: mpsc::UnboundedSender<WriteJob>,
    closed: AtomicBool,
    pending_bytes: Mutex<usize>,
    max_pending_bytes: usize,
    write_half: tokio::sync::Mutex<Option<tokio::net::unix::OwnedWriteHalf>>,
    cancellation: CancellationToken,
}

struct UnixByteTransport {
    inner: Arc<UnixInner>,
}

impl ByteTransport for UnixByteTransport {
    fn send(&self, chunk: Vec<u8>) -> TransportSendFuture {
        let result = self.enqueue(chunk);
        Box::pin(async move {
            match result {
                Ok(receiver) => receiver
                    .await
                    .unwrap_or_else(|_| Err(PiError::Transport("Unix transport is closed".into()))),
                Err(error) => Err(error),
            }
        })
    }

    fn close(&self) {
        if self.inner.closed.swap(true, Ordering::SeqCst) {
            return;
        }
        self.inner.cancellation.cancel();
        let _ = self.inner.jobs.send(WriteJob::Close);
    }
}

impl UnixByteTransport {
    fn enqueue(&self, chunk: Vec<u8>) -> Result<oneshot::Receiver<Result<(), PiError>>, PiError> {
        let length = chunk.len();
        {
            let mut pending = self.inner.pending_bytes.lock().unwrap();
            if self.inner.closed.load(Ordering::SeqCst) {
                return Err(PiError::Transport("Unix transport is closed".into()));
            }
            if *pending + length > self.inner.max_pending_bytes {
                return Err(PiError::Transport(
                    "Unix transport exceeded its pending byte limit".into(),
                ));
            }
            *pending += length;
        }
        let (done, receiver) = oneshot::channel();
        if self.inner.jobs.send(WriteJob::Chunk { bytes: chunk, done }).is_err() {
            *self.inner.pending_bytes.lock().unwrap() -= length;
            return Err(PiError::Transport("Unix transport is closed".into()));
        }
        Ok(receiver)
    }
}

async fn run_writer(inner: Arc<UnixInner>, mut jobs: mpsc::UnboundedReceiver<WriteJob>) {
    while let Some(job) = jobs.recv().await {
        match job {
            WriteJob::Chunk { bytes, done } => {
                let length = bytes.len();
                let result = {
                    let mut write_half = inner.write_half.lock().await;
                    match write_half.as_mut() {
                        Some(write_half) => write_half.write_all(&bytes).await.map_err(PiError::from),
                        None => Err(PiError::Transport("Unix transport is closed".into())),
                    }
                };
                *inner.pending_bytes.lock().unwrap() -= length;
                let _ = done.send(result);
            }
            WriteJob::Close => {
                if let Some(mut write_half) = inner.write_half.lock().await.take() {
                    let _ = write_half.shutdown().await;
                }
                return;
            }
        }
    }
}
