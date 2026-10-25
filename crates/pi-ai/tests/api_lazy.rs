#[path = "support/oracle.rs"]
mod oracle;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use pi_ai::api::lazy::{lazy_api, lazy_stream};
use pi_ai::request::{CallbackError, ProviderStreams, SimpleStreamOptions, StreamOptions};
use pi_ai::types::{
    AssistantMessage, AssistantMessageEvent, Context, DoneReason, Modality, Model, ModelCost, ModelCostRates,
    StopReason,
};
use pi_ai::utils::event_stream::{
    AssistantMessageEventSender, AssistantMessageEventStream, create_assistant_message_event_stream,
};

fn model() -> Model {
    Model {
        properties: Default::default(),
        id: "m1".to_owned(),
        name: "m1".into(),
        api: "test-api".to_owned(),
        provider: "p1".to_owned(),
        base_url: ("https://example.test/v1".to_owned()).into(),
        reasoning: false,
        thinking_level_map: None,
        input: vec![Modality::Text],
        cost: ModelCost {
            properties: Default::default(),
            rates: ModelCostRates::default(),
            tiers: None,
        },
        context_window: 10_000.0,
        auto_compaction_threshold: None,
        max_tokens: 1_000.0,
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
    calls: Arc<std::sync::Mutex<Vec<String>>>,
}

impl ProviderStreams for FakeStreams {
    fn stream(
        &self,
        model: Model,
        _context: Context,
        _options: StreamOptions,
    ) -> Result<AssistantMessageEventStream, pi_ai::request::CallbackError> {
        self.calls.lock().unwrap().push(format!("{}:{}", self.label, model.id));
        Ok(completed_stream(&model, "ok"))
    }

    fn stream_simple(
        &self,
        model: Model,
        _context: Context,
        _options: SimpleStreamOptions,
    ) -> Result<AssistantMessageEventStream, pi_ai::request::CallbackError> {
        self.calls.lock().unwrap().push(format!("{}:{}", self.label, model.id));
        Ok(completed_stream(&model, "ok"))
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
async fn lazy_forward_ends_iteration_without_reading_the_inner_result() {
    use serde_json::json;
    use std::time::Duration;

    let model = model();
    let message = done_message(&model, "no terminal event");
    let mut oracle = oracle::TsOracle::spawn("lazy-forward-oracle.ts", "packages/ai/src/api/lazy.ts");
    let expected = oracle.request(&json!({"model": model, "message": message}));
    for inner_result in [None, Some(message.clone())] {
        let (sender, inner) = create_assistant_message_event_stream();
        sender.push(AssistantMessageEvent::Start {
            partial: message.clone(),
        });
        sender.end(inner_result);
        let mut outer = lazy_stream(model.clone(), move || Box::pin(async move { Ok(inner) }));
        let mut events = Vec::new();
        loop {
            let event = tokio::time::timeout(Duration::from_millis(500), outer.next())
                .await
                .expect("forwarding must end iteration without awaiting result()");
            let Some(event) = event else {
                break;
            };
            events.push(event.unwrap());
        }
        let settled = tokio::time::timeout(Duration::from_millis(25), outer.result())
            .await
            .is_ok();
        assert_eq!(json!({"events":events,"resultSettled":settled}), expected);
    }
}

#[tokio::test]
async fn lazy_api_loads_on_first_use_and_forwards_both_stream_methods() {
    let loads = Arc::new(AtomicUsize::new(0));
    let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
    let streams = Arc::new(FakeStreams {
        label: "plain".to_owned(),
        calls: Arc::clone(&calls),
    });
    let api = lazy_api({
        let loads = Arc::clone(&loads);
        let streams = Arc::clone(&streams);
        Arc::new(move || {
            loads.fetch_add(1, Ordering::SeqCst);
            let streams = Arc::clone(&streams);
            Box::pin(async move { Ok(streams as Arc<dyn ProviderStreams>) })
        })
    });

    assert_eq!(loads.load(Ordering::SeqCst), 0);

    let result = api
        .stream_simple(model(), context(), SimpleStreamOptions::default())
        .expect("outer acquisition")
        .result()
        .await
        .unwrap();
    assert_eq!(result.stop_reason, StopReason::Stop);
    assert_eq!(loads.load(Ordering::SeqCst), 1);

    let streamed = api
        .stream(model(), context(), StreamOptions::default())
        .expect("outer acquisition")
        .result()
        .await
        .unwrap();
    assert_eq!(streamed.stop_reason, StopReason::Stop);
    assert_eq!(loads.load(Ordering::SeqCst), 2);

    assert_eq!(calls.lock().unwrap().as_slice(), &["plain:m1", "plain:m1"]);
}

#[tokio::test]
async fn lazy_api_surfaces_load_failure_as_stream_error() {
    let api = lazy_api(Arc::new(|| Box::pin(async { Err(CallbackError::new("load failed")) })));
    let result = api
        .stream(model(), context(), StreamOptions::default())
        .expect("outer acquisition")
        .result()
        .await
        .unwrap();
    assert_eq!(result.stop_reason, StopReason::Error);
    assert_eq!(
        result.error_message.as_ref(),
        Some(&pi_js_compat::js_json::JsString::from("load failed"))
    );
}
