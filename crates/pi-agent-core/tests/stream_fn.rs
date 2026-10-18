use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use pi_agent_core::agent::{AgentError, StreamFn, get_default_stream_fn, set_default_stream_fn};
use pi_ai::types::{AssistantMessage, Context, Model, ModelCost, ModelCostRates};
use pi_ai::utils::event_stream::create_assistant_message_event_stream;

fn model() -> Model {
    Model {
        id: "mock".to_owned(),
        name: "mock".to_owned(),
        api: "openai-responses".to_owned(),
        provider: "openai".to_owned(),
        base_url: "https://example.invalid".to_owned(),
        reasoning: false,
        thinking_level_map: None,
        input: vec![pi_ai::types::Modality::Text],
        cost: ModelCost {
            rates: ModelCostRates::default(),
            tiers: None,
        },
        context_window: 8192.0,
        auto_compaction_threshold: None,
        max_tokens: 2048.0,
        sampling_params: None,
        headers: None,
        compat: None,
    }
}

fn counting_stream_fn(calls: Arc<AtomicUsize>) -> StreamFn {
    Arc::new(move |_model, _context, _options| {
        let calls = Arc::clone(&calls);
        Box::pin(async move {
            calls.fetch_add(1, Ordering::SeqCst);
            let (sender, stream) = create_assistant_message_event_stream();
            sender.push(pi_ai::types::AssistantMessageEvent::Done {
                reason: pi_ai::types::DoneReason::Stop,
                message: AssistantMessage::empty("openai-responses", "openai", "mock", 1),
            });
            stream
        })
    })
}

async fn default_stream_fn_lock() -> tokio::sync::MutexGuard<'static, ()> {
    static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    LOCK.lock().await
}

#[tokio::test]
async fn default_stream_fn_errors_with_exact_message_when_unset() {
    let _guard = default_stream_fn_lock().await;
    set_default_stream_fn(None);
    let error = get_default_stream_fn().err().expect("default stream fn must be absent");
    assert_eq!(error, AgentError::default_stream_fn_missing());
    assert_eq!(
        error.to_string(),
        "No default stream function configured. Pass streamFn explicitly or call setDefaultStreamFn()."
    );
}

#[tokio::test]
async fn default_stream_fn_is_process_global_and_clearable() {
    let _guard = default_stream_fn_lock().await;
    let calls = Arc::new(AtomicUsize::new(0));
    set_default_stream_fn(Some(counting_stream_fn(Arc::clone(&calls))));

    let first = get_default_stream_fn().expect("default must be installed");
    let second = get_default_stream_fn().expect("default must remain installed");
    let mut stream = first(
        model(),
        Context {
            system_prompt: None,
            messages: Vec::new(),
            tools: None,
        },
        Default::default(),
    )
    .await;
    while stream.next().await.is_some() {}

    let mut stream = second(
        model(),
        Context {
            system_prompt: None,
            messages: Vec::new(),
            tools: None,
        },
        Default::default(),
    )
    .await;
    while stream.next().await.is_some() {}

    assert_eq!(calls.load(Ordering::SeqCst), 2);

    set_default_stream_fn(None);
    assert!(get_default_stream_fn().is_err());
}
