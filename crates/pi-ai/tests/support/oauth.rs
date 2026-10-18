#![allow(dead_code)]

//! Shared test doubles for OAuth flow tests.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};

use pi_ai::auth::oauth::support::{OAuthRuntime, RandomSource};
use pi_ai::auth::{AuthEvent, AuthInteraction, AuthPrompt, AuthPromptKind};
use pi_ai::request::{BoxFuture, CallbackError, Fetch};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

pub static CALLBACK_PORT_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Serializes tests that bind a flow's fixed loopback port.
pub async fn lock_callback_port() -> tokio::sync::MutexGuard<'static, ()> {
    CALLBACK_PORT_LOCK.lock().await
}

#[allow(dead_code)]
#[path = "recording_fetch.rs"]
pub mod recording_fetch;

use recording_fetch::RecordingFetch;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedRequest {
    pub method: String,
    pub url: String,
    pub headers: BTreeMap<String, String>,
    pub body: String,
}

pub fn recorded_requests(fetch: &RecordingFetch) -> Vec<RecordedRequest> {
    fetch
        .requests()
        .into_iter()
        .map(|request| RecordedRequest {
            method: request.method,
            url: request.url,
            headers: request.headers.into_iter().collect(),
            body: String::from_utf8_lossy(request.body.as_deref().unwrap_or_default()).into_owned(),
        })
        .collect()
}

/// Deterministic random source: `fill` writes `(index + 1)` bytes and `uuid`
/// returns a fixed value, matching the differential oracle's patches.
pub struct TestRandom {
    pub uuid: String,
}

impl TestRandom {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            uuid: "00000000-0000-4000-8000-000000000000".to_owned(),
        })
    }
}

impl RandomSource for TestRandom {
    fn fill(&self, bytes: &mut [u8]) {
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = (index % 255 + 1) as u8;
        }
    }

    fn uuid(&self) -> String {
        self.uuid.clone()
    }
}

pub fn runtime(fetch: Arc<dyn Fetch>, now: f64) -> OAuthRuntime {
    OAuthRuntime::new(fetch, Arc::new(move || now), TestRandom::new() as Arc<dyn RandomSource>)
}

#[derive(Debug, Clone)]
pub struct PromptRecord {
    pub kind: AuthPromptKind,
    pub signal: Option<CancellationToken>,
}

#[derive(Debug, Clone)]
pub enum PromptAnswer {
    Text(String),
    Error(String),
    Pending,
}

pub struct TestInteraction {
    signal: CancellationToken,
    answers: Mutex<VecDeque<PromptAnswer>>,
    prompts: Mutex<Vec<PromptRecord>>,
    events: Mutex<Vec<AuthEvent>>,
}

impl TestInteraction {
    pub fn new(answers: Vec<PromptAnswer>) -> Arc<Self> {
        Arc::new(Self {
            signal: CancellationToken::new(),
            answers: Mutex::new(VecDeque::from(answers)),
            prompts: Mutex::new(Vec::new()),
            events: Mutex::new(Vec::new()),
        })
    }

    pub fn prompts(&self) -> Vec<PromptRecord> {
        self.prompts.lock().expect("prompts mutex").clone()
    }

    pub fn events(&self) -> Vec<AuthEvent> {
        self.events.lock().expect("events mutex").clone()
    }
}

impl AuthInteraction for TestInteraction {
    fn signal(&self) -> CancellationToken {
        self.signal.clone()
    }

    fn prompt(&self, prompt: AuthPrompt) -> BoxFuture<'_, Result<String, CallbackError>> {
        self.prompts.lock().expect("prompts mutex").push(PromptRecord {
            kind: prompt.kind,
            signal: prompt.signal,
        });
        let answer = self
            .answers
            .lock()
            .expect("answers mutex")
            .pop_front()
            .unwrap_or(PromptAnswer::Error("unexpected prompt".to_owned()));
        Box::pin(async move {
            match answer {
                PromptAnswer::Text(text) => Ok(text),
                PromptAnswer::Error(message) => Err(CallbackError::new(message)),
                PromptAnswer::Pending => std::future::pending().await,
            }
        })
    }

    fn notify(&self, event: AuthEvent) {
        self.events.lock().expect("events mutex").push(event);
    }
}

/// Minimal HTTP GET used by callback-flow tests.
pub async fn http_get(url: &str) -> (u16, String) {
    let parsed = url::Url::parse(url).expect("test callback URL");
    let host = parsed.host_str().expect("host");
    let port = parsed.port_or_known_default().expect("port");
    let mut stream = tokio::net::TcpStream::connect((host, port))
        .await
        .expect("callback server must accept connections");
    let target = match parsed.query() {
        Some(query) => format!("{}?{query}", parsed.path()),
        None => parsed.path().to_owned(),
    };
    let request = format!("GET {target} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).await.expect("write request");
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.expect("read response");
    let response = String::from_utf8_lossy(&response).into_owned();
    let (head, body) = response.split_once("\r\n\r\n").unwrap_or((&response, ""));
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|status| status.parse().ok())
        .expect("status code");
    (status, body.to_owned())
}
