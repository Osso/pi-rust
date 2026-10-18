//! Shared runtime for the OAuth flows: injected `Fetch`, clock, randomness,
//! HTTP helpers, and the loopback callback server.

use std::sync::Arc;

use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::http::default_fetch;
use crate::request::{BoxFuture, CallbackError, Fetch, HttpBody, HttpRequest};

/// `Date.now()` with injectable time.
pub type Clock = Arc<dyn Fn() -> f64 + Send + Sync>;

/// Random source for PKCE verifiers, hex state, and UUIDs.
pub trait RandomSource: Send + Sync {
    fn fill(&self, bytes: &mut [u8]);

    fn uuid(&self) -> String;
}

pub struct OsRandom;

impl RandomSource for OsRandom {
    fn fill(&self, bytes: &mut [u8]) {
        getrandom::fill(bytes).expect("OS random source must be available for OAuth");
    }

    fn uuid(&self) -> String {
        uuid_v4(self)
    }
}

pub fn system_clock() -> Clock {
    Arc::new(|| crate::request::now_millis() as f64)
}

pub fn os_random() -> Arc<dyn RandomSource> {
    Arc::new(OsRandom)
}

/// Injected dependencies shared by every OAuth flow.
#[derive(Clone)]
pub struct OAuthRuntime {
    pub fetch: Arc<dyn Fetch>,
    pub clock: Clock,
    pub random: Arc<dyn RandomSource>,
}

impl OAuthRuntime {
    pub fn system() -> Self {
        Self {
            fetch: default_fetch(),
            clock: system_clock(),
            random: os_random(),
        }
    }

    pub fn new(fetch: Arc<dyn Fetch>, clock: Clock, random: Arc<dyn RandomSource>) -> Self {
        Self { fetch, clock, random }
    }

    pub fn now(&self) -> f64 {
        (self.clock)()
    }
}

impl Default for OAuthRuntime {
    fn default() -> Self {
        Self::system()
    }
}

pub fn uuid_v4(random: &dyn RandomSource) -> String {
    let mut bytes = [0_u8; 16];
    random.fill(&mut bytes);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format_uuid(&bytes)
}

fn format_uuid(bytes: &[u8; 16]) -> String {
    let mut text = String::with_capacity(36);
    for (index, byte) in bytes.iter().enumerate() {
        if matches!(index, 4 | 6 | 8 | 10) {
            text.push('-');
        }
        text.push_str(&format!("{byte:02x}"));
    }
    text
}

/// `randomBytes(length).toString("hex")`.
pub fn random_hex(random: &dyn RandomSource, length: usize) -> String {
    let mut bytes = vec![0_u8; length];
    random.fill(&mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// `URLSearchParams` / `application/x-www-form-urlencoded` encoding in pair order.
pub fn form_encode(pairs: &[(&str, &str)]) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    serializer.extend_pairs(pairs.iter().copied());
    serializer.finish()
}

/// Replaces the query string with `pairs`, preserving insertion order.
pub fn url_with_query(base: &str, pairs: &[(&str, &str)]) -> String {
    let mut url = Url::parse(base).expect("OAuth URL constants must be absolute");
    url.set_query(None);
    {
        let mut query = url.query_pairs_mut();
        for (name, value) in pairs {
            query.append_pair(name, value);
        }
    }
    url.to_string()
}

/// Appends query parameters to an existing URL.
pub fn url_append_query(base: &str, pairs: &[(&str, &str)]) -> String {
    let mut url = Url::parse(base).expect("OAuth URL constants must be absolute");
    {
        let mut query = url.query_pairs_mut();
        for (name, value) in pairs {
            query.append_pair(name, value);
        }
    }
    url.to_string()
}

pub fn status_text(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        204 => "No Content",
        301 => "Moved Permanently",
        302 => "Found",
        304 => "Not Modified",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        410 => "Gone",
        418 => "I'm a Teapot",
        422 => "Unprocessable Entity",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "",
    }
}

