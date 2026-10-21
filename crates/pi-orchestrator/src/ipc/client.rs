//! One request per Unix socket, returning the first newline-delimited response.
use super::protocol::{encode_message, parse_response_line};
use serde_json::Value;
use std::path::Path;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
};

pub async fn send_ipc_request(path: &Path, request: &Value) -> Result<Value, String> {
    let mut socket = UnixStream::connect(path).await.map_err(|e| e.to_string())?;
    socket
        .write_all(encode_message(request).as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    let mut buffer = String::new();
    let mut chunk = [0; 8192];
    loop {
        let count = socket.read(&mut chunk).await.map_err(|e| e.to_string())?;
        if count == 0 {
            return Err(format!(
                "Orchestrator socket closed before a response was received: {}",
                path.display()
            ));
        }
        buffer.push_str(&String::from_utf8_lossy(&chunk[..count]));
        if let Some(end) = buffer.find('\n') {
            let line = buffer[..end].trim();
            if !line.is_empty() {
                return parse_response_line(line);
            }
        }
    }
}
