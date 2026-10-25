//! Frozen `host-relay.ts`: newline protocol over caller-owned streams.
use super::host_relay_store::*;
use super::session_control_db::ControlDbError;
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_json::{self, JsString, JsValue};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub struct HostRelayOptions<'a> {
    pub control_db_path: &'a JsString,
    pub host: Option<&'a JsString>,
    pub poll_ms: Option<u64>,
    pub signal: AbortSignal,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RelayFrame {
    Hello { host: JsString },
    Ack { kind: String, id: i64 },
    Mailbox(RelayMailbox),
    Channel(RelayChannel),
}
impl RelayFrame {
    pub fn to_wire(&self) -> String {
        let value = match self {
            Self::Hello { host } => {
                let mut object = JsValue::from(serde_json::json!({"type":"hello"}))
                    .as_object()
                    .unwrap()
                    .clone();
                object.insert("host".into(), JsValue::String(host.clone()));
                JsValue::Object(object)
            }
            Self::Ack { kind, id } => JsValue::from(serde_json::json!({"type":"ack","kind":kind,"id":id})),
            Self::Mailbox(message) => {
                let mut frame = JsValue::from(serde_json::json!({"type":"mailbox","seq":message.seq}))
                    .as_object()
                    .unwrap()
                    .clone();
                frame.insert("sessionPath".into(), JsValue::String(message.session_path.clone()));
                frame.insert("messageId".into(), JsValue::String(message.message_id.clone()));
                frame.insert("data".into(), message.data.clone());
                JsValue::Object(frame)
            }
            Self::Channel(message) => message.to_value(),
        };
        js_json::stringify_lossless(&value)
    }
}
pub fn parse_frame(line: &str) -> Result<RelayFrame, ControlDbError> {
    let frame = js_json::parse_lossless(line).map_err(|error| ControlDbError::new(error.to_string()))?;
    if frame.as_object().is_none() {
        return Err(ControlDbError::new("Invalid relay frame"));
    }
    let kind = frame
        .get("type")
        .and_then(JsValue::as_string)
        .map(|s| s.to_utf8_lossy());
    match kind.as_deref() {
        Some("hello") => Ok(RelayFrame::Hello {
            host: require_text(frame.get("host"), "host")?,
        }),
        Some("ack") => parse_ack(&frame),
        Some("mailbox") => parse_mailbox(&frame),
        Some("channel") => Ok(RelayFrame::Channel(RelayChannel {
            origin_host: require_text(frame.get("originHost"), "originHost")?,
            origin_id: positive_integer(frame.get("originId"))?,
            sender_session_id: require_text(frame.get("senderSessionId"), "senderSessionId")?,
            sender_agent_id: nullable_text(frame.get("senderAgentId"), "senderAgentId")?,
            body: require_text(frame.get("body"), "body")?,
            created_at: require_text(frame.get("createdAt"), "createdAt")?,
        })),
        _ => Err(ControlDbError::new("Unknown relay frame type")),
    }
}

fn parse_ack(frame: &JsValue) -> Result<RelayFrame, ControlDbError> {
    let kind = frame
        .get("kind")
        .and_then(JsValue::as_string)
        .map(JsString::to_utf8_lossy);
    let Some(kind) = kind.filter(|kind| kind == "mailbox" || kind == "channel") else {
        return Err(ControlDbError::new("Invalid relay ack kind"));
    };
    Ok(RelayFrame::Ack {
        kind,
        id: positive_integer(frame.get("id"))?,
    })
}
fn parse_mailbox(frame: &JsValue) -> Result<RelayFrame, ControlDbError> {
    let Some(data) = frame.get("data").filter(|data| data.as_object().is_some()) else {
        return Err(ControlDbError::new("Invalid relay mailbox data"));
    };
    Ok(RelayFrame::Mailbox(RelayMailbox {
        seq: positive_integer(frame.get("seq"))?,
        session_path: require_text(frame.get("sessionPath"), "sessionPath")?,
        message_id: require_text(frame.get("messageId"), "messageId")?,
        data: data.clone(),
    }))
}