pub struct OAuthHttpResponse {
    pub status: u16,
    pub body: String,
    pub json: Option<Value>,
}

impl OAuthHttpResponse {
    pub fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// Sends one request and reads the full body.
pub async fn send(
    fetch: &Arc<dyn Fetch>,
    request: HttpRequest,
    signal: Option<CancellationToken>,
) -> Result<OAuthHttpResponse, CallbackError> {
    let response = fetch.fetch(request, signal).await?;
    let status = response.status;
    let body = collect_body(response.body).await?;
    let body = String::from_utf8_lossy(&body).into_owned();
    let json = serde_json::from_str(&body).ok();
    Ok(OAuthHttpResponse { status, body, json })
}

async fn collect_body(mut body: Box<dyn HttpBody>) -> Result<Vec<u8>, CallbackError> {
    let mut bytes = Vec::new();
    while let Some(chunk) = body.next_chunk().await {
        bytes.extend_from_slice(&chunk?);
    }
    Ok(bytes)
}

pub fn json_request(url: &str, body: &Value, accept: bool) -> HttpRequest {
    let mut headers = vec![("Content-Type".to_owned(), "application/json".to_owned())];
    if accept {
        headers.push(("Accept".to_owned(), "application/json".to_owned()));
    }
    HttpRequest {
        method: "POST".to_owned(),
        url: url.to_owned(),
        headers,
        body: Some(body.to_string().into_bytes()),
    }
}

pub fn form_request(url: &str, body: String, accept: bool) -> HttpRequest {
    let mut headers = vec![(
        "Content-Type".to_owned(),
        "application/x-www-form-urlencoded".to_owned(),
    )];
    if accept {
        headers.push(("Accept".to_owned(), "application/json".to_owned()));
    }
    HttpRequest {
        method: "POST".to_owned(),
        url: url.to_owned(),
        headers,
        body: Some(body.into_bytes()),
    }
}

/// Shared `parseAuthorizationInput` used by Anthropic and OpenAI Codex.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AuthorizationInput {
    pub code: Option<String>,
    pub state: Option<String>,
}

pub fn parse_authorization_input(input: &str) -> AuthorizationInput {
    let value = input.trim();
    if value.is_empty() {
        return AuthorizationInput::default();
    }

    if let Ok(url) = Url::parse(value) {
        let mut code = None;
        let mut state = None;
        for (name, value) in url.query_pairs() {
            match name.as_ref() {
                "code" => code = Some(value.into_owned()),
                "state" => state = Some(value.into_owned()),
                _ => {}
            }
        }
        return AuthorizationInput { code, state };
    }

    if let Some((code, state)) = value.split_once('#') {
        return AuthorizationInput {
            code: Some(code.to_owned()),
            state: Some(state.to_owned()),
        };
    }

    if value.contains("code=") {
        let mut code = None;
        let mut state = None;
        for (name, value) in url::form_urlencoded::parse(value.as_bytes()) {
            match name.as_ref() {
                "code" => code = Some(value.into_owned()),
                "state" => state = Some(value.into_owned()),
                _ => {}
            }
        }
        return AuthorizationInput { code, state };
    }

    AuthorizationInput {
        code: Some(value.to_owned()),
        state: None,
    }
}

/// Decodes the payload of a three-part JWT; `None` matches `decodeJwt`'s `null`.
pub fn decode_jwt_payload(token: &str) -> Option<Value> {
    let mut parts = token.split('.');
    let _header = parts.next()?;
    let payload = parts.next()?;
    let _signature = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    let bytes = base64_decode(payload)?;
    serde_json::from_slice(&bytes).ok()
}

