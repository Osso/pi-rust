//! Default [`Fetch`] implementation (the `globalThis.fetch` stand-in) backed by `reqwest`.

use std::error::Error;
use std::sync::{Arc, OnceLock, RwLock};
use std::time::Duration;

use pi_js_compat::abort::{AbortReason, AbortSignal};

use url::Url;

use crate::request::{BoxFuture, CallbackError, Fetch, HttpBody, HttpRequest, HttpResponse, header_bytes};

/// Message of the `AbortError` thrown by `fetch` when its signal aborts.
pub const ABORT_ERROR_MESSAGE: &str = "This operation was aborted";

mod environment_proxy;

struct ReqwestFetch {
    client: reqwest::Client,
}

struct ReqwestBody {
    response: reqwest::Response,
    signal: Option<AbortSignal>,
}

fn default_fetch_slot() -> &'static RwLock<Arc<dyn Fetch>> {
    static DEFAULT: OnceLock<RwLock<Arc<dyn Fetch>>> = OnceLock::new();
    DEFAULT.get_or_init(|| {
        RwLock::new(Arc::new(ReqwestFetch {
            client: reqwest::Client::new(),
        }))
    })
}

/// Process-wide default HTTP authority; explicit per-request Fetch overrides are separate.
pub fn default_fetch() -> Arc<dyn Fetch> {
    Arc::clone(
        &default_fetch_slot()
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    )
}

/// Installs the dispatcher used by subsequent default provider requests.
pub fn install_default_fetch(fetch: Arc<dyn Fetch>) {
    *default_fetch_slot()
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = fetch;
}

/// Send a request through reqwest, optionally through `proxy`.
pub(crate) async fn send_reqwest(
    request: HttpRequest,
    signal: Option<AbortSignal>,
    proxy: Option<&Url>,
) -> Result<HttpResponse, CallbackError> {
    let Some(proxy) = proxy else {
        return default_fetch().fetch(request, signal).await;
    };
    let proxy = reqwest::Proxy::all(proxy.as_str()).map_err(to_callback_error)?;
    let client = reqwest::Client::builder()
        .no_proxy()
        .proxy(proxy)
        .build()
        .map_err(to_callback_error)?;
    send_with_client(&client, request, signal).await
}

async fn send_with_client(
    client: &reqwest::Client,
    request: HttpRequest,
    signal: Option<AbortSignal>,
) -> Result<HttpResponse, CallbackError> {
    let builder = build_request(client, request)?;
    let response = until_aborted(signal.as_ref(), CallbackError::from_rejection, async {
        builder.send().await.map_err(to_send_error)
    })
    .await?;
    Ok(into_http_response(response, signal))
}

/// Options for a configured default client.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FetchClientOptions {
    /// `allowH2: false` — HTTP/1.1 only.
    pub http1_only: bool,
    /// Per-read idle timeout (`bodyTimeout`/`headersTimeout`).
    pub idle_timeout: Option<Duration>,
}

/// Build a [`Fetch`] from [`FetchClientOptions`] instead of the process-wide default.
pub fn fetch_with_options(options: FetchClientOptions) -> Result<Arc<dyn Fetch>, String> {
    let mut builder = reqwest::Client::builder()
        .no_proxy()
        .proxy(environment_proxy::capture_environment_proxy()?);
    if options.http1_only {
        builder = builder.http1_only();
    }
    if let Some(idle_timeout) = options.idle_timeout {
        builder = builder.read_timeout(idle_timeout);
    }
    let client = builder.build().map_err(|error| error.to_string())?;
    Ok(Arc::new(ReqwestFetch { client }))
}

impl Fetch for ReqwestFetch {
    fn fetch(
        &self,
        request: HttpRequest,
        signal: Option<AbortSignal>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        Box::pin(send_with_client(&self.client, request, signal))
    }
}

impl HttpBody for ReqwestBody {
    fn next_chunk(&mut self) -> BoxFuture<'_, Option<Result<Vec<u8>, CallbackError>>> {
        Box::pin(async move {
            let signal = self.signal.clone();
            let chunk = until_aborted(signal.as_ref(), body_abort_error, async {
                self.response.chunk().await.map_err(to_body_error)
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
        let value = reqwest::header::HeaderValue::from_bytes(&header_bytes(&value.as_str().into())?)
            .map_err(|error| CallbackError::new(error.to_string()))?;
        builder = builder.header(name, value);
    }
    if let Some(body) = request.body {
        builder = builder.body(body);
    }
    Ok(builder)
}

fn into_http_response(response: reqwest::Response, signal: Option<AbortSignal>) -> HttpResponse {
    let headers = response
        .headers()
        .iter()
        .map(|(name, value)| {
            (
                name.as_str().to_owned(),
                value.as_bytes().iter().copied().map(char::from).collect(),
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
    signal: Option<&AbortSignal>,
    abort_error: impl FnOnce(AbortReason) -> CallbackError,
    operation: impl Future<Output = Result<T, CallbackError>>,
) -> Result<T, CallbackError> {
    let Some(signal) = signal else {
        return operation.await;
    };
    tokio::select! {
        reason = signal.wait() => Err(abort_error(reason)),
        result = operation => result,
    }
}

fn body_abort_error(reason: AbortReason) -> CallbackError {
    let reason = match reason {
        AbortReason::Value(ref value) if !pi_js_compat::js_json::is_truthy(value) => AbortReason::DomException(
            pi_js_compat::node_error::NativeNodeError::new("AbortError", "The operation was aborted.")
                .with_property("code", pi_js_compat::js_json::JsValue::Number(20.0)),
        ),
        reason => reason,
    };
    CallbackError::from_rejection(reason)
}

fn to_callback_error(error: reqwest::Error) -> CallbackError {
    CallbackError::new(error.to_string())
}

fn to_send_error(error: reqwest::Error) -> CallbackError {
    if error.is_request() {
        native_fetch_error(error, "fetch failed")
    } else {
        to_callback_error(error)
    }
}

fn to_body_error(error: reqwest::Error) -> CallbackError {
    if error.is_body() || error.is_decode() {
        native_fetch_error(error, "terminated")
    } else {
        to_callback_error(error)
    }
}

fn native_fetch_error(error: reqwest::Error, message: &str) -> CallbackError {
    // reqwest attaches the request URL, which can contain credentials/query secrets.
    let error = error.without_url();
    let mut diagnostics = vec![error.to_string()];
    let mut source = error.source();
    while let Some(cause) = source {
        diagnostics.push(cause.to_string());
        source = cause.source();
    }
    let cause = diagnostics.join(": ");
    let error = pi_js_compat::node_error::NativeNodeError::from_constructor(
        pi_js_compat::node_error::NativeErrorClass::TypeError,
        message,
    )
    .with_property("cause", pi_js_compat::js_json::JsValue::String(cause.clone().into()));
    CallbackError {
        message: message.into(),
        name: "TypeError".into(),
        rejection: Some(Box::new(AbortReason::Error(error))),
        code: None,
        cause: Some(cause),
        original: None,
    }
}
