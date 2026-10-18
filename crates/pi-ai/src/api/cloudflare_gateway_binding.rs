//! Port of `packages/ai/src/api/cloudflare-gateway-binding.ts`.
//!
//! Translates AI Gateway HTTPS requests into Workers AI binding universal
//! endpoint calls (`env.AI.gateway(id).run({provider, endpoint, headers, query})`).
//! Binding calls are pre-authenticated and return the provider's native wire
//! format as a regular streaming response.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::Value;
use tokio_util::sync::CancellationToken;
use url::{Origin, Url};

use crate::request::{BoxFuture, CallbackError, Fetch, HttpRequest, HttpResponse};

/// Placeholder auth value for binding-routed requests; stripped before the
/// binding call.
pub const CLOUDFLARE_GATEWAY_BINDING_AUTH_SENTINEL: &str = "cloudflare-gateway-binding";

/// One universal-endpoint request entry, as accepted by `AiGateway.run()`.
#[derive(Debug, Clone, PartialEq)]
pub struct AiGatewayUniversalRequest {
    pub provider: String,
    pub endpoint: String,
    pub headers: BTreeMap<String, String>,
    pub query: Value,
}

/// Structural gateway surface of the Workers AI binding (`env.AI.gateway(id)`).
pub trait AiGatewayBindingGateway: Send + Sync {
    fn run(
        &self,
        data: AiGatewayUniversalRequest,
        signal: Option<CancellationToken>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>>;
}

/// Structural type for the Workers AI binding (`env.AI`).
pub trait AiGatewayBinding: Send + Sync {
    fn gateway(&self, id: &str) -> Arc<dyn AiGatewayBindingGateway>;
}

/// `GatewayBindingFetchOptions`.
#[derive(Clone)]
pub struct GatewayBindingFetchOptions {
    pub binding: Arc<dyn AiGatewayBinding>,
    /// Gateway HTTPS prefix: `https://gateway.ai.cloudflare.com/v1/{account}/{gateway}`.
    pub base_url: String,
    /// Gateway name passed to `binding.gateway()`.
    pub gateway: String,
}

/// `Fetch` implementation that routes gateway-prefixed requests through a binding.
pub struct GatewayBindingFetch {
    binding: Arc<dyn AiGatewayBinding>,
    gateway: String,
    base_origin: Origin,
    base_path: String,
}

// Never forwarded to the binding: hop-by-hop/derived headers, and gateway auth
// (binding calls are pre-authenticated; the sentinel must not reach the wire).
const STRIP_HEADERS: [&str; 3] = ["content-length", "host", "cf-aig-authorization"];

/// Create a `fetch` that routes AI Gateway requests through the Workers AI binding.
pub fn create_gateway_binding_fetch(options: GatewayBindingFetchOptions) -> Result<GatewayBindingFetch, CallbackError> {
    let base = Url::parse(&options.base_url)
        .map_err(|error| CallbackError::new(format!("createGatewayBindingFetch: {error}")))?;
    let base_path = if base.path().ends_with('/') {
        base.path().to_owned()
    } else {
        format!("{}/", base.path())
    };
    Ok(GatewayBindingFetch {
        binding: options.binding,
        gateway: options.gateway,
        base_origin: base.origin(),
        base_path,
    })
}

impl GatewayBindingFetch {
    fn prefix(&self) -> String {
        format!("{}{}", self.base_origin.ascii_serialization(), self.base_path)
    }

    fn parse_target(&self, request: &HttpRequest) -> Result<AiGatewayUniversalRequest, CallbackError> {
        let method = request.method.to_uppercase();
        let parsed = self.parse_gateway_url(request, &method)?;
        if method != "POST" {
            return Err(unexpressible_error(&method, &request.url, "only POST is supported"));
        }
        let (provider, endpoint) = split_provider_endpoint(&parsed, &self.base_path)
            .ok_or_else(|| unexpressible_error(&method, &request.url, "missing provider/endpoint path"))?;
        let query = parse_body_query(request, &method)?;
        Ok(AiGatewayUniversalRequest {
            provider,
            endpoint,
            headers: collect_headers(&request.headers),
            query,
        })
    }

    fn parse_gateway_url(&self, request: &HttpRequest, method: &str) -> Result<Url, CallbackError> {
        let parsed = Url::parse(&request.url).ok();
        let in_prefix = parsed
            .as_ref()
            .is_some_and(|parsed| parsed.origin() == self.base_origin && parsed.path().starts_with(&self.base_path));
        if !in_prefix {
            return Err(CallbackError::new(format!(
                "createGatewayBindingFetch: {method} {} is outside the configured gateway prefix ({}); this fetch only serves its gateway-bound client",
                request.url,
                self.prefix(),
            )));
        }
        parsed.ok_or_else(|| CallbackError::new("createGatewayBindingFetch: request URL was lost"))
    }
}

impl Fetch for GatewayBindingFetch {
    fn fetch(
        &self,
        request: HttpRequest,
        signal: Option<CancellationToken>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        Box::pin(async move {
            let target = self.parse_target(&request)?;
            self.binding.gateway(&self.gateway).run(target, signal).await
        })
    }
}

fn unexpressible_error(method: &str, url: &str, reason: &str) -> CallbackError {
    CallbackError::new(format!(
        "createGatewayBindingFetch: cannot express {method} {url} as a universal gateway request ({reason}); route it over HTTPS with gateway auth instead"
    ))
}

fn split_provider_endpoint(parsed: &Url, base_path: &str) -> Option<(String, String)> {
    let rest = &parsed.path()[base_path.len()..];
    let slash = rest.find('/').filter(|slash| *slash > 0)?;
    let provider = rest[..slash].to_owned();
    let endpoint = format!(
        "{}{}",
        &rest[slash + 1..],
        parsed.query().map(|query| format!("?{query}")).unwrap_or_default()
    );
    Some((provider, endpoint))
}

fn parse_body_query(request: &HttpRequest, method: &str) -> Result<Value, CallbackError> {
    let body = request
        .body
        .as_deref()
        .ok_or_else(|| unexpressible_error(method, &request.url, "missing body"))?;
    let body_text = String::from_utf8_lossy(body);
    serde_json::from_str(&body_text).map_err(|_| unexpressible_error(method, &request.url, "non-JSON body"))
}

// Entry header names are lowercased so case-variant duplicates collapse and
// stripping is uniform.
fn collect_headers(headers: &[(String, String)]) -> BTreeMap<String, String> {
    let mut result = BTreeMap::new();
    for (key, value) in headers {
        let name = key.to_ascii_lowercase();
        if !STRIP_HEADERS.contains(&name.as_str()) {
            result.insert(name, value.clone());
        }
    }
    result
}
