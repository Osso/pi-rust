pub use crate::shared_agent_support as support;

use pi_js_compat::js_json::JsString;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use pi_agent_core::agent::{
    Agent, AgentEvent, AgentListener, AgentMessage, AgentOptions, AgentTool, AgentToolResult, ContinueOptions,
    PromptInput,
};
use pi_ai::request::SimpleStreamOptions;
use pi_ai::types::{AssistantContent, AssistantMessage, AssistantMessageEvent, StopReason, UserContent};
use pi_ai::utils::event_stream::create_assistant_message_event_stream;
use serde_json::json;
use support::*;

fn message_listener(events: Arc<Mutex<Vec<AgentEvent>>>) -> AgentListener {
    Arc::new(move |event, _signal| {
        let events = Arc::clone(&events);
        Box::pin(async move {
            events.lock().unwrap().push(event);
            Ok(())
        })
    })
}

fn user_message(text: &str) -> AgentMessage {
    AgentMessage::User(user(text))
}

fn user_block_message(text: &str) -> AgentMessage {
    AgentMessage::User(user_block(text))
}

#[tokio::test]
async fn preserves_streamed_partial_on_abort_without_duplicate_start() {
    let partial = text_assistant("already delivered");
    let partial_hook = partial.clone();
    let agent = Agent::new(AgentOptions {
        stream_fn: Some(Arc::new(move |_, _, _| {
            let partial = partial_hook.clone();
            Box::pin(async move {
                let (sender, stream) = create_assistant_message_event_stream();
                sender.push(AssistantMessageEvent::Start { partial });
                std::mem::forget(sender);
                Ok(stream)
            })
        })),
        ..Default::default()
    })
    .unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));
    agent.subscribe(message_listener(Arc::clone(&events)));
    let abort_agent = agent.clone();
    agent.subscribe(Arc::new(move |event, _| {
        if matches!(
            event,
            AgentEvent::MessageStart {
                message: AgentMessage::Assistant(_)
            }
        ) {
            abort_agent.abort();
        }
        Box::pin(async { Ok(()) })
    }));
    agent.prompt("start").await.unwrap();
    let messages = agent.messages();
    let failure = messages.last().unwrap().as_assistant().unwrap();
    assert_eq!(failure.content, partial.content);
    assert_eq!(failure.stop_reason, StopReason::Aborted);
    let starts = events
        .lock()
        .unwrap()
        .iter()
        .filter(|event| {
            matches!(
                event,
                AgentEvent::MessageStart {
                    message: AgentMessage::Assistant(_)
                }
            )
        })
        .count();
    assert_eq!(starts, 1);
}

#[tokio::test]
async fn reset_clears_state_during_active_run_without_aborting_it() {
    let agent = Agent::new(AgentOptions {
        stream_fn: Some(scripted_stream_fn(vec![Script::message(text_assistant("ok"))])),
        ..Default::default()
    })
    .unwrap();
    let reset_agent = agent.clone();
    agent.subscribe(Arc::new(move |event, _| {
        if matches!(event, AgentEvent::ModelRequestStart) {
            reset_agent.follow_up(user_message("later"));
            reset_agent.reset();
            assert!(reset_agent.messages().is_empty());
            assert!(!reset_agent.is_streaming());
            assert!(!reset_agent.has_queued_messages());
            assert!(!reset_agent.signal().unwrap().aborted());
        }
        Box::pin(async { Ok(()) })
    }));
    agent.prompt("start").await.unwrap();
    assert_eq!(agent.messages().len(), 1);
}

#[tokio::test]
async fn defaults_to_compat_stream_without_registration() {
    let agent = Agent::new(AgentOptions::default()).expect("frozen constructor has a built-in default");
    agent.prompt("hello").await.unwrap();
    let messages = agent.messages();
    let failure = messages.last().unwrap().as_assistant().unwrap();
    assert_eq!(failure.stop_reason, StopReason::Error);
    assert!(
        failure
            .error_message
            .as_ref()
            .is_some_and(|message| message.contains("unknown"))
    );
}

#[tokio::test]
async fn subscriptions_use_live_set_iteration_and_deduplicate_listeners() {
    let agent = Agent::new(AgentOptions {
        stream_fn: Some(scripted_stream_fn(vec![Script::message(text_assistant("ok"))])),
        ..Default::default()
    })
    .unwrap();
    let removed = Arc::new(Mutex::new(None));
    let observed = Arc::new(Mutex::new(Vec::new()));
    let control = agent.clone();
    let removed_hook = removed.clone();
    let observed_hook = observed.clone();
    let first: AgentListener = Arc::new(move |event, _| {
        if matches!(event, AgentEvent::AgentStart) {
            control.unsubscribe(removed_hook.lock().unwrap().unwrap());
            let observed = observed_hook.clone();
            control.subscribe(Arc::new(move |event, _| {
                if matches!(event, AgentEvent::AgentStart) {
                    observed.lock().unwrap().push("added");
                }
                Box::pin(async { Ok(()) })
            }));
        }
        Box::pin(async { Ok(()) })
    });
    let id = agent.subscribe(first.clone());
    assert_eq!(agent.subscribe(first), id);
    let observed_hook = observed.clone();
    *removed.lock().unwrap() = Some(agent.subscribe(Arc::new(move |event, _| {
        if matches!(event, AgentEvent::AgentStart) {
            observed_hook.lock().unwrap().push("removed");
        }
        Box::pin(async { Ok(()) })
    })));
    agent.prompt("hello").await.unwrap();
    assert_eq!(*observed.lock().unwrap(), vec!["added"]);
}

