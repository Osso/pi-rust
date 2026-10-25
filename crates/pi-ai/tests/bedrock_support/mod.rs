#![allow(dead_code)]

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use pi_ai::api::bedrock_eventstream::crc32;
use pi_ai::request::{BoxFuture, CallbackError, Fetch, HttpBody, HttpRequest, HttpResponse};
use pi_js_compat::abort::AbortSignal;

pub struct ScriptedResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub chunks: Vec<Vec<u8>>,
}

impl ScriptedResponse {
    pub fn ok(body: Vec<u8>, chunk_sizes: &[usize]) -> Self {
        Self {
            status: 200,
            headers: vec![
                (
                    "content-type".to_owned(),
                    "application/vnd.amazon.eventstream".to_owned(),
                ),
                ("x-amzn-requestid".to_owned(), "req-1".to_owned()),
            ],
            chunks: split(body, chunk_sizes),
        }
    }

    pub fn http(status: u16, headers: Vec<(&str, &str)>, body: &str) -> Self {
        Self {
            status,
            headers: headers
                .into_iter()
                .map(|(name, value)| (name.to_owned(), value.to_owned()))
                .collect(),
            chunks: vec![body.as_bytes().to_vec()],
        }
    }
}

fn split(body: Vec<u8>, sizes: &[usize]) -> Vec<Vec<u8>> {
    if sizes.is_empty() {
        return vec![body];
    }
    let mut chunks = Vec::new();
    let mut index = 0;
    for size in sizes {
        if index >= body.len() {
            break;
        }
        let end = (index + size).min(body.len());
        chunks.push(body[index..end].to_vec());
        index = end;
    }
    if index < body.len() {
        chunks.push(body[index..].to_vec());
    }
    chunks
}

pub struct ScriptedFetch {
    requests: Mutex<Vec<HttpRequest>>,
    responses: Mutex<VecDeque<ScriptedResponse>>,
}

impl ScriptedFetch {
    pub fn new(responses: Vec<ScriptedResponse>) -> Arc<Self> {
        Arc::new(Self {
            requests: Mutex::new(Vec::new()),
            responses: Mutex::new(responses.into()),
        })
    }

    pub fn requests(&self) -> Vec<HttpRequest> {
        self.requests.lock().expect("request lock").clone()
    }

    pub fn request_count(&self) -> usize {
        self.requests.lock().expect("request lock").len()
    }
}

impl Fetch for ScriptedFetch {
    fn fetch(
        &self,
        request: HttpRequest,
        signal: Option<AbortSignal>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        Box::pin(async move {
            self.requests.lock().expect("request lock").push(request);
            if signal.as_ref().is_some_and(|signal| signal.aborted()) {
                return Err(CallbackError::new(pi_ai::http::ABORT_ERROR_MESSAGE));
            }
            let response = self
                .responses
                .lock()
                .expect("response lock")
                .pop_front()
                .expect("scripted response must exist");
            Ok(HttpResponse {
                status: response.status,
                status_text: String::new(),
                headers: response.headers,
                body: Box::new(ScriptedBody {
                    chunks: response.chunks.into(),
                    signal,
                }),
            })
        })
    }
}

struct ScriptedBody {
    chunks: VecDeque<Vec<u8>>,
    signal: Option<AbortSignal>,
}

impl HttpBody for ScriptedBody {
    fn next_chunk(&mut self) -> BoxFuture<'_, Option<Result<Vec<u8>, CallbackError>>> {
        Box::pin(async move {
            if self.signal.as_ref().is_some_and(|signal| signal.aborted()) {
                return Some(Err(CallbackError::new(pi_ai::http::ABORT_ERROR_MESSAGE)));
            }
            self.chunks.pop_front().map(Ok)
        })
    }
}

