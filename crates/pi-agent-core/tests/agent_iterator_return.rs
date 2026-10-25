use crate::shared_oracle as oracle;
use pi_agent_core::{AgentContext, AgentError, AgentEvent, AgentEventSink, AgentLoopConfig, StreamFn, run_agent_loop};
use pi_ai::providers::faux::{RegisterFauxProviderOptions, faux_provider};
use pi_ai::types::{AssistantContent, AssistantMessage, AssistantMessageEvent, DoneReason, TextContent};
use pi_ai::utils::event_stream::{StreamFailure, create_assistant_message_event_stream};
use pi_js_compat::{js_json as json, js_json::JsValue};
use std::sync::{Arc, Mutex};

async fn execute(scenario: &str) -> JsValue {
    let trace = Arc::new(Mutex::new(Vec::new()));
    let events = Arc::new(Mutex::new(Vec::new()));
    let controller_signal = pi_js_compat::abort::AbortController::new();
    let signal = controller_signal.signal();
    let model = faux_provider(RegisterFauxProviderOptions {
        api: Some("fixture".into()),
        provider: Some("fixture".into()),
        ..Default::default()
    })
    .get_model(None)
    .unwrap();
    let mut message = AssistantMessage::empty("fixture", "fixture", "fixture", 0.0);
    message.content = vec![AssistantContent::Text(TextContent::new("done"))];
    let (sender, mut stream) = create_assistant_message_event_stream();
    match scenario {
        "next-error" => sender.fail(StreamFailure::new("next failed")),
        "next-done" => sender.end(Some(message)),
        _ => {
            sender.push(AssistantMessageEvent::Start {
                partial: message.clone(),
            });
            sender.push(AssistantMessageEvent::Done {
                reason: DoneReason::Stop,
                message,
            });
        }
    }
    let recorded = trace.clone();
    let controller_close_signal = controller_signal.clone();
    let kind = scenario.to_owned();
    stream.set_iterator_return(Some(Arc::new(move || {
        recorded.lock().unwrap().push("return");
        if kind == "sync-return-error" {
            return Err(StreamFailure::new("sync return failed"));
        }
        if kind == "close-abort" {
            controller_close_signal.abort(None);
        }
        let recorded = recorded.clone();
        let reject = kind == "return-error" || kind == "abort-return-error";
        Ok(Box::pin(async move {
            recorded.lock().unwrap().push("closed");
            if reject {
                Err(StreamFailure::new("return failed"))
            } else {
                Ok(())
            }
        }))
    })));
    let stream = Arc::new(Mutex::new(Some(stream)));
    let stream_fn: StreamFn = Arc::new(move |_, _, _| {
        let stream = stream.lock().unwrap().take().unwrap();
        Box::pin(async { Ok(stream) })
    });
    let observed = events.clone();
    let kind = scenario.to_owned();
    let controller_abort = controller_signal.clone();
    let emit: AgentEventSink = Arc::new(move |event| {
        observed.lock().unwrap().push(
            serde_json::to_value(&event).unwrap()["type"]
                .as_str()
                .unwrap()
                .to_owned(),
        );
        let kind = kind.clone();
        let controller_abort = controller_abort.clone();
        Box::pin(async move {
            if matches!(event,AgentEvent::MessageStart {message} if message.as_assistant().is_some()) {
                if kind == "abort" || kind == "abort-return-error" {
                    controller_abort.abort(None);
                }
                if kind == "emit-error" {
                    return Err(AgentError::new("sink failed"));
                }
            }
            Ok(())
        })
    });
    let config = AgentLoopConfig::new(
        model,
        Arc::new(|messages| Box::pin(async move { pi_agent_core::convert_to_llm(&messages) })),
    );
    let prompt = serde_json::from_str(r#"{"role":"user","content":"hello","timestamp":0}"#).unwrap();
    let result = run_agent_loop(
        vec![prompt],
        AgentContext {
            system_prompt: "".into(),
            messages: vec![],
            tools: Some(vec![]),
        },
        config,
        emit,
        Some(signal),
        Some(stream_fn),
    )
    .await;
    tokio::task::yield_now().await;
    let mut value = json!({"trace":trace.lock().unwrap().clone(),"events":events.lock().unwrap().clone()});
    if let Err(error) = result {
        value["error"] = JsValue::String(error.message().clone());
    }
    value
}
#[tokio::test]
async fn iterator_return_matches_frozen_completion_abort_and_failure_ordering() {
    let mut oracle = oracle::TsOracle::spawn("agent-loop-iterator-oracle.ts", "packages/agent-core/src/agent-loop.ts");
    for scenario in [
        "done",
        "abort",
        "next-done",
        "next-error",
        "emit-error",
        "return-error",
        "sync-return-error",
        "abort-return-error",
        "close-abort",
    ] {
        let expected: JsValue = oracle.request(&json!({"scenario":scenario}));
        let actual = execute(scenario).await;
        assert_eq!(actual, expected, "{scenario}");
    }
}

#[tokio::test]
async fn iterator_return_carrier_is_optional_and_preserves_both_failure_channels() {
    let (_, mut stream) = create_assistant_message_event_stream();
    assert!(stream.return_iterator().unwrap().is_none());
    stream.set_iterator_return(Some(Arc::new(|| Err(StreamFailure::new("synchronous")))));
    assert_eq!(stream.return_iterator().err().unwrap().message(), "synchronous");
    stream.set_iterator_return(Some(Arc::new(|| {
        Ok(Box::pin(async { Err(StreamFailure::new("asynchronous")) }))
    })));
    assert_eq!(
        stream.return_iterator().unwrap().unwrap().await.unwrap_err().message(),
        "asynchronous"
    );
    stream.set_iterator_return(None);
    assert!(stream.return_iterator().unwrap().is_none());
}
