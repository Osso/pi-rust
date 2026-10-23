//! Recording/replaying `Fetch` used by OpenAI provider differential tests.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use pi_ai::request::{BoxFuture, CallbackError, Fetch, HttpBody, HttpRequest, HttpResponse};
use pi_js_compat::abort::AbortSignal;

#[derive(Debug, Clone)]
pub struct ScriptedResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub chunks: Vec<Vec<u8>>,
}

impl ScriptedResponse {
    pub fn sse(chunks: &[&str]) -> Self {
        Self {
            status: 200,
            headers: vec![("content-type".to_owned(), "text/event-stream".to_owned())],
            chunks: chunks.iter().map(|chunk| chunk.as_bytes().to_vec()).collect(),
        }
    }

    pub fn json(status: u16, body: &str) -> Self {
        Self {
            status,
            headers: vec![("content-type".to_owned(), "application/json".to_owned())],
            chunks: vec![body.as_bytes().to_vec()],
        }
    }

    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_owned(), value.to_owned()));
        self
    }
}

pub struct RecordingFetch {
    responses: Mutex<VecDeque<ScriptedResponse>>,
    requests: Mutex<Vec<HttpRequest>>,
    abort_after_chunks: Option<usize>,
    controller: Option<pi_js_compat::abort::AbortController>,
}

impl RecordingFetch {
    pub fn new(responses: Vec<ScriptedResponse>) -> Arc<Self> {
        Arc::new(Self {
            responses: Mutex::new(responses.into()),
            requests: Mutex::new(Vec::new()),
            abort_after_chunks: None,
            controller: None,
        })
    }

    pub fn with_abort_after_chunks(
        responses: Vec<ScriptedResponse>,
        abort_after_chunks: usize,
        controller: pi_js_compat::abort::AbortController,
    ) -> Arc<Self> {
        Arc::new(Self {
            responses: Mutex::new(responses.into()),
            requests: Mutex::new(Vec::new()),
            abort_after_chunks: Some(abort_after_chunks),
            controller: Some(controller),
        })
    }

    pub fn requests(&self) -> Vec<HttpRequest> {
        self.requests.lock().expect("requests lock").clone()
    }
}

impl Fetch for RecordingFetch {
    fn fetch(
        &self,
        request: HttpRequest,
        _signal: Option<AbortSignal>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        self.requests.lock().expect("requests lock").push(request);
        let response = self
            .responses
            .lock()
            .expect("responses lock")
            .pop_front()
            .unwrap_or(ScriptedResponse {
                status: 500,
                headers: Vec::new(),
                chunks: vec![b"no scripted response".to_vec()],
            });
        let abort_after = self.abort_after_chunks;
        let controller = self.controller.clone();
        Box::pin(async move {
            Ok(HttpResponse {
                status: response.status,
                // Matches the oracle's `new Response(body, { status })`, whose statusText is "".
                status_text: String::new(),
                headers: response.headers,
                body: Box::new(ScriptedBody {
                    chunks: response.chunks.into(),
                    delivered: 0,
                    abort_after,
                    controller,
                }),
            })
        })
    }
}

struct ScriptedBody {
    chunks: VecDeque<Vec<u8>>,
    delivered: usize,
    abort_after: Option<usize>,
    controller: Option<pi_js_compat::abort::AbortController>,
}

impl HttpBody for ScriptedBody {
    fn next_chunk(&mut self) -> BoxFuture<'_, Option<Result<Vec<u8>, CallbackError>>> {
        if let Some(abort_after) = self.abort_after
            && self.delivered == abort_after
        {
            if let Some(controller) = &self.controller {
                controller.abort(None);
            }
            return Box::pin(async { Some(Err(CallbackError::new("This operation was aborted"))) });
        }
        self.delivered += 1;
        let chunk = self.chunks.pop_front();
        Box::pin(async move { chunk.map(Ok) })
    }
}