pub fn encode_frame(headers: &[(&str, &str)], payload: &[u8], corrupt_message_crc: bool) -> Vec<u8> {
    let mut header_bytes = Vec::new();
    for (name, value) in headers {
        header_bytes.push(name.len() as u8);
        header_bytes.extend_from_slice(name.as_bytes());
        header_bytes.push(7);
        header_bytes.extend_from_slice(&(value.len() as u16).to_be_bytes());
        header_bytes.extend_from_slice(value.as_bytes());
    }
    let total_length = 12 + header_bytes.len() + payload.len() + 4;
    let mut frame = vec![0u8; total_length];
    frame[0..4].copy_from_slice(&(total_length as u32).to_be_bytes());
    frame[4..8].copy_from_slice(&(header_bytes.len() as u32).to_be_bytes());
    let prelude_crc = crc32(&frame[0..8]);
    frame[8..12].copy_from_slice(&prelude_crc.to_be_bytes());
    frame[12..12 + header_bytes.len()].copy_from_slice(&header_bytes);
    frame[12 + header_bytes.len()..total_length - 4].copy_from_slice(payload);
    let mut message_crc = crc32(&frame[0..total_length - 4]);
    if corrupt_message_crc {
        message_crc ^= 1;
    }
    frame[total_length - 4..].copy_from_slice(&message_crc.to_be_bytes());
    frame
}

pub fn event_frame(event_type: &str, payload: serde_json::Value) -> Vec<u8> {
    encode_frame(
        &[
            (":message-type", "event"),
            (":event-type", event_type),
            (":content-type", "application/json"),
        ],
        serde_json::to_string(&payload).expect("payload").as_bytes(),
        false,
    )
}

pub fn exception_frame(exception_type: &str, payload: serde_json::Value) -> Vec<u8> {
    encode_frame(
        &[
            (":message-type", "exception"),
            (":exception-type", exception_type),
            (":content-type", "application/json"),
        ],
        serde_json::to_string(&payload).expect("payload").as_bytes(),
        false,
    )
}

pub fn message_start() -> Vec<u8> {
    event_frame("messageStart", serde_json::json!({"role": "assistant"}))
}

pub fn text_delta(index: i64, text: &str) -> Vec<u8> {
    event_frame(
        "contentBlockDelta",
        serde_json::json!({"contentBlockIndex": index, "delta": {"text": text}}),
    )
}

pub fn block_stop(index: i64) -> Vec<u8> {
    event_frame("contentBlockStop", serde_json::json!({"contentBlockIndex": index}))
}

pub fn message_stop(reason: &str) -> Vec<u8> {
    event_frame("messageStop", serde_json::json!({"stopReason": reason}))
}

pub fn metadata(input: i64, output: i64, cache_read: i64, cache_write: i64, total: i64) -> Vec<u8> {
    event_frame(
        "metadata",
        serde_json::json!({
            "usage": {
                "inputTokens": input,
                "outputTokens": output,
                "cacheReadInputTokens": cache_read,
                "cacheWriteInputTokens": cache_write,
                "totalTokens": total,
            }
        }),
    )
}

pub fn model() -> pi_ai::types::Model {
    pi_ai::types::Model {
        properties: Default::default(),
        id: "us.anthropic.claude-opus-4-8".to_owned(),
        name: "Claude Opus 4.8".into(),
        api: "bedrock-converse-stream".to_owned(),
        provider: "amazon-bedrock".to_owned(),
        base_url: ("https://bedrock-runtime.us-east-1.amazonaws.com".to_owned()).into(),
        reasoning: true,
        thinking_level_map: None,
        input: vec![pi_ai::types::Modality::Text, pi_ai::types::Modality::Image],
        cost: pi_ai::types::ModelCost {
            properties: Default::default(),
            rates: pi_ai::types::ModelCostRates {
                input: 3.0,
                output: 15.0,
                cache_read: 0.3,
                cache_write: 3.75,
            },
        },
        context_window: 200_000.0,
        auto_compaction_threshold: None,
        max_tokens: 64_000.0,
        headers: None,
        compat: None,
    }
}

pub fn env() -> pi_ai::types::ProviderEnv {
    let mut env = pi_ai::types::ProviderEnv::new();
    env.insert("AWS_ACCESS_KEY_ID".to_owned(), ("AKIDEXAMPLE".to_owned()).into());
    env.insert(
        "AWS_SECRET_ACCESS_KEY".to_owned(),
        ("wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY".to_owned()).into(),
    );
    env.insert("AWS_REGION".to_owned(), ("us-east-1".to_owned()).into());
    env
}