fn base64_decode(value: &str) -> Option<Vec<u8>> {
    let mut output = Vec::with_capacity(value.len() * 3 / 4);
    let mut buffer = 0_u32;
    let mut bits = 0_u32;
    for character in value.bytes() {
        let digit = match character {
            b'A'..=b'Z' => character - b'A',
            b'a'..=b'z' => character - b'a' + 26,
            b'0'..=b'9' => character - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' => break,
            b' ' | b'\t' | b'\r' | b'\n' => continue,
            _ => return None,
        };
        buffer = (buffer << 6) | u32::from(digit);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    Some(output)
}

/// One HTTP request received by a loopback callback server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallbackRequest {
    pub method: String,
    pub path: String,
    pub query: Vec<(String, String)>,
}

impl CallbackRequest {
    pub fn query_param(&self, name: &str) -> Option<String> {
        self.query
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
    }
}

pub struct CallbackReply {
    pub status: u16,
    pub html: String,
    pub no_store: bool,
}

impl CallbackReply {
    pub fn html(status: u16, html: String) -> Self {
        Self {
            status,
            html,
            no_store: false,
        }
    }

    pub fn no_store(status: u16, html: String) -> Self {
        Self {
            status,
            html,
            no_store: true,
        }
    }
}

pub type CallbackHandler = Arc<dyn Fn(CallbackRequest) -> BoxFuture<'static, CallbackReply> + Send + Sync>;

/// A loopback HTTP server serving one OAuth callback route.
pub struct CallbackServer {
    port: u16,
    shutdown: CancellationToken,
    task: JoinHandle<()>,
}

impl CallbackServer {
    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn close(self) {
        self.shutdown.cancel();
        self.task.abort();
    }
}

pub async fn start_callback_server(
    host: &str,
    port: u16,
    handler: CallbackHandler,
) -> std::io::Result<CallbackServer> {
    let listener = TcpListener::bind((host, port)).await?;
    let local_port = listener.local_addr()?.port();
    let shutdown = CancellationToken::new();
    let task_shutdown = shutdown.clone();
    let task = tokio::spawn(async move {
        loop {
            tokio::select! {
                biased;
                () = task_shutdown.cancelled() => break,
                accepted = listener.accept() => {
                    let Ok((stream, _)) = accepted else { continue };
                    let handler = Arc::clone(&handler);
                    tokio::spawn(serve_connection(stream, handler));
                }
            }
        }
    });
    Ok(CallbackServer {
        port: local_port,
        shutdown,
        task,
    })
}

async fn serve_connection(mut stream: tokio::net::TcpStream, handler: CallbackHandler) {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 1024];
    while !buffer.windows(4).any(|window| window == b"\r\n\r\n") {
        match stream.read(&mut chunk).await {
            Ok(0) | Err(_) => return,
            Ok(read) => buffer.extend_from_slice(&chunk[..read]),
        }
        if buffer.len() > 64 * 1024 {
            return;
        }
    }

    let Some(request) = parse_callback_request(&buffer) else {
        let _ = write_reply(&mut stream, CallbackReply::html(400, String::new())).await;
        return;
    };
    let reply = handler(request).await;
    let _ = write_reply(&mut stream, reply).await;
}

fn parse_callback_request(buffer: &[u8]) -> Option<CallbackRequest> {
    let text = String::from_utf8_lossy(buffer);
    let mut request_line = text.lines().next()?.split_whitespace();
    let method = request_line.next()?.to_owned();
    let target = request_line.next()?;
    let url = Url::parse(&format!("http://localhost{target}")).ok()?;
    Some(CallbackRequest {
        method,
        path: url.path().to_owned(),
        query: url
            .query_pairs()
            .map(|(name, value)| (name.into_owned(), value.into_owned()))
            .collect(),
    })
}

async fn write_reply(stream: &mut tokio::net::TcpStream, reply: CallbackReply) -> std::io::Result<()> {
    let body = reply.html.into_bytes();
    let mut headers = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n",
        reply.status,
        status_text(reply.status),
        body.len()
    );
    if reply.no_store {
        headers.push_str("cache-control: no-store\r\n");
    }
    headers.push_str("\r\n");
    stream.write_all(headers.as_bytes()).await?;
    stream.write_all(&body).await?;
    stream.flush().await
}
