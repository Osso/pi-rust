//! Unix JSONL server, including the frozen initial-data-event buffering boundary.
use super::protocol::{encode_message, error_response, parse_request_line};
use pi_ai::request::BoxFuture;
use serde_json::Value;
use std::{path::Path, sync::Arc};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{UnixListener, UnixStream},
    sync::{mpsc, oneshot},
    task::{JoinHandle, JoinSet},
};

pub type OnMessage = Arc<dyn Fn(Value) + Send + Sync>;
pub trait RpcStream: Send + Sync {
    fn handle_request(&self, request: Value) -> BoxFuture<'_, Result<(), String>>;
    fn close(&self);
}
pub trait IpcRequestHandler: Send + Sync {
    fn handle(&self, request: Value) -> BoxFuture<'_, Result<Value, String>>;
    fn open_rpc_stream(&self, instance_id: &str, on_message: OnMessage) -> Option<Arc<dyn RpcStream>>;
}

pub struct IpcServer {
    task: JoinHandle<()>,
    stop: Option<oneshot::Sender<()>>,
}
impl IpcServer {
    pub async fn close(mut self) {
        self.stop_accepting();
        if let Err(error) = (&mut self.task).await {
            eprintln!("IPC server close: {error}");
        }
    }
    fn stop_accepting(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
    }
}
impl Drop for IpcServer {
    fn drop(&mut self) {
        self.stop_accepting();
    }
}

pub async fn start_ipc_server(path: &Path, handler: Arc<dyn IpcRequestHandler>) -> Result<IpcServer, String> {
    remove_stale_socket(path).await?;
    let listener = UnixListener::bind(path)
        .map_err(|error| pi_js_compat::node_fs::node_unix_listen_error(&error, &path.to_string_lossy()))?;
    let path = path.to_owned();
    let (stop, mut stopped) = oneshot::channel();
    let task = tokio::spawn(async move {
        let mut clients = JoinSet::new();
        loop {
            tokio::select! {
                biased;
                _ = &mut stopped => break,
                result = listener.accept() => match result {
                    Ok((socket, _)) => {
                        let handler = handler.clone();
                        clients.spawn(async move {
                            if let Err(error) = serve_connection(socket, handler).await { eprintln!("IPC connection: {error}"); }
                        });
                    },
                    Err(error) => { eprintln!("IPC accept: {error}"); break; }
                },
                _ = clients.join_next(), if !clients.is_empty() => {}
            }
        }
        drop(listener);
        if let Err(error) = crate::storage::remove_if_exists(&path) {
            eprintln!("IPC socket close: {error}");
        }
        // Node Server.close stops accepts but leaves established streams alive.
        clients.detach_all();
    });
    Ok(IpcServer { task, stop: Some(stop) })
}

async fn remove_stale_socket(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    match UnixStream::connect(path).await {
        Ok(_) => Err(format!("orchestrator is already running: {}", path.display())),
        Err(e)
            if matches!(
                e.kind(),
                std::io::ErrorKind::ConnectionRefused
                    | std::io::ErrorKind::NotFound
                    | std::io::ErrorKind::BrokenPipe
                    | std::io::ErrorKind::ConnectionReset
            ) =>
        {
            std::fs::remove_file(path)
                .map_err(|error| pi_js_compat::node_fs::node_fs_error(&error, "unlink", &path.to_string_lossy()))
        }
        Err(error) => Err(pi_js_compat::node_fs::node_unix_connect_error(
            &error,
            &path.to_string_lossy(),
        )),
    }
}

fn take_line(buffer: &mut String) -> Option<String> {
    let end = buffer.find('\n')?;
    let line = buffer[..end].trim().to_owned();
    buffer.drain(..=end);
    Some(line)
}

async fn serve_connection(mut socket: UnixStream, handler: Arc<dyn IpcRequestHandler>) -> Result<(), String> {
    let mut buffer = String::new();
    let mut chunk = [0; 8192];
    loop {
        let count = socket.read(&mut chunk).await.map_err(|e| e.to_string())?;
        if count == 0 {
            return Ok(());
        }
        buffer.push_str(&String::from_utf8_lossy(&chunk[..count]));
        let Some(line) = take_line(&mut buffer) else {
            continue;
        };
        if line.is_empty() {
            continue;
        }
        let request = match parse_request_line(&line) {
            Ok(request) => request,
            Err(error) => return send_and_end(socket, error_response(error)).await,
        };
        let response = handler.handle(request.clone()).await.unwrap_or_else(error_response);
        if request["type"] != "rpc_stream"
            || response["ok"] != true
            || response["type"] != "rpc_ready"
            || !response["instance"].is_object()
        {
            return send_and_end(socket, response).await;
        }
        return serve_rpc_stream(socket, handler, request, response, buffer).await;
    }
}

async fn send_and_end(mut socket: UnixStream, response: Value) -> Result<(), String> {
    socket
        .write_all(encode_message(&response).as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    socket.shutdown().await.map_err(|e| e.to_string())
}

struct CloseStream(Arc<dyn RpcStream>);
impl Drop for CloseStream {
    fn drop(&mut self) {
        self.0.close();
    }
}

async fn serve_rpc_stream(
    mut socket: UnixStream,
    handler: Arc<dyn IpcRequestHandler>,
    request: Value,
    response: Value,
    mut buffer: String,
) -> Result<(), String> {
    let (sender, mut receiver) = mpsc::unbounded_channel();
    let sender_for_errors = sender.clone();
    let on_message = Arc::new(move |message| {
        let _ = sender.send(message);
    });
    let id = request["instanceId"].as_str().unwrap_or_default();
    let Some(stream) = handler.open_rpc_stream(id, on_message) else {
        return send_and_end(socket, error_response(format!("Unknown instance: {id}"))).await;
    };
    let _close = CloseStream(stream.clone());
    let (requests, queued) = mpsc::unbounded_channel();
    socket
        .write_all(encode_message(&response).as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    let _worker = RequestWorker(queue_stream_requests(stream, queued, sender_for_errors));
    pump_stream_socket(&mut socket, &mut buffer, &requests, &mut receiver).await
}

struct RequestWorker(JoinHandle<()>);
impl Drop for RequestWorker {
    fn drop(&mut self) {
        self.0.abort();
    }
}

fn queue_stream_requests(
    stream: Arc<dyn RpcStream>,
    mut queued: mpsc::UnboundedReceiver<String>,
    responses: mpsc::UnboundedSender<Value>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(line) = queued.recv().await {
            let result = match parse_request_line(&line) {
                Ok(request) => stream.handle_request(request).await,
                Err(error) => Err(error),
            };
            if let Err(error) = result {
                let _ = responses.send(error_response(error));
            }
        }
    })
}

async fn pump_stream_socket(
    socket: &mut UnixStream,
    buffer: &mut String,
    requests: &mpsc::UnboundedSender<String>,
    receiver: &mut mpsc::UnboundedReceiver<Value>,
) -> Result<(), String> {
    let mut chunk = [0; 8192];
    loop {
        tokio::select! {
            Some(message) = receiver.recv() => {
                socket.write_all(encode_message(&message).as_bytes()).await.map_err(|e| e.to_string())?;
            },
            result = socket.read(&mut chunk) => {
                let count = result.map_err(|e| e.to_string())?;
                if count == 0 { return Ok(()); }
                buffer.push_str(&String::from_utf8_lossy(&chunk[..count]));
                // Frozen boundary: buffered stream lines are drained only on a new data event.
                while let Some(line) = take_line(buffer) {
                    if !line.is_empty() { requests.send(line).map_err(|e| e.to_string())?; }
                }
            }
        }
    }
}
