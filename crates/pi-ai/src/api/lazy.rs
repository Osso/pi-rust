//! Port of `packages/ai/src/api/lazy.ts`.

use std::sync::Arc;

use crate::request::{
    BoxFuture, CallbackError, DeferredCancelOptions, DeferredFetchOptions, ProviderStreams, SimpleStreamOptions,
    StreamOptions, error_stream, now_millis,
};
use crate::types::{AssistantMessage, AssistantMessageEvent, Context, DeferredHandle, Model, StopReason};
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
            Ok(mut inner) => forward_stream(&sender, &model, &mut inner).await,
            Err(error) => fail_stream(&sender, &model, &error.message),
        }
    });
    stream
}

async fn forward_stream(
    sender: &crate::utils::event_stream::AssistantMessageEventSender,
    model: &Model,
    source: &mut AssistantMessageEventStream,
) {
    while let Some(event) = source.next().await {
        match event {
            Ok(event) => sender.push(event),
            Err(failure) => {
                fail_stream(sender, model, failure.message());
                return;
            }
        }
    }
    match source.result().await {
        Ok(message) => sender.end(Some(message)),
        Err(failure) => fail_stream(sender, model, failure.message()),
    }
}

fn fail_stream(
    sender: &crate::utils::event_stream::AssistantMessageEventSender,
    model: &Model,
    message: impl Into<pi_js_compat::js_json::JsString>,
) {
    let mut error = AssistantMessage::empty(&model.api, &model.provider, &model.id, now_millis());
    error.stop_reason = StopReason::Error;
    error.error_message = Some(message.into());
    sender.push(AssistantMessageEvent::Error {
        reason: crate::types::ErrorReason::Error,
        error: error.clone(),
    });
    sender.end(Some(error));
}

/// `lazyApi` capabilities.
#[derive(Debug, Clone, Copy, Default)]
pub struct LazyApiCapabilities {
    pub fetch_deferred: bool,
    pub cancel_deferred: bool,
}

pub type ApiLoader = Arc<dyn Fn() -> BoxFuture<'static, Result<Arc<dyn ProviderStreams>, CallbackError>> + Send + Sync>;

/// Wraps a dynamically imported API implementation module as `ProviderStreams`.
pub struct LazyApi {
    load: ApiLoader,
    capabilities: LazyApiCapabilities,
}

pub fn lazy_api(load: ApiLoader, capabilities: LazyApiCapabilities) -> LazyApi {
    LazyApi { load, capabilities }
}

impl ProviderStreams for LazyApi {
    fn stream(&self, model: Model, context: Context, options: StreamOptions) -> AssistantMessageEventStream {
        let load = Arc::clone(&self.load);
        lazy_stream(model.clone(), move || {
            Box::pin(async move {
                let implementation = load().await?;
                Ok(implementation.stream(model, context, options))
            })
        })
    }

    fn stream_simple(
        &self,
        model: Model,
        context: Context,
        options: SimpleStreamOptions,
    ) -> AssistantMessageEventStream {
        let load = Arc::clone(&self.load);
        lazy_stream(model.clone(), move || {
            Box::pin(async move {
                let implementation = load().await?;
                Ok(implementation.stream_simple(model, context, options))
            })
        })
    }

    fn supports_fetch_deferred(&self) -> bool {
        self.capabilities.fetch_deferred
    }

    fn fetch_deferred(
        &self,
        model: Model,
        handle: DeferredHandle,
        options: DeferredFetchOptions,
    ) -> AssistantMessageEventStream {
        if !self.capabilities.fetch_deferred {
            return error_stream(&model, "API does not support deferred responses");
        }
        let load = Arc::clone(&self.load);
        lazy_stream(model.clone(), move || {
            Box::pin(async move {
                let implementation = load().await?;
                if !implementation.supports_fetch_deferred() {
                    return Err(CallbackError::new("API does not support deferred responses"));
                }
                Ok(implementation.fetch_deferred(model, handle, options))
            })
        })
    }

    fn supports_cancel_deferred(&self) -> bool {
        self.capabilities.cancel_deferred
    }

    fn cancel_deferred(
        &self,
        model: Model,
        handle: DeferredHandle,
        options: DeferredCancelOptions,
    ) -> BoxFuture<'_, Result<(), CallbackError>> {
        if !self.capabilities.cancel_deferred {
            return Box::pin(async { Err(CallbackError::new("API cannot cancel deferred responses")) });
        }
        let load = Arc::clone(&self.load);
        Box::pin(async move {
            let implementation = load().await?;
            if !implementation.supports_cancel_deferred() {
                return Err(CallbackError::new("API cannot cancel deferred responses"));
            }
            implementation.cancel_deferred(model, handle, options).await
        })
    }
}
