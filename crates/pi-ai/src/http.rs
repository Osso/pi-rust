//! Default [`Fetch`] implementation (the `globalThis.fetch` stand-in) backed by `reqwest`.

use std::sync::{Arc, OnceLock};

use tokio_util::sync::CancellationToken;

use crate::request::{BoxFuture, CallbackError, Fetch, HttpBody, HttpRequest, HttpResponse};

/// Message of the `AbortError` thrown by `fetch` when its signal aborts.
pub const ABORT_ERROR_MESSAGE: &str = "This operation was aborted";

struct ReqwestFetch {
    client: reqwest::Client,
}

struct ReqwestBody {
    response: reqwest::Response,
    signal: Option<CancellationToken>,
}

/// Process-wide default HTTP client.
pub fn default_fetch() -> Arc<dyn Fetch> {
    static DEFAULT: OnceLock<Arc<dyn Fetch>> = OnceLock::new();
    Arc::clone(DEFAULT.get_or_init(|| {
        Arc::new(ReqwestFetch {
            client: reqwest::Client::new(),
        })
    }))
}

impl Fetch for ReqwestFetch {
    fn fetch(
        &self,
        request: HttpRequest,
        signal: Option<CancellationToken>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        Box::pin(async move {
            let builder = build_request(&self.client, request)?;
            let response = until_aborted(signal.as_ref(), async {
                builder.send().await.map_err(to_callback_error)
            })
            .await?;
            Ok(into_http_response(response, signal))
        })
    }
}

impl HttpBody for ReqwestBody {
    fn next_chunk(&mut self) -> BoxFuture<'_, Option<Result<Vec<u8>, CallbackError>>> {
        Box::pin(async move {
            let signal = self.signal.clone();
            let chunk = until_aborted(signal.as_ref(), async {
                self.response.chunk().await.map_err(to_callback_error)
            })
            .await;
            match chunk {
                Ok(Some(bytes)) => Some(Ok(bytes.to_vec())),
                Ok(None) => None,
                Err(error) => Some(Err(error)),
            }
        })
    }
}

fn build_request(client: &reqwest::Client, request: HttpRequest) -> Result<reqwest::RequestBuilder, CallbackError> {
    let method = reqwest::Method::from_bytes(request.method.as_bytes())
        .map_err(|error| CallbackError::new(error.to_string()))?;
    let mut builder = client.request(method, &request.url);
    for (name, value) in &request.headers {
        builder = builder.header(name, value);
    }
    if let Some(body) = request.body {
        builder = builder.body(body);
    }
    Ok(builder)
}

fn into_http_response(response: reqwest::Response, signal: Option<CancellationToken>) -> HttpResponse {
    let headers = response
        .headers()
        .iter()
        .map(|(name, value)| {
            (
                name.as_str().to_owned(),
                String::from_utf8_lossy(value.as_bytes()).into_owned(),
            )
        })
        .collect();
    HttpResponse {
        status: response.status().as_u16(),
        status_text: response.status().canonical_reason().unwrap_or_default().to_owned(),
        headers,
        body: Box::new(ReqwestBody { response, signal }),
    }
}

async fn until_aborted<T>(
    signal: Option<&CancellationToken>,
    operation: impl Future<Output = Result<T, CallbackError>>,
) -> Result<T, CallbackError> {
    let Some(signal) = signal else {
        return operation.await;
    };
    tokio::select! {
        () = signal.cancelled() => Err(CallbackError::new(ABORT_ERROR_MESSAGE)),
        result = operation => result,
    }
}

fn to_callback_error(error: reqwest::Error) -> CallbackError {
    CallbackError::new(error.to_string())
}
