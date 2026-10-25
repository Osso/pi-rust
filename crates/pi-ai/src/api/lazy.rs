//! Port of `packages/ai/src/api/lazy.ts`.

use std::sync::Arc;

use crate::request::{BoxFuture, CallbackError, ProviderStreams, SimpleStreamOptions, StreamOptions, now_millis};
use crate::types::{AssistantMessage, AssistantMessageEvent, Context, Model, StopReason};
use crate::utils::event_stream::{AssistantMessageEventStream, create_assistant_message_event_stream};

/// Async setup run behind a returned stream.
pub type StreamSetup =
    Arc<dyn Fn() -> BoxFuture<'static, Result<AssistantMessageEventStream, CallbackError>> + Send + Sync>;

/// `lazyStream`: returns a stream synchronously while setup runs behind it.
pub fn lazy_stream(
    model: Model,
    setup: impl FnOnce() -> BoxFuture<'static, Result<AssistantMessageEventStream, CallbackError>> + Send + 'static,
) -> AssistantMessageEventStream {
    let (sender, stream) = create_assistant_message_event_stream();
    tokio::spawn(async move {
        match setup().await {
            Ok(mut inner) => forward_stream(&sender, &mut inner).await,
            Err(error) => {
                // Frozen lazy.ts:49–53 catches acquisition; its Error.message/
                // String(error) conversion can itself reject without an event.
                let message = match error.error_or_string_message() {
                    Ok(message) => message,
                    Err(rejection) => pi_js_compat::callback_error::throw_callback_uncaught(rejection),
                };
                fail_stream(&sender, &model, message);
            }
        }
    });
    stream
}

async fn forward_stream(
    sender: &crate::utils::event_stream::AssistantMessageEventSender,
    source: &mut AssistantMessageEventStream,
) {
    while let Some(event) = source.next().await {
        match event {
            Ok(event) => sender.push(event),
            Err(failure) => {
                let error = CallbackError::from_error(failure.message().clone(), failure);
                pi_js_compat::callback_error::throw_callback_uncaught(error);
            }
        }
    }
    // Frozen forwardStream ends iteration without reading source.result().
    // Only a forwarded terminal event settles the outer result.
    sender.end(None);
}

fn fail_stream(
    sender: &crate::utils::event_stream::AssistantMessageEventSender,
    model: &Model,
    message: impl Into<pi_js_compat::js_json::JsString>,
) {
    let mut error = AssistantMessage::empty(&model.api, &model.provider, &model.id, now_millis() as f64);
    error.stop_reason = StopReason::Error;
    error.error_message = Some(message.into());
    sender.push(AssistantMessageEvent::Error {
        reason: crate::types::ErrorReason::Error,
        error: error.clone(),
    });
    sender.end(Some(error));
}

pub type ApiLoader = Arc<dyn Fn() -> BoxFuture<'static, Result<Arc<dyn ProviderStreams>, CallbackError>> + Send + Sync>;

/// Wraps a dynamically imported API implementation module as `ProviderStreams`.
pub struct LazyApi {
    load: ApiLoader,
}

pub fn lazy_api(load: ApiLoader) -> LazyApi {
    LazyApi { load }
}

impl ProviderStreams for LazyApi {
    fn stream(
        &self,
        model: Model,
        context: Context,
        options: StreamOptions,
    ) -> Result<AssistantMessageEventStream, CallbackError> {
        let load = Arc::clone(&self.load);
        // Frozen lazy.ts:68–71 returns the outer stream now; inner synchronous
        // rejection belongs to the async setup catch, not this acquisition.
        Ok(lazy_stream(model.clone(), move || {
            Box::pin(async move {
                let implementation = load().await?;
                implementation.stream(model, context, options)
            })
        }))
    }

    fn stream_simple(
        &self,
        model: Model,
        context: Context,
        options: SimpleStreamOptions,
    ) -> Result<AssistantMessageEventStream, CallbackError> {
        let load = Arc::clone(&self.load);
        Ok(lazy_stream(model.clone(), move || {
            Box::pin(async move {
                let implementation = load().await?;
                implementation.stream_simple(model, context, options)
            })
        }))
    }
}
