use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use pi_ai::api::lazy::{LazyApiCapabilities, lazy_api, lazy_stream};
use pi_ai::request::{
    BoxFuture, CallbackError, DeferredCancelOptions, DeferredFetchOptions, ProviderRequestOptions, ProviderStreams,
    SimpleStreamOptions, StreamOptions,
};
use pi_ai::types::{
    AssistantMessage, AssistantMessageEvent, Context, DeferredHandle, DoneReason, Modality, Model, ModelCost,
    ModelCostRates, StopReason,
};
use pi_ai::utils::event_stream::{
    AssistantMessageEventSender, AssistantMessageEventStream, create_assistant_message_event_stream,
};

fn model() -> Model {
    Model {
        id: "m1".to_owned(),
        name: "m1".to_owned(),
        api: "test-api".to_owned(),
        provider: "p1".to_owned(),
        base_url: ("https://example.test/v1".to_owned()).into(),
        reasoning: false,
        thinking_level_map: None,
        input: vec![Modality::Text],
        cost: ModelCost {
            rates: ModelCostRates::default(),
            tiers: None,
        },
        context_window: 10_000.0,
        auto_compaction_threshold: None,
        max_tokens: 1_000.0,
        sampling_params: None,
        headers: None,
        compat: None,
    }
}

fn context() -> Context {
    Context {
        system_prompt: None,
        messages: Vec::new(),
        tools: None,
    }
}

fn done_message(model: &Model, text: &str) -> AssistantMessage {
    let mut message = AssistantMessage::empty(&model.api, &model.provider, &model.id, 1.0);
    message.content = vec![pi_ai::types::AssistantContent::Text(pi_ai::types::TextContent::new(
        text,
    ))];
    message.stop_reason = StopReason::Stop;
    message
}

fn completed_stream(model: &Model, text: &str) -> AssistantMessageEventStream {
    let (sender, stream) = create_assistant_message_event_stream();
    push_completed(&sender, model, text);
    stream
}

fn push_completed(sender: &AssistantMessageEventSender, model: &Model, text: &str) {
    let message = done_message(model, text);
    sender.push(AssistantMessageEvent::Start {
        partial: message.clone(),
    });
    sender.push(AssistantMessageEvent::Done {
        reason: DoneReason::Stop,
        message: message.clone(),
    });
    sender.end(Some(message));
}

struct FakeStreams {
    label: String,
    supports_fetch: bool,
    supports_cancel: bool,
    calls: Arc<std::sync::Mutex<Vec<String>>>,
}

impl ProviderStreams for FakeStreams {
    fn stream(&self, model: Model, _context: Context, _options: StreamOptions) -> AssistantMessageEventStream {
        self.calls.lock().unwrap().push(format!("{}:{}", self.label, model.id));
        completed_stream(&model, "ok")
    }

    fn stream_simple(
        &self,
        model: Model,
        _context: Context,
        _options: SimpleStreamOptions,
    ) -> AssistantMessageEventStream {
        self.calls.lock().unwrap().push(format!("{}:{}", self.label, model.id));
        completed_stream(&model, "ok")
    }

    fn supports_fetch_deferred(&self) -> bool {
        self.supports_fetch
    }

    fn fetch_deferred(
        &self,
        model: Model,
        _handle: DeferredHandle,
        _options: DeferredFetchOptions,
    ) -> AssistantMessageEventStream {
        completed_stream(&model, "fetched")
    }

    fn supports_cancel_deferred(&self) -> bool {
        self.supports_cancel
    }

    fn cancel_deferred(
        &self,
        _model: Model,
        _handle: DeferredHandle,
        _options: DeferredCancelOptions,
    ) -> BoxFuture<'_, Result<(), CallbackError>> {
        Box::pin(async { Ok(()) })
    }
}

fn handle() -> DeferredHandle {
    DeferredHandle {
        provider: "p1".to_owned(),
        model_id: "m1".to_owned(),
        api: "test-api".to_owned(),
        id: "response-1".to_owned(),
        expires_at: None,
        poll_after_ms: None,
        data: None,
    }
}