#[tokio::test]
async fn listener_errors_do_not_interrupt_other_listeners_or_the_run() {
    let agent = Agent::new(AgentOptions {
        stream_fn: Some(scripted_stream_fn(vec![Script::message(text_assistant("ok"))])),
        ..Default::default()
    })
    .unwrap();
    agent.subscribe(Arc::new(|_, _| {
        Box::pin(async { Err(pi_agent_core::AgentError::new("listener rejected")) })
    }));
    let events = Arc::new(Mutex::new(Vec::new()));
    agent.subscribe(message_listener(events.clone()));
    agent.prompt("hello").await.unwrap();
    assert!(matches!(
        events.lock().unwrap().last(),
        Some(AgentEvent::AgentEnd { .. })
    ));
    assert_eq!(agent.messages().len(), 2);
    assert!(agent.error_message().is_none());
}

#[tokio::test]
async fn prepare_next_turn_reads_replaced_hook_after_turn_listeners() {
    let prompts = Arc::new(Mutex::new(Vec::new()));
    let prompts_hook = prompts.clone();
    let responses = scripted_stream_fn(vec![
        Script::message(tool_use(vec![AssistantContent::ToolCall(tool_call(
            "call",
            "lookup",
            json!({}),
        ))])),
        Script::message(text_assistant("done")),
    ]);
    let agent = Agent::new(AgentOptions {
        initial_state: Some(pi_agent_core::AgentInitialState {
            tools: Some(vec![Arc::new(TestTool::no_args("lookup", || text_result("found")))]),
            ..Default::default()
        }),
        prepare_next_turn_with_context: Some(Arc::new(|_, _| Box::pin(async { Ok(None) }))),
        stream_fn: Some(Arc::new(move |model, context, options| {
            prompts_hook.lock().unwrap().push(context.system_prompt.clone());
            responses(model, context, options)
        })),
        ..Default::default()
    })
    .unwrap();
    let control = agent.clone();
    agent.subscribe(Arc::new(move |event, _| {
        if matches!(event, AgentEvent::TurnEnd { .. }) {
            control.set_prepare_next_turn_with_context(Some(Arc::new(|turn, _| {
                Box::pin(async move {
                    let mut context = turn.context;
                    context.system_prompt = "replaced hook".into();
                    Ok(Some(pi_agent_core::AgentLoopTurnUpdate {
                        context: Some(context),
                        model: None,
                        thinking_level: None,
                    }))
                })
            })));
        }
        Box::pin(async { Ok(()) })
    }));
    agent.prompt("hello").await.unwrap();
    assert_eq!(
        prompts.lock().unwrap()[1].as_ref(),
        Some(&JsString::from("replaced hook"))
    );
}

#[tokio::test]
async fn forwards_frozen_agent_callbacks_and_mutable_provider_configuration() {
    let observed = Arc::new(Mutex::new(Vec::new()));
    let observed_hook = observed.clone();
    let agent = Agent::new(AgentOptions {
        stream_fn: Some(scripted_stream_fn(Vec::new())),
        on_payload: Some(Arc::new(|payload, _| Box::pin(async move { Ok(Some(payload)) }))),
        on_response: Some(Arc::new(|_, _| Box::pin(async { Ok(()) }))),
        on_retry: Some(Arc::new(|_, _| Ok(()))),
        ..Default::default()
    })
    .unwrap();
    agent.set_transform_context(Some(Arc::new(|_, _| {
        Box::pin(async { Ok(vec![user_message("transformed")]) })
    })));
    agent.set_convert_to_llm(Arc::new(|messages| {
        Box::pin(async move {
            assert_eq!(messages[0], user_message("transformed"));
            Ok(vec![pi_ai::types::Message::User(user("converted"))])
        })
    }));
    agent.set_get_api_key(Some(Arc::new(|_| Box::pin(async { Some("new-key".into()) }))));
    agent.set_stream_fn(Arc::new(move |_, context, options| {
        observed_hook.lock().unwrap().push((
            context.messages,
            options.stream.request.api_key,
            options.stream.request.on_payload.is_some(),
            options.stream.request.on_response.is_some(),
            options.stream.on_retry.is_some(),
            options.thinking_budgets.and_then(|budgets| budgets.low),
            options.stream.request.max_retry_delay_ms,
        ));
        Box::pin(async {
            let (sender, stream) = create_assistant_message_event_stream();
            sender.push(AssistantMessageEvent::Done {
                reason: pi_ai::types::DoneReason::Stop,
                message: text_assistant("new stream"),
            });
            Ok(stream)
        })
    }));
    agent.prompt("hello").await.unwrap();
    agent.set_on_payload(None);
    agent.set_on_response(None);
    agent.set_on_retry(None);
    agent.set_get_api_key(None);
    agent.set_thinking_budgets(Some(pi_ai::types::ThinkingBudgets {
        low: Some(42.0),
        ..Default::default()
    }));
    agent.set_max_retry_delay_ms(Some(123.0));
    let prepared = Arc::new(AtomicBool::new(false));
    let prepared_hook = prepared.clone();
    agent.set_prepare_next_turn(Some(Arc::new(move |signal| {
        prepared_hook.store(signal.is_some(), Ordering::SeqCst);
        Box::pin(async { Ok(None) })
    })));
    agent.prompt("again").await.unwrap();
    assert!(prepared.load(Ordering::SeqCst));
    let observed = observed.lock().unwrap();
    assert_eq!(
        observed[0],
        (
            vec![pi_ai::types::Message::User(user("converted"))],
            Some("new-key".into()),
            true,
            true,
            true,
            None,
            None
        )
    );
    assert_eq!(
        observed[1],
        (
            vec![pi_ai::types::Message::User(user("converted"))],
            None,
            false,
            false,
            false,
            Some(42.0),
            Some(123.0)
        )
    );
}

