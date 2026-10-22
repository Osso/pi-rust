//! Port of `packages/ai/src/providers/cloudflare-stream.ts`.

use pi_js_compat::js_json::JsString;
use std::sync::Arc;

use crate::request::{ProviderStreams, SimpleStreamOptions, StreamOptions};
use crate::types::{Context, Model, ProviderEnv};
use crate::utils::event_stream::AssistantMessageEventStream;

const CLOUDFLARE_ACCOUNT_ID: &str = "CLOUDFLARE_ACCOUNT_ID";
const CLOUDFLARE_GATEWAY_ID: &str = "CLOUDFLARE_GATEWAY_ID";

/// `resolveCloudflareModel`: materializes endpoint placeholders from the env.
pub fn resolve_cloudflare_model(model: &Model, env: Option<&ProviderEnv>) -> Model {
    let Some(env) = env else {
        return model.clone();
    };
    let account_id = env
        .get(CLOUDFLARE_ACCOUNT_ID)
        .cloned()
        .unwrap_or_else(|| "{CLOUDFLARE_ACCOUNT_ID}".into());
    let gateway_id = env
        .get(CLOUDFLARE_GATEWAY_ID)
        .cloned()
        .unwrap_or_else(|| "{CLOUDFLARE_GATEWAY_ID}".into());
    let base_url = JsString::from(&model.base_url)
        .replace(&format!("{{{CLOUDFLARE_ACCOUNT_ID}}}"), &account_id)
        .replace(&format!("{{{CLOUDFLARE_GATEWAY_ID}}}"), &gateway_id);
    if base_url == model.base_url {
        return model.clone();
    }
    let mut resolved = model.clone();
    resolved.base_url = base_url;
    resolved
}

/// `cloudflareStreams(streams)`: resolves the model endpoint before dispatch.
pub struct CloudflareStreams {
    inner: Arc<dyn ProviderStreams>,
}

pub fn cloudflare_streams(streams: Arc<dyn ProviderStreams>) -> CloudflareStreams {
    CloudflareStreams { inner: streams }
}

impl ProviderStreams for CloudflareStreams {
    fn stream(&self, model: Model, context: Context, options: StreamOptions) -> AssistantMessageEventStream {
        let resolved = resolve_cloudflare_model(&model, options.request.env.as_ref());
        self.inner.stream(resolved, context, options)
    }

    fn stream_simple(
        &self,
        model: Model,
        context: Context,
        options: SimpleStreamOptions,
    ) -> AssistantMessageEventStream {
        let resolved = resolve_cloudflare_model(&model, options.stream.request.env.as_ref());
        self.inner.stream_simple(resolved, context, options)
    }
}