#[tokio::test]
async fn lazy_stream_turns_setup_failure_into_an_error_event() {
    let model = model();
    let mut stream = lazy_stream(model.clone(), move || {
        Box::pin(async move { Err(CallbackError::new("setup boom")) })
    });
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event.unwrap());
    }
    assert_eq!(events.len(), 1);
    match &events[0] {
        AssistantMessageEvent::Error { reason, error } => {
            assert_eq!(*reason, pi_ai::types::ErrorReason::Error);
            assert_eq!(error.stop_reason, StopReason::Error);
            assert_eq!(
                error.error_message.as_ref(),
                Some(&pi_js_compat::js_json::JsString::from("setup boom"))
            );
            assert_eq!(error.api, model.api);
            assert_eq!(error.provider, model.provider);
            assert_eq!(error.model, model.id);
        }
        other => panic!("expected error event, got {other:?}"),
    }
    let result = stream.result().await.unwrap();
    assert_eq!(result.stop_reason, StopReason::Error);
    assert_eq!(
        result.error_message.as_ref(),
        Some(&pi_js_compat::js_json::JsString::from("setup boom"))
    );
}

#[tokio::test]
async fn lazy_stream_forwards_events_and_result() {
    let model = model();
    let inner = completed_stream(&model, "forwarded");
    let mut stream = lazy_stream(model, move || Box::pin(async move { Ok(inner) }));
    let mut types = Vec::new();
    while let Some(event) = stream.next().await {
        types.push(format!("{:?}", std::mem::discriminant(&event.unwrap())));
    }
    assert_eq!(types.len(), 2);
    let result = stream.result().await.unwrap();
    assert_eq!(result.stop_reason, StopReason::Stop);
}

#[tokio::test]
async fn lazy_api_loads_on_first_use_and_exposes_only_declared_capabilities() {
    let loads = Arc::new(AtomicUsize::new(0));
    let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
    let streams = Arc::new(FakeStreams {
        label: "deferred".to_owned(),
        supports_fetch: true,
        supports_cancel: true,
        calls: Arc::clone(&calls),
    });
    let api = lazy_api(
        {
            let loads = Arc::clone(&loads);
            let streams = Arc::clone(&streams);
            Arc::new(move || {
                loads.fetch_add(1, Ordering::SeqCst);
                let streams = Arc::clone(&streams);
                Box::pin(async move { Ok(streams as Arc<dyn ProviderStreams>) })
            })
        },
        LazyApiCapabilities {
            fetch_deferred: true,
            cancel_deferred: false,
        },
    );

    assert_eq!(loads.load(Ordering::SeqCst), 0);
    assert!(api.supports_fetch_deferred());
    assert!(!api.supports_cancel_deferred());

    let result = api
        .stream_simple(model(), context(), SimpleStreamOptions::default())
        .result()
        .await
        .unwrap();
    assert_eq!(result.stop_reason, StopReason::Stop);
    assert_eq!(loads.load(Ordering::SeqCst), 1);

    let fetched = api
        .fetch_deferred(model(), handle(), DeferredFetchOptions::default())
        .result()
        .await
        .unwrap();
    assert_eq!(fetched.stop_reason, StopReason::Stop);
    assert_eq!(loads.load(Ordering::SeqCst), 2);

    let cancel_error = api
        .cancel_deferred(model(), handle(), ProviderRequestOptions::default())
        .await
        .unwrap_err();
    assert_eq!(cancel_error.message, "API cannot cancel deferred responses");
    assert_eq!(calls.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn lazy_api_fetch_deferred_without_capability_is_an_error_stream() {
    let streams = Arc::new(FakeStreams {
        label: "plain".to_owned(),
        supports_fetch: false,
        supports_cancel: false,
        calls: Arc::new(std::sync::Mutex::new(Vec::new())),
    });
    let api = lazy_api(
        Arc::new(move || {
            let streams = Arc::clone(&streams);
            Box::pin(async move { Ok(streams as Arc<dyn ProviderStreams>) })
        }),
        LazyApiCapabilities::default(),
    );
    let result = api
        .fetch_deferred(model(), handle(), DeferredFetchOptions::default())
        .result()
        .await
        .unwrap();
    assert_eq!(result.stop_reason, StopReason::Error);
    assert_eq!(
        result.error_message.as_ref(),
        Some(&pi_js_compat::js_json::JsString::from(
            "API does not support deferred responses"
        ))
    );
}

#[tokio::test]
async fn lazy_api_surfaces_load_failure_as_stream_error() {
    let api = lazy_api(
        Arc::new(|| Box::pin(async { Err(CallbackError::new("load failed")) })),
        LazyApiCapabilities::default(),
    );
    let result = api
        .stream(model(), context(), StreamOptions::default())
        .result()
        .await
        .unwrap();
    assert_eq!(result.stop_reason, StopReason::Error);
    assert_eq!(
        result.error_message.as_ref(),
        Some(&pi_js_compat::js_json::JsString::from("load failed"))
    );
}