struct RelayState<'a> {
    path: &'a JsString,
    host: &'a JsString,
    peer: Option<JsString>,
    mailbox: Option<RelayMailbox>,
    channel: Option<RelayChannel>,
}
impl RelayState<'_> {
    fn scan(&mut self) -> Result<Vec<RelayFrame>, ControlDbError> {
        let Some(peer) = &self.peer else {
            return Ok(Vec::new());
        };
        self.mailbox = next_relay_mailbox(self.path, peer)?;
        self.channel = next_relay_channel(self.path, peer, self.host)?;
        Ok(self
            .mailbox
            .clone()
            .map(RelayFrame::Mailbox)
            .into_iter()
            .chain(self.channel.clone().map(RelayFrame::Channel))
            .collect())
    }
    fn receive(&mut self, frame: RelayFrame, can_scan: bool) -> Result<Vec<RelayFrame>, ControlDbError> {
        if let RelayFrame::Hello { host } = frame {
            if self.peer.is_some() || host == *self.host {
                return Err(ControlDbError::new("Invalid relay peer hello"));
            }
            self.peer = Some(host);
            return if can_scan { self.scan() } else { Ok(Vec::new()) };
        }
        let Some(peer) = &self.peer else {
            return Err(ControlDbError::new("Relay hello required before messages"));
        };
        match frame {
            RelayFrame::Mailbox(message) => {
                if message.data.get("recipientHost") != Some(&JsValue::String(self.host.clone()))
                    || message.data.get("relaySeq").and_then(JsValue::as_f64) != Some(message.seq as f64)
                {
                    return Err(ControlDbError::new("Relay mailbox route/sequence mismatch"));
                }
                import_relay_mailbox(self.path, peer, &message)?;
                return Ok(vec![RelayFrame::Ack {
                    kind: "mailbox".into(),
                    id: message.seq,
                }]);
            }
            RelayFrame::Channel(message) => {
                if message.origin_host != *peer {
                    return Err(ControlDbError::new("Relay channel origin mismatch"));
                }
                import_relay_channel(self.path, &message)?;
                return Ok(vec![RelayFrame::Ack {
                    kind: "channel".into(),
                    id: message.origin_id,
                }]);
            }
            RelayFrame::Ack { kind, id }
                if kind == "mailbox" && self.mailbox.as_ref().is_some_and(|message| message.seq == id) =>
            {
                ack_relay_mailbox(self.path, peer, self.mailbox.as_ref().unwrap())?;
                self.mailbox = None;
            }
            RelayFrame::Ack { kind, id }
                if kind == "channel" && self.channel.as_ref().is_some_and(|message| message.origin_id == id) =>
            {
                ack_relay_channel(self.path, peer, id)?;
                self.channel = None;
            }
            _ => {}
        }
        Ok(Vec::new())
    }
}
struct RelayOutput {
    sender: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    need_drain: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
impl RelayOutput {
    fn send(&self, frames: Vec<RelayFrame>) -> Result<(), String> {
        for frame in frames {
            self.sender
                .send(format!("{}\n", frame.to_wire()).into_bytes())
                .map_err(|_| "Relay output closed".to_owned())?;
        }
        Ok(())
    }
}
fn start_output_writer(
    output: impl AsyncWrite + Unpin + Send + 'static,
) -> (RelayOutput, tokio::sync::oneshot::Receiver<Result<(), String>>) {
    let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
    let (finished, result) = tokio::sync::oneshot::channel();
    let need_drain = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let drain = std::sync::Arc::clone(&need_drain);
    tokio::spawn(async move {
        let outcome = drain_output_queue(output, receiver, drain).await;
        if let Err(Err(error)) = finished.send(outcome) {
            eprintln!("Relay output error: {error}");
        }
    });
    (RelayOutput { sender, need_drain }, result)
}
async fn drain_output_queue(
    mut output: impl AsyncWrite + Unpin,
    mut receiver: tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>,
    need_drain: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> Result<(), String> {
    use std::sync::atomic::Ordering;
    while let Some(bytes) = receiver.recv().await {
        let mut remaining = bytes.as_slice();
        while !remaining.is_empty() {
            let count = std::future::poll_fn(|cx| {
                let result = std::pin::Pin::new(&mut output).poll_write(cx, remaining);
                if result.is_pending() {
                    need_drain.store(true, Ordering::SeqCst);
                }
                result
            })
            .await
            .map_err(|error| error.to_string())?;
            if count == 0 {
                return Err("Relay output write returned zero bytes".into());
            }
            remaining = &remaining[count..];
        }
        output.flush().await.map_err(|error| error.to_string())?;
        if receiver.is_empty() {
            need_drain.store(false, Ordering::SeqCst);
        }
    }
    Ok(())
}

struct RelayLines<R> {
    input: R,
    bytes: Vec<u8>,
    skip_lf: bool,
    eof: bool,
}
impl<R: AsyncRead + Unpin> RelayLines<R> {
    fn new(input: R) -> Self {
        Self {
            input,
            bytes: Vec::new(),
            skip_lf: false,
            eof: false,
        }
    }
    async fn next_line(&mut self) -> std::io::Result<Option<String>> {
        loop {
            if self.skip_lf && !self.bytes.is_empty() {
                if self.bytes[0] == b'\n' {
                    self.bytes.remove(0);
                }
                self.skip_lf = false;
            }
            if let Some(end) = self.bytes.iter().position(|byte| *byte == b'\n' || *byte == b'\r') {
                let line = String::from_utf8_lossy(&self.bytes[..end]).into_owned();
                self.skip_lf = self.bytes[end] == b'\r';
                self.bytes.drain(..=end);
                return Ok(Some(line));
            }
            if self.eof {
                if self.bytes.is_empty() {
                    return Ok(None);
                }
                return Ok(Some(
                    String::from_utf8_lossy(&std::mem::take(&mut self.bytes)).into_owned(),
                ));
            }
            self.eof = self.input.read_buf(&mut self.bytes).await? == 0;
        }
    }
}

/// One mailbox and channel in flight; cancellation leaves authoritative queues intact.
pub async fn run_host_relay(
    input: impl AsyncRead + Unpin,
    output: impl AsyncWrite + Unpin + Send + 'static,
    options: HostRelayOptions<'_>,
) -> Result<(), String> {
    if options.signal.aborted() {
        return Ok(());
    }
    let (output, mut finished) = start_output_writer(output);
    let result = tokio::select! {
        biased;
        _ = options.signal.wait() => Ok(()),
        result = &mut finished => return result.map_err(|error| error.to_string())?,
        result = relay_stream(input, &output, &options) => result,
    };
    drop(output);
    tokio::task::yield_now().await;
    // Frozen close resolves without waiting for Writable drain; queued writes retain their task.
    result
}
async fn relay_stream(
    input: impl AsyncRead + Unpin,
    output: &RelayOutput,
    options: &HostRelayOptions<'_>,
) -> Result<(), String> {
    let host = options
        .host
        .cloned()
        .unwrap_or_else(|| pi_js_compat::node_fs::decode_node_path(gethostname::gethostname().as_os_str()));
    let mut state = RelayState {
        path: options.control_db_path,
        host: &host,
        peer: None,
        mailbox: None,
        channel: None,
    };
    output.send(vec![RelayFrame::Hello { host: host.clone() }])?;
    // Frozen stream listeners observe input only after the initial synchronous hello write.
    tokio::task::yield_now().await;
    let period = Duration::from_millis(options.poll_ms.unwrap_or(2000).max(1));
    let mut timer = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut lines = RelayLines::new(input);
    loop {
        let frames = tokio::select! {
            line = lines.next_line() => {
                let Some(line) = line.map_err(|e| e.to_string())? else { return Ok(()); };
                let can_scan = !output.need_drain.load(std::sync::atomic::Ordering::SeqCst);
                state.receive(parse_frame(&line).map_err(|e| e.to_string())?, can_scan).map_err(|e| e.to_string())?
            }
            _ = timer.tick() => {
                if output.need_drain.load(std::sync::atomic::Ordering::SeqCst) { Vec::new() } else { state.scan().map_err(|e| e.to_string())? }
            },
        };
        output.send(frames)?;
    }
}
pub async fn run_relay_serve() -> Result<(), pi_ai::request::CallbackError> {
    let path = super::session_control_db::get_control_db_path(&crate::config::get_user_state_root()?);
    run_host_relay(
        tokio::io::stdin(),
        tokio::io::stdout(),
        HostRelayOptions {
            control_db_path: &path,
            host: None,
            poll_ms: None,
            signal: AbortSignal::default(),
        },
    )
    .await
    .map_err(Into::into)
}
