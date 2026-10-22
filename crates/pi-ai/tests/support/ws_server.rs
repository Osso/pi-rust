//! Scripted local WebSocket server shared by Codex transport tests.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};

#[derive(Default)]
pub struct ServerRecords {
    pub connections: Mutex<Vec<ServerConnection>>,
    pub frame_notify: tokio::sync::Notify,
}

#[derive(Default)]
pub struct ServerConnection {
    pub headers: BTreeMap<String, String>,
    pub frames: Vec<Value>,
    pub pings: usize,
    pub close: Option<(u16, String)>,
}

pub type ServerScript = Arc<dyn Fn(usize, usize) -> Vec<Value> + Send + Sync>;

pub struct WsServer {
    pub url: String,
    pub records: Arc<ServerRecords>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for WsServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

pub async fn start_ws_server(script: ServerScript) -> WsServer {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ws server");
    let address = listener.local_addr().expect("server address");
    let records = Arc::new(ServerRecords::default());
    let captured = records.clone();
    let task = tokio::spawn(async move {
        let mut connection_index = 0usize;
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            let captured = captured.clone();
            let script = script.clone();
            let index = connection_index;
            connection_index += 1;
            tokio::spawn(async move {
                let headers = peek_http_headers(&stream).await;
                let Ok(mut socket) = tokio_tungstenite::accept_async(stream).await else {
                    return;
                };
                {
                    let mut connections = captured.connections.lock().unwrap();
                    while connections.len() <= index {
                        connections.push(ServerConnection::default());
                    }
                    connections[index].headers = headers;
                }
                let mut frame_index = 0usize;
                while let Some(message) = socket.next().await {
                    let Ok(message) = message else {
                        return;
                    };
                    match message {
                        tokio_tungstenite::tungstenite::Message::Text(text) => {
                            let frame: Value = serde_json::from_str(text.as_str()).expect("client frame");
                            {
                                let mut connections = captured.connections.lock().unwrap();
                                connections[index].frames.push(frame);
                            }
                            captured.frame_notify.notify_waiters();
                            let events = script(index, frame_index);
                            frame_index += 1;
                            for event in events {
                                if socket
                                    .send(tokio_tungstenite::tungstenite::Message::text(event.to_string()))
                                    .await
                                    .is_err()
                                {
                                    return;
                                }
                            }
                        }
                        tokio_tungstenite::tungstenite::Message::Ping(_) => {
                            let mut connections = captured.connections.lock().unwrap();
                            connections[index].pings += 1;
                        }
                        tokio_tungstenite::tungstenite::Message::Close(frame) => {
                            let mut connections = captured.connections.lock().unwrap();
                            connections[index].close =
                                frame.map(|frame| (u16::from(frame.code), frame.reason.as_str().to_owned()));
                            return;
                        }
                        _ => {}
                    }
                }
            });
        }
    });
    WsServer {
        url: format!("ws://{address}"),
        records,
        task,
    }
}

pub fn completed_event(response_id: &str, text: Option<&str>, end_turn: Option<bool>) -> Vec<Value> {
    let mut events = vec![json!({"type": "response.created", "response": {"id": response_id}})];
    if let Some(text) = text {
        events.push(json!({"type": "response.output_item.added", "output_index": 0, "item": {
            "type": "message", "id": format!("msg_{response_id}"), "role": "assistant", "status": "in_progress", "content": []
        }}));
        events.push(json!({"type": "response.output_text.delta", "output_index": 0, "delta": text}));
        events.push(json!({"type": "response.output_item.done", "output_index": 0, "item": {
            "type": "message", "id": format!("msg_{response_id}"), "role": "assistant", "status": "completed",
            "content": [{"type": "output_text", "text": text}]
        }}));
    }
    events.push(json!({"type": "response.completed", "response": {
        "id": response_id,
        "status": "completed",
        "end_turn": end_turn,
        "usage": {"input_tokens": 5, "output_tokens": 3, "total_tokens": 8,
            "input_tokens_details": {"cached_tokens": 0}}
    }}));
    events
}

async fn peek_http_headers(stream: &tokio::net::TcpStream) -> BTreeMap<String, String> {
    let mut buffer = vec![0_u8; 16 * 1024];
    loop {
        let read = stream.peek(&mut buffer).await.expect("peek handshake");
        // Handshake headers are ByteStrings: decode each wire byte as one Latin-1 unit.
        let text: String = buffer[..read].iter().copied().map(char::from).collect();
        if let Some(end) = text.find("\r\n\r\n") {
            return parse_http_headers(&text[..end]);
        }
        tokio::task::yield_now().await;
    }
}

fn parse_http_headers(request: &str) -> BTreeMap<String, String> {
    request
        .lines()
        .skip(1)
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_owned()))
        .collect()
}