#[tokio::test]
async fn empty_assistant_error_text_does_not_set_state_error() {
    let mut message = text_assistant("ok");
    message.error_message = Some(JsString::default());
    let agent = Agent::new(AgentOptions {
        stream_fn: Some(scripted_stream_fn(vec![Script::message(message)])),
        ..Default::default()
    })
    .unwrap();
    agent.prompt("hello").await.unwrap();
    assert_eq!(agent.error_message(), None);
}

#[tokio::test]
async fn creates_agent_with_default_state() {
    let agent = Agent::new(AgentOptions {
        stream_fn: Some(scripted_stream_fn(Vec::new())),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(agent.system_prompt(), "");
    assert_eq!(agent.model().id, "unknown");
    assert_eq!(agent.thinking_level(), pi_agent_core::harness::ThinkingLevel::Off);
    assert!(agent.tools().is_empty());
    assert!(agent.messages().is_empty());
    assert!(!agent.is_streaming());
    assert!(!agent.is_model_request_active());
    assert!(agent.streaming_message().is_none());
    assert!(agent.pending_tool_calls().is_empty());
    assert!(agent.error_message().is_none());
}

#[tokio::test]
async fn creates_agent_with_custom_initial_state() {
    let agent = Agent::new(AgentOptions {
        stream_fn: Some(scripted_stream_fn(Vec::new())),
        initial_state: Some(pi_agent_core::agent::AgentInitialState {
            system_prompt: Some(JsString::from("You are a helpful assistant.")),
            model: Some(model()),
            thinking_level: Some(pi_agent_core::harness::ThinkingLevel::Low),
            ..Default::default()
        }),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(agent.system_prompt(), "You are a helpful assistant.");
    assert_eq!(agent.model(), model());
    assert_eq!(agent.thinking_level(), pi_agent_core::harness::ThinkingLevel::Low);
}

#[tokio::test]
async fn subscribes_to_events_and_unsubscribes() {
    let agent = Agent::new(AgentOptions {
        stream_fn: Some(scripted_stream_fn(Vec::new())),
        ..Default::default()
    })
    .unwrap();
    let event_count = Arc::new(AtomicUsize::new(0));
    let event_count_hook = Arc::clone(&event_count);
    let subscription = agent.subscribe(Arc::new(move |_event, _signal| {
        let event_count_hook = Arc::clone(&event_count_hook);
        Box::pin(async move {
            event_count_hook.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
    }));
    assert_eq!(event_count.load(Ordering::SeqCst), 0);
    agent.set_system_prompt("Test prompt");
    assert_eq!(event_count.load(Ordering::SeqCst), 0);
    assert_eq!(agent.system_prompt(), "Test prompt");
    agent.unsubscribe(subscription);
    agent.set_system_prompt("Another prompt");
    assert_eq!(event_count.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn emits_full_lifecycle_events_for_run_failures() {
    let agent = Agent::new(AgentOptions {
        stream_fn: Some(scripted_stream_fn(Vec::new())),
        convert_to_llm: Some(Arc::new(|_messages| {
            Box::pin(async { Err(pi_agent_core::agent::AgentError::new("provider exploded")) })
        })),
        ..Default::default()
    })
    .unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));
    agent.subscribe(message_listener(Arc::clone(&events)));
    agent.prompt("hello").await.unwrap();
    assert_eq!(
        event_types(&events.lock().unwrap()),
        vec![
            "agent_start",
            "turn_start",
            "message_start",
            "message_end",
            "model_request_start",
            "model_request_end",
            "message_start",
            "message_end",
            "turn_end",
            "agent_end",
        ]
    );
    let messages = agent.messages();
    let last = messages.last().unwrap();
    let assistant = last.as_assistant().expect("assistant failure message");
    assert_eq!(assistant.stop_reason, StopReason::Error);
    assert_eq!(
        assistant.error_message.as_ref(),
        Some(&JsString::from("provider exploded"))
    );
    assert_eq!(
        agent.error_message().as_ref(),
        Some(&JsString::from("provider exploded"))
    );
}

#[tokio::test]
async fn awaits_async_subscribers_before_prompt_resolves() {
    let agent = Agent::new(AgentOptions {
        stream_fn: Some(scripted_stream_fn(vec![Script::message(text_assistant("ok"))])),
        ..Default::default()
    })
    .unwrap();
    let barrier = Arc::new(tokio::sync::Notify::new());
    let listener_finished = Arc::new(AtomicBool::new(false));
    let barrier_hook = Arc::clone(&barrier);
    let listener_finished_hook = Arc::clone(&listener_finished);
    agent.subscribe(Arc::new(move |event, _signal| {
        let barrier_hook = Arc::clone(&barrier_hook);
        let listener_finished_hook = Arc::clone(&listener_finished_hook);
        let is_agent_end = matches!(event, AgentEvent::AgentEnd { .. });
        Box::pin(async move {
            if is_agent_end {
                barrier_hook.notified().await;
                listener_finished_hook.store(true, Ordering::SeqCst);
            }
            Ok(())
        })
    }));
    let prompt_resolved = Arc::new(AtomicBool::new(false));
    let prompt_resolved_hook = Arc::clone(&prompt_resolved);
    let agent_clone = agent.clone();
    let prompt = tokio::spawn(async move {
        agent_clone.prompt("hello").await.unwrap();
        prompt_resolved_hook.store(true, Ordering::SeqCst);
    });
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    assert!(!prompt_resolved.load(Ordering::SeqCst));
    assert!(!listener_finished.load(Ordering::SeqCst));
    assert!(agent.is_streaming());
    barrier.notify_waiters();
    prompt.await.unwrap();
    assert!(listener_finished.load(Ordering::SeqCst));
    assert!(prompt_resolved.load(Ordering::SeqCst));
    assert!(!agent.is_streaming());
}

#[tokio::test]
async fn wait_for_idle_waits_for_async_subscribers() {
    let agent = Agent::new(AgentOptions {
        stream_fn: Some(scripted_stream_fn(vec![Script::message(text_assistant("ok"))])),
        ..Default::default()
    })
    .unwrap();
    let barrier = Arc::new(tokio::sync::Notify::new());
    let barrier_hook = Arc::clone(&barrier);
    agent.subscribe(Arc::new(move |event, _signal| {
        let barrier_hook = Arc::clone(&barrier_hook);
        let should_wait = matches!(
            &event,
            AgentEvent::MessageEnd { message } if message.as_assistant().is_some()
        );
        Box::pin(async move {
            if should_wait {
                barrier_hook.notified().await;
            }
            Ok(())
        })
    }));
    let agent_clone = agent.clone();
    let prompt = tokio::spawn(async move { agent_clone.prompt("hello").await.unwrap() });
    let idle_resolved = Arc::new(AtomicBool::new(false));
    let idle_resolved_hook = Arc::clone(&idle_resolved);
    let idle_agent = agent.clone();
    let idle = tokio::spawn(async move {
        idle_agent.wait_for_idle().await;
        idle_resolved_hook.store(true, Ordering::SeqCst);
    });
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    assert!(!idle_resolved.load(Ordering::SeqCst));
    assert!(agent.is_streaming());
    barrier.notify_waiters();
    prompt.await.unwrap();
    idle.await.unwrap();
    assert!(idle_resolved.load(Ordering::SeqCst));
    assert!(!agent.is_streaming());
}

#[tokio::test]
async fn passes_active_abort_signal_to_subscribers() {
    let received_signal = Arc::new(Mutex::new(None::<pi_js_compat::abort::AbortSignal>));
    let received_signal_hook = Arc::clone(&received_signal);
    let stream_fn: pi_agent_core::agent::StreamFn = Arc::new(|_model, _context, options| {
        let signal = options.stream.request.signal.clone();
        Box::pin(async move {
            let (sender, stream) = create_assistant_message_event_stream();
            sender.push(AssistantMessageEvent::Start {
                partial: AssistantMessage::empty("openai-responses", "openai", "mock", 1000.0),
            });
            tokio::spawn(async move {
                loop {
                    if signal.as_ref().is_some_and(|signal| signal.aborted()) {
                        sender.push(AssistantMessageEvent::Error {
                            reason: pi_ai::types::ErrorReason::Aborted,
                            error: text_assistant("Aborted"),
                        });
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                }
            });
            Ok(stream)
        })
    });
    let agent = Agent::new(AgentOptions {
        stream_fn: Some(stream_fn),
        ..Default::default()
    })
    .unwrap();
    agent.subscribe(Arc::new(move |event, signal| {
        let received_signal_hook = Arc::clone(&received_signal_hook);
        let is_start = matches!(event, AgentEvent::AgentStart);
        Box::pin(async move {
            if is_start {
                *received_signal_hook.lock().unwrap() = Some(signal);
            }
            Ok(())
        })
    }));
    let agent_clone = agent.clone();
    let prompt = tokio::spawn(async move { agent_clone.prompt("hello").await.unwrap() });
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    let signal = received_signal.lock().unwrap().clone().expect("signal");
    assert!(!signal.aborted());
    agent.abort();
    prompt.await.unwrap();
    assert!(signal.aborted());
}

#[tokio::test]
async fn aborts_active_model_request_when_steering_is_queued() {
    let request_signal = Arc::new(Mutex::new(None::<pi_js_compat::abort::AbortSignal>));
    let request_signal_hook = Arc::clone(&request_signal);
    let stream_fn: pi_agent_core::agent::StreamFn = Arc::new(move |_model, _context, options| {
        let signal = options.stream.request.signal.clone();
        *request_signal_hook.lock().unwrap() = signal.clone();
        Box::pin(async move {
            let (sender, stream) = create_assistant_message_event_stream();
            sender.push(AssistantMessageEvent::Start {
                partial: AssistantMessage::empty("openai-responses", "openai", "mock", 1000.0),
            });
            tokio::spawn(async move {
                loop {
                    if signal.as_ref().is_some_and(|signal| signal.aborted()) {
                        sender.push(AssistantMessageEvent::Error {
                            reason: pi_ai::types::ErrorReason::Aborted,
                            error: text_assistant("Aborted"),
                        });
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                }
            });
            Ok(stream)
        })
    });
    let agent = Agent::new(AgentOptions {
        stream_fn: Some(stream_fn),
        ..Default::default()
    })
    .unwrap();
    let agent_clone = agent.clone();
    let prompt = tokio::spawn(async move { agent_clone.prompt("hello").await.unwrap() });
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    agent.steer(user_block_message("change course"));
    let signal = request_signal.lock().unwrap().clone().expect("request signal");
    assert!(signal.aborted());
    prompt.await.unwrap();
}

#[tokio::test]
async fn ignores_tool_updates_after_execution_settles() {
    let delayed_update = Arc::new(Mutex::new(None::<pi_agent_core::agent::AgentToolUpdateCallback>));
    let delayed_update_hook = Arc::clone(&delayed_update);
    let mut tool = TestTool::new("settled_tool", |_id, _params| text_result("done"));
    tool.parameters = json!({"type": "object", "properties": {}});
    tool.execute = Arc::new(move |_id, _params, _signal, on_update| {
        *delayed_update_hook.lock().unwrap() = Some(on_update);
        Box::pin(async { Ok(text_result("done")) })
    });
    let agent = Agent::new(AgentOptions {
        initial_state: Some(pi_agent_core::agent::AgentInitialState {
            tools: Some(vec![Arc::new(tool)]),
            ..Default::default()
        }),
        stream_fn: Some(scripted_stream_fn(vec![
            Script::message(tool_use(vec![AssistantContent::ToolCall(tool_call(
                "call-1",
                "settled_tool",
                json!({}),
            ))])),
            Script::message(text_assistant("done")),
        ])),
        ..Default::default()
    })
    .unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));
    agent.subscribe(message_listener(Arc::clone(&events)));
    agent.prompt("run tool").await.unwrap();
    let before = events.lock().unwrap().len();
    let update = delayed_update.lock().unwrap().clone().expect("update callback");
    update(text_result("late")).expect("tool update");
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    assert_eq!(events.lock().unwrap().len(), before);
    assert_eq!(
        events
            .lock()
            .unwrap()
            .iter()
            .filter(|event| matches!(event, AgentEvent::ToolExecutionUpdate { .. }))
            .count(),
        0
    );
}

#[tokio::test]
async fn ignores_settled_parallel_tool_update_while_another_tool_runs() {
    let settled_tool_update = Arc::new(Mutex::new(None::<pi_agent_core::agent::AgentToolUpdateCallback>));
    let settled_hook = Arc::clone(&settled_tool_update);
    let release_slow = Arc::new(tokio::sync::Notify::new());
    let slow_started = Arc::new(AtomicBool::new(false));
    let slow_started_hook = Arc::clone(&slow_started);
    let release_slow_hook = Arc::clone(&release_slow);

    let mut settled_tool = TestTool::new("settled_tool", |_id, _params| text_result("done"));
    settled_tool.parameters = json!({"type": "object", "properties": {}});
    settled_tool.execute = Arc::new(move |_id, _params, _signal, on_update| {
        *settled_hook.lock().unwrap() = Some(on_update);
        Box::pin(async {
            Ok(AgentToolResult {
                terminate: Some(true),
                ..text_result("done")
            })
        })
    });
    let mut slow_tool = TestTool::new("slow_tool", |_id, _params| text_result("done"));
    slow_tool.parameters = json!({"type": "object", "properties": {}});
    slow_tool.execute = Arc::new(move |_id, _params, _signal, _on_update| {
        let slow_started_hook = Arc::clone(&slow_started_hook);
        let release_slow_hook = Arc::clone(&release_slow_hook);
        Box::pin(async move {
            slow_started_hook.store(true, Ordering::SeqCst);
            release_slow_hook.notified().await;
            Ok(AgentToolResult {
                terminate: Some(true),
                ..text_result("done")
            })
        })
    });
    let agent = Agent::new(AgentOptions {
        initial_state: Some(pi_agent_core::agent::AgentInitialState {
            tools: Some(vec![Arc::new(settled_tool), Arc::new(slow_tool)]),
            ..Default::default()
        }),
        stream_fn: Some(scripted_stream_fn(vec![Script::message(tool_use(vec![
            AssistantContent::ToolCall(tool_call("call-1", "settled_tool", json!({}))),
            AssistantContent::ToolCall(tool_call("call-2", "slow_tool", json!({}))),
        ]))])),
        ..Default::default()
    })
    .unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));
    let events_hook = Arc::clone(&events);
    agent.subscribe(Arc::new(move |event, _signal| {
        let events_hook = Arc::clone(&events_hook);
        Box::pin(async move {
            events_hook.lock().unwrap().push(event);
            Ok(())
        })
    }));
    let agent_clone = agent.clone();
    let prompt = tokio::spawn(async move { agent_clone.prompt("run tools").await.unwrap() });
    while !slow_started.load(Ordering::SeqCst) {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let update = settled_tool_update.lock().unwrap().clone().expect("update callback");
    let before = events.lock().unwrap().len();
    update(text_result("late")).expect("tool update");
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    assert_eq!(events.lock().unwrap().len(), before);
    release_slow.notify_waiters();
    prompt.await.unwrap();
    assert_eq!(
        events
            .lock()
            .unwrap()
            .iter()
            .filter(|event| matches!(event, AgentEvent::ToolExecutionUpdate { .. }))
            .count(),
        0
    );
}

#[tokio::test]
async fn updates_state_with_mutators() {
    let agent = Agent::new(AgentOptions {
        stream_fn: Some(scripted_stream_fn(Vec::new())),
        ..Default::default()
    })
    .unwrap();
    agent.set_system_prompt("Custom prompt");
    assert_eq!(agent.system_prompt(), "Custom prompt");
    agent.set_model(model());
    assert_eq!(agent.model(), model());
    agent.set_thinking_level(pi_agent_core::harness::ThinkingLevel::High);
    assert_eq!(agent.thinking_level(), pi_agent_core::harness::ThinkingLevel::High);
    agent.set_thinking_level(pi_agent_core::harness::ThinkingLevel::Max);
    assert_eq!(agent.thinking_level(), pi_agent_core::harness::ThinkingLevel::Max);
    let tools: Vec<Arc<dyn AgentTool>> = vec![Arc::new(TestTool::no_args("test", || text_result("ok")))];
    agent.set_tools(tools.clone());
    assert_eq!(agent.tools().len(), tools.len());
    let messages = vec![user_message("Hello")];
    agent.set_messages(messages.clone());
    assert_eq!(agent.messages(), messages);
    agent.set_messages(Vec::new());
    assert!(agent.messages().is_empty());
}

#[tokio::test]
async fn supports_steering_and_follow_up_queues() {
    let agent = Agent::new(AgentOptions {
        stream_fn: Some(scripted_stream_fn(Vec::new())),
        ..Default::default()
    })
    .unwrap();
    agent.steer(user_message("Steering message"));
    agent.follow_up(user_message("Follow-up message"));
    assert!(!agent.messages().iter().any(|message| matches!(
        message,
        AgentMessage::User(user) if matches!(&user.content, UserContent::Text(text) if text == "Steering message")
    )));
    assert!(agent.has_queued_messages());
    agent.clear_all_queues();
    assert!(!agent.has_queued_messages());
}

#[tokio::test]
async fn abort_is_a_noop_without_an_active_run() {
    let agent = Agent::new(AgentOptions {
        stream_fn: Some(scripted_stream_fn(Vec::new())),
        ..Default::default()
    })
    .unwrap();
    agent.abort();
}

#[tokio::test]
async fn reset_during_pending_response_keeps_run_reserved_and_completes() {
    let release_response = Arc::new(tokio::sync::Notify::new());
    let release_hook = Arc::clone(&release_response);
    let stream_fn: pi_agent_core::agent::StreamFn = Arc::new(move |_model, _context, _options| {
        let release_hook = Arc::clone(&release_hook);
        Box::pin(async move {
            let (sender, stream) = create_assistant_message_event_stream();
            sender.push(AssistantMessageEvent::Start {
                partial: AssistantMessage::empty("openai-responses", "openai", "mock", 1000.0),
            });
            tokio::spawn(async move {
                release_hook.notified().await;
                sender.push(AssistantMessageEvent::Done {
                    reason: pi_ai::types::DoneReason::Stop,
                    message: text_assistant("Done"),
                });
            });
            Ok(stream)
        })
    });
    let agent = Agent::new(AgentOptions {
        stream_fn: Some(stream_fn),
        ..Default::default()
    })
    .unwrap();
    let agent_clone = agent.clone();
    let prompt = tokio::spawn(async move { agent_clone.prompt("Hello").await.unwrap() });
    while !agent.is_streaming() {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert_eq!(
        agent
            .messages()
            .iter()
            .map(|message| message.role())
            .collect::<Vec<_>>(),
        vec!["user"]
    );
    agent.reset();
    assert!(!agent.is_streaming());
    assert!(agent.signal().is_some());
    assert!(agent.messages().is_empty());
    release_response.notify_waiters();
    prompt.await.unwrap();
    assert!(!agent.is_streaming());
    assert_eq!(
        agent
            .messages()
            .iter()
            .map(|message| message.role())
            .collect::<Vec<_>>(),
        vec!["assistant"]
    );
}

async fn aborting_agent() -> Agent {
    let stream_fn: pi_agent_core::agent::StreamFn = Arc::new(|_model, _context, options| {
        let signal = options.stream.request.signal.clone();
        Box::pin(async move {
            let (sender, stream) = create_assistant_message_event_stream();
            sender.push(AssistantMessageEvent::Start {
                partial: AssistantMessage::empty("openai-responses", "openai", "mock", 1000.0),
            });
            tokio::spawn(async move {
                loop {
                    if signal.as_ref().is_some_and(|signal| signal.aborted()) {
                        sender.push(AssistantMessageEvent::Error {
                            reason: pi_ai::types::ErrorReason::Aborted,
                            error: text_assistant("Aborted"),
                        });
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                }
            });
            Ok(stream)
        })
    });
    Agent::new(AgentOptions {
        stream_fn: Some(stream_fn),
        ..Default::default()
    })
    .unwrap()
}

#[tokio::test]
async fn rejects_prompt_while_streaming() {
    let agent = aborting_agent().await;
    let agent_clone = agent.clone();
    let first_prompt = tokio::spawn(async move { agent_clone.prompt("First message").await });
    while !agent.is_streaming() {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let error = agent.prompt("Second message").await.unwrap_err();
    assert_eq!(
        error.to_string(),
        "Agent is already processing a prompt. Use steer() or followUp() to queue messages, or wait for completion."
    );
    agent.abort();
    first_prompt.await.unwrap().unwrap();
}

#[tokio::test]
async fn rejects_continue_while_streaming() {
    let agent = aborting_agent().await;
    let agent_clone = agent.clone();
    let first_prompt = tokio::spawn(async move { agent_clone.prompt("First message").await });
    while !agent.is_streaming() {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let error = agent.continue_run(ContinueOptions::default()).await.unwrap_err();
    assert_eq!(
        error.to_string(),
        "Agent is already processing. Wait for completion before continuing."
    );
    agent.abort();
    first_prompt.await.unwrap().unwrap();
}

#[tokio::test]
async fn continue_processes_queued_follow_up_after_assistant_turn() {
    let agent = Agent::new(AgentOptions {
        stream_fn: Some(scripted_stream_fn(vec![Script::message(text_assistant("Processed"))])),
        ..Default::default()
    })
    .unwrap();
    agent.set_messages(vec![
        user_block_message("Initial"),
        AgentMessage::Assistant(Box::new(text_assistant("Initial response"))),
    ]);
    agent.follow_up(user_block_message("Queued follow-up"));
    agent.continue_run(ContinueOptions::default()).await.unwrap();
    let has_follow_up = agent.messages().iter().any(|message| match message {
        AgentMessage::User(user) => match &user.content {
            UserContent::Text(text) => text == "Queued follow-up",
            UserContent::Blocks(blocks) => blocks.iter().any(|block| {
                matches!(
                    block,
                    pi_ai::types::TextOrImageContent::Text(text) if text.text == "Queued follow-up"
                )
            }),
        },
        _ => false,
    });
    assert!(has_follow_up);
    assert_eq!(agent.messages().last().unwrap().role(), "assistant");
}

#[tokio::test]
async fn continue_keeps_one_at_a_time_steering_from_assistant_tail() {
    let response_count = Arc::new(AtomicUsize::new(0));
    let response_count_hook = Arc::clone(&response_count);
    let stream_fn: pi_agent_core::agent::StreamFn = Arc::new(move |_model, _context, _options| {
        let count = response_count_hook.fetch_add(1, Ordering::SeqCst) + 1;
        Box::pin(async move {
            let (sender, stream) = create_assistant_message_event_stream();
            sender.push(AssistantMessageEvent::Done {
                reason: pi_ai::types::DoneReason::Stop,
                message: text_assistant(&format!("Processed {count}")),
            });
            Ok(stream)
        })
    });
    let agent = Agent::new(AgentOptions {
        stream_fn: Some(stream_fn),
        ..Default::default()
    })
    .unwrap();
    agent.set_messages(vec![
        user_block_message("Initial"),
        AgentMessage::Assistant(Box::new(text_assistant("Initial response"))),
    ]);
    agent.steer(user_block_message("Steering 1"));
    agent.steer(user_block_message("Steering 2"));
    agent.continue_run(ContinueOptions::default()).await.unwrap();
    let messages = agent.messages();
    let recent: Vec<JsString> = messages
        .iter()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|message| message.role())
        .collect();
    assert_eq!(recent, vec!["user", "assistant", "user", "assistant"]);
    assert_eq!(response_count.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn keeps_legacy_prepare_next_turn_signal_callback_behavior() {
    let tool = TestTool::no_args("noop", || text_result("ok"));
    let request_count = Arc::new(AtomicUsize::new(0));
    let request_count_hook = Arc::clone(&request_count);
    let saw_signal = Arc::new(AtomicBool::new(false));
    let saw_signal_hook = Arc::clone(&saw_signal);
    let agent = Agent::new(AgentOptions {
        initial_state: Some(pi_agent_core::agent::AgentInitialState {
            tools: Some(vec![Arc::new(tool)]),
            ..Default::default()
        }),
        prepare_next_turn: Some(Arc::new(move |signal| {
            saw_signal_hook.store(signal.is_some(), Ordering::SeqCst);
            Box::pin(async { Ok(None) })
        })),
        stream_fn: Some({
            let request_count_hook = Arc::clone(&request_count_hook);
            Arc::new(move |_model, _context, _options| {
                let call = request_count_hook.fetch_add(1, Ordering::SeqCst);
                Box::pin(async move {
                    let (sender, stream) = create_assistant_message_event_stream();
                    let message = if call == 0 {
                        tool_use(vec![AssistantContent::ToolCall(tool_call("tool-1", "noop", json!({})))])
                    } else {
                        text_assistant("done")
                    };
                    let reason = if call == 0 {
                        pi_ai::types::DoneReason::ToolUse
                    } else {
                        pi_ai::types::DoneReason::Stop
                    };
                    sender.push(AssistantMessageEvent::Done { reason, message });
                    Ok(stream)
                })
            })
        }),
        ..Default::default()
    })
    .unwrap();
    agent.prompt("start").await.unwrap();
    assert_eq!(request_count.load(Ordering::SeqCst), 2);
    assert!(saw_signal.load(Ordering::SeqCst));
}

#[tokio::test]
async fn pending_tool_calls_preserve_insertion_order() {
    let barrier = Arc::new(tokio::sync::Barrier::new(2));
    let blocking_tool = |name: &str| {
        let barrier = Arc::clone(&barrier);
        TestTool {
            name: name.to_owned(),
            label: name.to_owned(),
            description: format!("{name} tool"),
            parameters: json!({"type": "object", "properties": {}}),
            execution_mode: None,
            prepare_arguments: None,
            prepare_execution: None,
            execute: Arc::new(move |_id, _params, _signal, _on_update| {
                let barrier = Arc::clone(&barrier);
                Box::pin(async move {
                    barrier.wait().await;
                    Ok(text_result("ok"))
                })
            }),
        }
    };
    let agent = Agent::new(AgentOptions {
        initial_state: Some(pi_agent_core::agent::AgentInitialState {
            tools: Some(vec![Arc::new(blocking_tool("noop"))]),
            ..Default::default()
        }),
        stream_fn: Some(scripted_stream_fn(vec![
            Script::message(tool_use(vec![
                AssistantContent::ToolCall(tool_call("call-b", "noop", json!({}))),
                AssistantContent::ToolCall(tool_call("call-a", "noop", json!({}))),
            ])),
            Script::message(text_assistant("done")),
        ])),
        ..Default::default()
    })
    .unwrap();
    let observed = Arc::new(Mutex::new(Vec::<Vec<String>>::new()));
    let observed_hook = Arc::clone(&observed);
    let agent_hook = agent.clone();
    agent.subscribe(Arc::new(move |event, _signal| {
        if matches!(event, AgentEvent::ToolExecutionStart { .. }) {
            observed_hook
                .lock()
                .unwrap()
                .push(agent_hook.pending_tool_calls().iter().cloned().collect());
        }
        Box::pin(async { Ok(()) })
    }));
    agent.prompt("start").await.unwrap();
    assert_eq!(
        *observed.lock().unwrap(),
        vec![
            vec!["call-b".to_owned()],
            vec!["call-b".to_owned(), "call-a".to_owned()]
        ]
    );
}

#[tokio::test]
async fn forwards_should_stop_after_turn_through_agent_options() {
    let tool = TestTool::no_args("noop", || text_result("tool complete"));
    let request_count = Arc::new(AtomicUsize::new(0));
    let request_count_hook = Arc::clone(&request_count);
    let callback_roles = Arc::new(Mutex::new(Vec::<JsString>::new()));
    let callback_roles_hook = Arc::clone(&callback_roles);
    let agent = Agent::new(AgentOptions {
        initial_state: Some(pi_agent_core::agent::AgentInitialState {
            tools: Some(vec![Arc::new(tool)]),
            ..Default::default()
        }),
        should_stop_after_turn: Some(Arc::new(move |context| {
            *callback_roles_hook.lock().unwrap() = context
                .context
                .messages
                .iter()
                .map(|message| message.role().to_owned())
                .collect();
            Box::pin(async { Ok(true) })
        })),
        stream_fn: Some({
            let request_count_hook = Arc::clone(&request_count_hook);
            Arc::new(move |_model, _context, _options| {
                let call = request_count_hook.fetch_add(1, Ordering::SeqCst);
                Box::pin(async move {
                    let (sender, stream) = create_assistant_message_event_stream();
                    let message = if call == 0 {
                        tool_use(vec![AssistantContent::ToolCall(tool_call("tool-1", "noop", json!({})))])
                    } else {
                        text_assistant("should not run")
                    };
                    let reason = if call == 0 {
                        pi_ai::types::DoneReason::ToolUse
                    } else {
                        pi_ai::types::DoneReason::Stop
                    };
                    sender.push(AssistantMessageEvent::Done { reason, message });
                    Ok(stream)
                })
            })
        }),
        ..Default::default()
    })
    .unwrap();
    agent.prompt("start").await.unwrap();
    assert_eq!(request_count.load(Ordering::SeqCst), 1);
    assert_eq!(
        *callback_roles.lock().unwrap(),
        vec!["user".to_owned(), "assistant".to_owned(), "toolResult".to_owned()]
    );
}

#[tokio::test]
async fn forwards_session_id_to_stream_function_options() {
    let received = Arc::new(Mutex::new(Vec::<Option<JsString>>::new()));
    let received_hook = Arc::clone(&received);
    let agent = Agent::new(AgentOptions {
        session_id: Some(JsString::from("session-abc")),
        stream_fn: Some(Arc::new(move |_model, _context, options: SimpleStreamOptions| {
            received_hook.lock().unwrap().push(options.stream.session_id.clone());
            Box::pin(async move {
                let (sender, stream) = create_assistant_message_event_stream();
                sender.push(AssistantMessageEvent::Done {
                    reason: pi_ai::types::DoneReason::Stop,
                    message: text_assistant("ok"),
                });
                Ok(stream)
            })
        })),
        ..Default::default()
    })
    .unwrap();
    agent.prompt("hello").await.unwrap();
    assert_eq!(
        received.lock().unwrap().last().cloned().flatten().as_ref(),
        Some(&JsString::from("session-abc"))
    );
    agent.set_session_id(Some(JsString::from("session-def")));
    assert_eq!(agent.session_id().as_ref(), Some(&JsString::from("session-def")));
    agent.prompt("hello again").await.unwrap();
    assert_eq!(
        received.lock().unwrap().last().cloned().flatten().as_ref(),
        Some(&JsString::from("session-def"))
    );
}

#[tokio::test]
async fn prompt_accepts_agent_messages_and_batches() {
    let agent = Agent::new(AgentOptions {
        stream_fn: Some(scripted_stream_fn(vec![
            Script::message(text_assistant("one")),
            Script::message(text_assistant("two")),
        ])),
        ..Default::default()
    })
    .unwrap();
    agent
        .prompt(PromptInput::Message(Box::new(user_message("single"))))
        .await
        .unwrap();
    agent
        .prompt(PromptInput::Messages(vec![user_message("a"), user_message("b")]))
        .await
        .unwrap();
    let messages = agent.messages();
    let roles: Vec<JsString> = messages.iter().map(|message| message.role()).collect();
    assert_eq!(roles, vec!["user", "assistant", "user", "user", "assistant"]);
}
