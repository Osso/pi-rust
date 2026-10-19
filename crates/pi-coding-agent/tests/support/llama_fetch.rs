//! Shared `Fetch` stub for the llama.cpp extension tests.

use std::sync::{Arc, Mutex};

use pi_ai::request::{BoxFuture, CallbackError, Fetch, HttpBody, HttpRequest, HttpResponse};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

pub(crate) struct VecBody {
    pub(crate) chunks: Vec<Vec<u8>>,
}

impl HttpBody for VecBody {
    fn next_chunk(&mut self) -> BoxFuture<'_, Option<Result<Vec<u8>, CallbackError>>> {
        let chunk = if self.chunks.is_empty() {
            None
        } else {
            Some(Ok(self.chunks.remove(0)))
        };
        Box::pin(async move { chunk })
    }
}

pub(crate) fn json_response(status: u16, value: &Value) -> HttpResponse {
    HttpResponse {
        status,
        status_text: String::new(),
        headers: vec![("content-type".to_owned(), "application/json".to_owned())],
        body: Box::new(VecBody {
            chunks: vec![serde_json::to_vec(value).unwrap()],
        }),
    }
}

pub(crate) type Handler = Arc<dyn Fn(&HttpRequest) -> HttpResponse + Send + Sync>;

pub(crate) struct HandlerFetch {
    handler: Handler,
    requests: Mutex<Vec<HttpRequest>>,
}

impl HandlerFetch {
    pub(crate) fn new(handler: Handler) -> Arc<Self> {
        Arc::new(Self {
            handler,
            requests: Mutex::new(Vec::new()),
        })
    }

    pub(crate) fn requests(&self) -> Vec<HttpRequest> {
        self.requests.lock().unwrap().clone()
    }
}

impl Fetch for HandlerFetch {
    fn fetch(
        &self,
        request: HttpRequest,
        _signal: Option<CancellationToken>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        self.requests.lock().unwrap().push(request.clone());
        let response = (self.handler)(&request);
        Box::pin(async move { Ok(response) })
    }
}

pub(crate) fn header(request: &HttpRequest, name: &str) -> Option<String> {
    request
        .headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.clone())
}
