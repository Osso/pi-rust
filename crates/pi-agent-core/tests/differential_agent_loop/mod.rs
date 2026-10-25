use crate::shared_agent_support as support;
use crate::shared_oracle as oracle;

use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use pi_agent_core::agent::{
    AfterToolCallResult, Agent, AgentContext, AgentError, AgentEvent, AgentEventSink, AgentLoopConfig, AgentMessage,
    AgentOptions, AgentTool, AgentToolResult, ContinueOptions, StreamFn,
};
use pi_agent_core::harness::BeforeToolCallResult;
use pi_agent_core::harness::ToolExecutionMode;
use pi_ai::types::{
    AssistantContent, AssistantMessage, StopReason, TextContent, TextOrImageContent, ToolCallExecution,
};
use serde_json::{Value, json};
use support::*;

fn normalize_value(value: &mut Value) {
    match value {
        Value::Array(items) => items.iter_mut().for_each(normalize_value),
        Value::Object(map) => {
            for (key, entry) in map.iter_mut() {
                if matches!(key.as_str(), "timestamp" | "startedAt" | "finishedAt") {
                    *entry = json!(0);
                } else {
                    normalize_value(entry);
                }
            }
        }
        _ => {}
    }
}

fn normalized(value: Value) -> Value {
    let mut value = value;
    normalize_value(&mut value);
    value
}

fn value_tool_schema() -> Value {
    json!({"type": "object", "properties": {"value": {"type": "string"}}, "required": ["value"]})
}

fn context(messages: Vec<AgentMessage>, tools: Vec<Arc<dyn AgentTool>>, system_prompt: &str) -> AgentContext {
    AgentContext {
        system_prompt: JsString::from(system_prompt.to_owned()),
        messages,
        tools: Some(tools),
    }
}

fn config() -> AgentLoopConfig {
    AgentLoopConfig::new(model(), identity_convert())
}

fn recording_stream_fn(requests: Arc<Mutex<Vec<Value>>>, scripts: Vec<Script>) -> StreamFn {
    let base = scripted_stream_fn(scripts);
    Arc::new(move |model, request_context, options| {
        let request = json!({
            "systemPrompt": request_context.system_prompt,
            "messages": request_context.messages,
        });
        requests.lock().unwrap().push(normalized(request));
        base(model, request_context, options)
    })
}

async fn run_loop(
    prompts: Vec<AgentMessage>,
    context: AgentContext,
    config: AgentLoopConfig,
    requests: Arc<Mutex<Vec<Value>>>,
    scripts: Vec<Script>,
) -> Value {
    let stream = pi_agent_core::agent::agent_loop(
        prompts,
        context,
        config,
        None,
        Some(recording_stream_fn(requests.clone(), scripts)),
    );
    let (raw_events, result) = collect_events(stream).await;
    assert_eq!(event_types(&raw_events).first(), Some(&"agent_start"));
    let events: Vec<Value> = raw_events
        .iter()
        .map(|event| normalized(serde_json::to_value(event).unwrap()))
        .collect();
    let messages = result.expect("loop must settle");
    json!({
        "events": events,
        "messages": normalized(serde_json::to_value(messages).unwrap()),
        "requests": requests.lock().unwrap().clone(),
    })
}

async fn run_continue(
    context: AgentContext,
    config: AgentLoopConfig,
    requests: Arc<Mutex<Vec<Value>>>,
    scripts: Vec<Script>,
) -> Value {
    let stream = pi_agent_core::agent::agent_loop_continue(
        context,
        config,
        None,
        Some(recording_stream_fn(requests.clone(), scripts)),
    )
    .unwrap();
    let (raw_events, result) = collect_events(stream).await;
    assert_eq!(event_types(&raw_events).first(), Some(&"agent_start"));
    let events: Vec<Value> = raw_events
        .iter()
        .map(|event| normalized(serde_json::to_value(event).unwrap()))
        .collect();
    let messages = result.expect("loop must settle");
    json!({
        "events": events,
        "messages": normalized(serde_json::to_value(messages).unwrap()),
        "requests": requests.lock().unwrap().clone(),
    })
}

fn echo_tool() -> TestTool {
    let mut tool = TestTool::new("echo", |_id, params| {
        text_result(&pi_js_compat::js_format!(
            "echoed: {}",
            params["value"].as_string().expect("string tool argument")
        ))
    });
    tool.parameters = value_tool_schema();
    tool
}

fn event_name(event: &AgentEvent) -> &'static str {
    event_types(std::slice::from_ref(event))[0]
}

fn throwing_event_sink(events: Arc<Mutex<Vec<Value>>>, throw_on: &'static str) -> AgentEventSink {
    Arc::new(move |event| {
        let events = Arc::clone(&events);
        Box::pin(async move {
            let name = event_name(&event);
            events
                .lock()
                .unwrap()
                .push(normalized(serde_json::to_value(&event).unwrap()));
            if name == throw_on {
                return Err(AgentError::new(format!("listener exploded on {name}")));
            }
            Ok(())
        })
    })
}

async fn run_loop_with_throwing_emit(
    throw_on: &'static str,
    tools: Vec<Arc<dyn AgentTool>>,
    scripts: Vec<Script>,
) -> Value {
    let events = Arc::new(Mutex::new(Vec::new()));
    let result = pi_agent_core::agent::run_agent_loop(
        vec![AgentMessage::User(user("Hello"))],
        context(Vec::new(), tools, ""),
        config(),
        throwing_event_sink(Arc::clone(&events), throw_on),
        None,
        Some(scripted_stream_fn(scripts)),
    )
    .await;
    let mut output = json!({"events": events.lock().unwrap().clone()});
    match result {
        Ok(messages) => output["messages"] = normalized(serde_json::to_value(messages).unwrap()),
        Err(error) => output["error"] = json!(error.to_string()),
    }
    output
}

async fn run_agent_with_throwing_listener(
    throw_on: &'static str,
    tools: Vec<Arc<dyn AgentTool>>,
    scripts: Vec<Script>,
) -> Value {
    let events = Arc::new(Mutex::new(Vec::<Value>::new()));
    let requests = Arc::new(Mutex::new(Vec::new()));
    let agent = Agent::new(AgentOptions {
        initial_state: Some(pi_agent_core::agent::AgentInitialState {
            tools: Some(tools),
            ..Default::default()
        }),
        stream_fn: Some(recording_stream_fn(Arc::clone(&requests), scripts)),
        ..Default::default()
    })
    .unwrap();
    let events_hook = Arc::clone(&events);
    agent.subscribe(Arc::new(move |event, _signal| {
        let events_hook = Arc::clone(&events_hook);
        Box::pin(async move {
            let name = event_name(&event);
            events_hook
                .lock()
                .unwrap()
                .push(normalized(serde_json::to_value(&event).unwrap()));
            if name == throw_on {
                return Err(AgentError::new(format!("listener exploded on {name}")));
            }
            Ok(())
        })
    }));
    let error = agent.prompt("Hello").await.err().map(|error| error.to_string());
    let mut output = agent_output(&agent, events.lock().unwrap().clone(), &requests);
    if let Some(error) = error {
        output["error"] = json!(error);
    }
    output
}

fn big_text(mode: &JsString) -> String {
    match mode {
        mode if mode == "lines" => (0..2001)
            .map(|index| char::from(b'a' + (index % 26) as u8).to_string())
            .collect::<Vec<_>>()
            .join("\n"),
        mode if mode == "single" => "a".repeat(20000),
        mode if mode == "emoji" => "😀".repeat(3000),
        _ => panic!("unknown big mode {mode:?}"),
    }
}

fn big_details(mode: &JsString) -> Value {
    let value = if mode == "lines" {
        "y".repeat(20000)
    } else {
        big_text(mode)
    };
    json!({"2": 2.0, "1": 1.0, "value": value})
}

fn tool_message(id: &str, name: &str, value: &str) -> AssistantMessage {
    tool_use(vec![AssistantContent::ToolCall(tool_call(
        id,
        name,
        json!({"value": value}),
    ))])
}

async fn run_scenario(name: &str) -> Value {
    let requests = Arc::new(Mutex::new(Vec::new()));
    match name {
        "text" => {
            run_loop(
                vec![AgentMessage::User(user("Hello"))],
                context(Vec::new(), Vec::new(), ""),
                config(),
                requests,
                vec![Script::Events(vec![pi_ai::types::AssistantMessageEvent::Done {
                    reason: pi_ai::types::DoneReason::Stop,
                    message: text_assistant("Hi there!"),
                }])],
            )
            .await
        }
        "tools" => {
            run_loop(
                vec![AgentMessage::User(user("echo something"))],
                context(Vec::new(), vec![Arc::new(echo_tool())], ""),
                config(),
                requests,
                vec![
                    Script::message(tool_message("tool-1", "echo", "hello")),
                    Script::message(text_assistant("done")),
                ],
            )
            .await
        }
        "tool_error" => {
            let mut tool = echo_tool();
            tool.execute = Arc::new(|_id, _params, _signal, _on_update| {
                Box::pin(async { Err(pi_agent_core::agent::ToolError::new("tool exploded")) })
            });
            run_loop(
                vec![AgentMessage::User(user("echo"))],
                context(Vec::new(), vec![Arc::new(tool)], ""),
                config(),
                requests,
                vec![
                    Script::message(tool_message("tool-1", "echo", "hello")),
                    Script::message(text_assistant("done")),
                ],
            )
            .await
        }
        "parallel_order" => {
            let release_first = Arc::new(tokio::sync::Notify::new());
            let mut tool = echo_tool();
            tool.execute = {
                let release_first = Arc::clone(&release_first);
                Arc::new(move |_id, params, _signal, _on_update| {
                    let value = params["value"].as_string().expect("string tool argument").to_owned();
                    let release_first = Arc::clone(&release_first);
                    Box::pin(async move {
                        if value == "first" {
                            release_first.notified().await;
                        }
                        if value == "second" {
                            release_first.notify_waiters();
                        }
                        Ok(text_result(&pi_js_compat::js_format!("echoed: {}", value)))
                    })
                })
            };
            let mut cfg = config();
            cfg.tool_execution = ToolExecutionMode::Parallel;
            run_loop(
                vec![AgentMessage::User(user("echo both"))],
                context(Vec::new(), vec![Arc::new(tool)], ""),
                cfg,
                requests,
                vec![
                    Script::message(tool_use(vec![
                        AssistantContent::ToolCall(tool_call("tool-1", "echo", json!({"value": "first"}))),
                        AssistantContent::ToolCall(tool_call("tool-2", "echo", json!({"value": "second"}))),
                    ])),
                    Script::message(text_assistant("done")),
                ],
            )
            .await
        }
        "sequential_config" | "sequential_tool_mode" => {
            let mut tool = echo_tool();
            if name == "sequential_tool_mode" {
                tool.execution_mode = Some(ToolExecutionMode::Sequential);
            }
            let mut cfg = config();
            if name == "sequential_config" {
                cfg.tool_execution = ToolExecutionMode::Sequential;
            }
            run_loop(
                vec![AgentMessage::User(user("echo both"))],
                context(Vec::new(), vec![Arc::new(tool)], ""),
                cfg,
                requests,
                vec![
                    Script::message(tool_use(vec![
                        AssistantContent::ToolCall(tool_call("tool-1", "echo", json!({"value": "first"}))),
                        AssistantContent::ToolCall(tool_call("tool-2", "echo", json!({"value": "second"}))),
                    ])),
                    Script::message(text_assistant("done")),
                ],
            )
            .await
        }
        "blocked_terminate" => {
            let mut cfg = config();
            cfg.before_tool_call = Some(Arc::new(|_ctx, _signal| {
                Box::pin(async {
                    Ok(Some(BeforeToolCallResult {
                        block: Some(true),
                        reason: Some(JsString::from("Blocked by policy")),
                    }))
                })
            }));
            run_loop(
                vec![AgentMessage::User(user("echo"))],
                context(Vec::new(), vec![Arc::new(echo_tool())], ""),
                cfg,
                requests,
                vec![
                    Script::message(tool_message("tool-1", "echo", "hello")),
                    Script::message(text_assistant("should not run")),
                ],
            )
            .await
        }
        "before_mutation" => {
            let seen = Arc::new(Mutex::new(Vec::<JsValue>::new()));
            let mut tool = echo_tool();
            tool.execute = {
                let seen = Arc::clone(&seen);
                Arc::new(move |_id, params, _signal, _on_update| {
                    let seen = Arc::clone(&seen);
                    Box::pin(async move {
                        seen.lock().unwrap().push(params.clone());
                        Ok(text_result(&pi_js_compat::js_format!(
                            "echoed: {}",
                            pi_js_compat::js_json::stringify_lossless(&params["value"])
                        )))
                    })
                })
            };
            let mut cfg = config();
            cfg.before_tool_call = Some(Arc::new(|ctx, _signal| {
                Box::pin(async move {
                    ctx.set_args(JsValue::from(json!({"value": 123})));
                    Ok(None)
                })
            }));
            let mut output = run_loop(
                vec![AgentMessage::User(user("echo"))],
                context(Vec::new(), vec![Arc::new(tool)], ""),
                cfg,
                requests,
                vec![
                    Script::message(tool_message("tool-1", "echo", "hello")),
                    Script::message(text_assistant("done")),
                ],
            )
            .await;
            output["toolArgs"] = json!(seen.lock().unwrap().clone());
            output
        }
        "terminate_all" => {
            let mut tool = echo_tool();
            tool.execute = Arc::new(|_id, params, _signal, _on_update| {
                let value = params["value"].as_string().expect("string tool argument").to_owned();
                Box::pin(async move {
                    Ok(AgentToolResult {
                        terminate: Some(true),
                        ..text_result(&pi_js_compat::js_format!("echoed: {}", value))
                    })
                })
            });
            run_loop(
                vec![AgentMessage::User(user("echo"))],
                context(Vec::new(), vec![Arc::new(tool)], ""),
                config(),
                requests,
                vec![Script::message(tool_message("tool-1", "echo", "hello"))],
            )
            .await
        }
        "terminate_mixed" => {
            let mut tool = echo_tool();
            tool.execute = Arc::new(|_id, params, _signal, _on_update| {
                let value = params["value"].as_string().expect("string tool argument").to_owned();
                Box::pin(async move {
                    Ok(AgentToolResult {
                        terminate: Some(value == "first"),
                        ..text_result(&pi_js_compat::js_format!("echoed: {}", value))
                    })
                })
            });
            let mut cfg = config();
            cfg.tool_execution = ToolExecutionMode::Parallel;
            run_loop(
                vec![AgentMessage::User(user("echo both"))],
                context(Vec::new(), vec![Arc::new(tool)], ""),
                cfg,
                requests,
                vec![
                    Script::message(tool_use(vec![
                        AssistantContent::ToolCall(tool_call("tool-1", "echo", json!({"value": "first"}))),
                        AssistantContent::ToolCall(tool_call("tool-2", "echo", json!({"value": "second"}))),
                    ])),
                    Script::message(text_assistant("done")),
                ],
            )
            .await
        }
        "after_tool_call" => {
            let mut cfg = config();
            cfg.after_tool_call = Some(Arc::new(|_ctx, _signal| {
                Box::pin(async {
                    Ok(Some(AfterToolCallResult {
                        content: Some(vec![TextOrImageContent::Text(TextContent::new("overridden"))]),
                        details: Some(JsValue::from(json!({"overridden": true}))),
                        is_error: Some(true),
                        terminate: Some(true),
                    }))
                })
            }));
            run_loop(
                vec![AgentMessage::User(user("echo"))],
                context(Vec::new(), vec![Arc::new(echo_tool())], ""),
                cfg,
                requests,
                vec![Script::message(tool_message("tool-1", "echo", "hello"))],
            )
            .await
        }
        "prepare_arguments_error" => {
            let mut tool = echo_tool();
            tool.prepare_arguments = Some(Arc::new(|_args: JsValue| {
                Err(pi_agent_core::agent::ToolError::new("prepareArguments exploded"))
            }));
            tool.execute =
                Arc::new(|_id, _params, _signal, _on_update| Box::pin(async { Ok(text_result("should not run")) }));
            run_loop(
                vec![AgentMessage::User(user("echo"))],
                context(Vec::new(), vec![Arc::new(tool)], ""),
                config(),
                requests,
                vec![
                    Script::message(tool_message("tool-1", "echo", "hello")),
                    Script::message(text_assistant("done")),
                ],
            )
            .await
        }
        "prepare_execution" => {
            let mut tool = echo_tool();
            tool.prepare_execution = Some(Arc::new(|_id, _signal| {
                Box::pin(async {
                    Ok(Some(ToolCallExecution {
                        agent_id: "known-agent".to_owned(),
                    }))
                })
            }));
            tool.execute = Arc::new(|_id, params, _signal, _on_update| {
                let value = params["value"].as_string().expect("string tool argument").to_owned();
                Box::pin(async move { Ok(text_result(&pi_js_compat::js_format!("{}:known-agent", value))) })
            });
            run_loop(
                vec![AgentMessage::User(user("echo"))],
                context(Vec::new(), vec![Arc::new(tool)], ""),
                config(),
                requests,
                vec![
                    Script::message(tool_use(vec![AssistantContent::ToolCall(tool_call(
                        "provider-call",
                        "echo",
                        json!({"value": "hello"}),
                    ))])),
                    Script::message(text_assistant("done")),
                ],
            )
            .await
        }
        "steering_error" | "follow_up_error" | "stream_acquisition_error" => {
            let mut cfg = config();
            let hook: pi_agent_core::agent::MessageQueueHook =
                Arc::new(|| Box::pin(async { Err(pi_agent_core::agent::AgentError::new("queue hook failed")) }));
            if name == "steering_error" {
                cfg.get_steering_messages = Some(hook);
            } else if name == "follow_up_error" {
                cfg.get_follow_up_messages = Some(hook);
            }
            let stream: StreamFn = if name == "stream_acquisition_error" {
                Arc::new(|_, _, _| Box::pin(async { Err(pi_agent_core::AgentError::new("stream acquisition failed")) }))
            } else {
                recording_stream_fn(requests.clone(), vec![Script::message(text_assistant("answer"))])
            };
            let events = Arc::new(Mutex::new(Vec::new()));
            let captured = events.clone();
            let emit: pi_agent_core::agent::AgentEventSink = Arc::new(move |event| {
                captured
                    .lock()
                    .unwrap()
                    .push(normalized(serde_json::to_value(event).unwrap()));
                Box::pin(async { Ok(()) })
            });
            let result = pi_agent_core::agent::run_agent_loop(
                vec![AgentMessage::User(user("work"))],
                context(Vec::new(), Vec::new(), ""),
                cfg,
                emit,
                None,
                Some(stream),
            )
            .await;
            json!({"events":events.lock().unwrap().clone(),"error":result.unwrap_err().message(),"requests":requests.lock().unwrap().clone()})
        }
        "steering" => {
            let polls = Arc::new(AtomicUsize::new(0));
            let mut cfg = config();
            cfg.get_steering_messages = Some(Arc::new(move || {
                let polls = Arc::clone(&polls);
                Box::pin(async move {
                    if polls.fetch_add(1, Ordering::SeqCst) == 1 {
                        Ok(vec![AgentMessage::User(user("interrupt"))])
                    } else {
                        Ok(Vec::new())
                    }
                })
            }));
            run_loop(
                vec![AgentMessage::User(user("work"))],
                context(Vec::new(), Vec::new(), ""),
                cfg,
                requests,
                vec![
                    Script::message(text_assistant("first")),
                    Script::message(text_assistant("second")),
                ],
            )
            .await
        }
        "follow_up" => {
            let polls = Arc::new(AtomicUsize::new(0));
            let mut cfg = config();
            cfg.get_follow_up_messages = Some(Arc::new(move || {
                let polls = Arc::clone(&polls);
                Box::pin(async move {
                    if polls.fetch_add(1, Ordering::SeqCst) == 0 {
                        Ok(vec![AgentMessage::User(user("follow up"))])
                    } else {
                        Ok(Vec::new())
                    }
                })
            }));
            run_loop(
                vec![AgentMessage::User(user("work"))],
                context(Vec::new(), Vec::new(), ""),
                cfg,
                requests,
                vec![
                    Script::message(text_assistant("first")),
                    Script::message(text_assistant("second")),
                ],
            )
            .await
        }
        "error_stop" | "done_error" => {
            let mut message = text_assistant("");
            message.stop_reason = StopReason::Error;
            let script = if name == "done_error" {
                Script::Events(vec![
                    serde_json::from_value(json!({"type":"done","reason":"error","message":message})).unwrap(),
                ])
            } else {
                Script::message(message)
            };
            run_loop(
                vec![AgentMessage::User(user("work"))],
                context(Vec::new(), Vec::new(), ""),
                config(),
                requests,
                vec![script],
            )
            .await
        }
        "length_stop" => {
            let mut message = text_assistant("partial");
            message.stop_reason = StopReason::Length;
            run_loop(
                vec![AgentMessage::User(user("work"))],
                context(Vec::new(), Vec::new(), ""),
                config(),
                requests,
                vec![Script::message(message)],
            )
            .await
        }
        "prepare_next_turn" => {
            let prepared = Arc::new(AtomicUsize::new(0));
            let mut cfg = config();
            cfg.prepare_next_turn = Some(Arc::new(move |turn_context| {
                let prepared = Arc::clone(&prepared);
                Box::pin(async move {
                    if prepared.fetch_add(1, Ordering::SeqCst) > 0 {
                        return Ok(None);
                    }
                    Ok(Some(pi_agent_core::agent::AgentLoopTurnUpdate {
                        context: Some(AgentContext {
                            system_prompt: JsString::from("second prompt"),
                            messages: turn_context.context.messages.clone(),
                            tools: turn_context.context.tools.clone(),
                        }),
                        model: None,
                        thinking_level: None,
                    }))
                })
            }));
            run_loop(
                vec![AgentMessage::User(user("work"))],
                context(Vec::new(), Vec::new(), "first prompt"),
                cfg,
                requests,
                vec![
                    Script::message(text_assistant("first")),
                    Script::message(text_assistant("second")),
                ],
            )
            .await
        }
        "should_stop" => {
            let steering_polls = Arc::new(AtomicUsize::new(0));
            let follow_up_polls = Arc::new(AtomicUsize::new(0));
            let mut cfg = config();
            cfg.get_steering_messages = {
                let steering_polls = Arc::clone(&steering_polls);
                Some(Arc::new(move || {
                    let steering_polls = Arc::clone(&steering_polls);
                    Box::pin(async move {
                        steering_polls.fetch_add(1, Ordering::SeqCst);
                        Ok(Vec::new())
                    })
                }))
            };
            cfg.get_follow_up_messages = {
                let follow_up_polls = Arc::clone(&follow_up_polls);
                Some(Arc::new(move || {
                    let follow_up_polls = Arc::clone(&follow_up_polls);
                    Box::pin(async move {
                        follow_up_polls.fetch_add(1, Ordering::SeqCst);
                        Ok(vec![AgentMessage::User(user("stays queued"))])
                    })
                }))
            };
            cfg.should_stop_after_turn = Some(Arc::new(|_context| Box::pin(async { Ok(true) })));
            let mut output = run_loop(
                vec![AgentMessage::User(user("work"))],
                context(Vec::new(), Vec::new(), ""),
                cfg,
                requests,
                vec![Script::message(text_assistant("first"))],
            )
            .await;
            output["polls"] = json!({
                "steeringPolls": steering_polls.load(Ordering::SeqCst),
                "followUpPolls": follow_up_polls.load(Ordering::SeqCst),
            });
            output
        }
        "end_turn" => {
            let mut tool = TestTool::no_args("end_turn", || AgentToolResult {
                terminate: Some(true),
                ..text_result("Ended: Finished requested work")
            });
            tool.parameters = json!({
                "type": "object",
                "properties": {"reason": {"type": "string"}},
                "required": ["reason"]
            });
            run_loop(
                vec![AgentMessage::User(user("work"))],
                context(Vec::new(), vec![Arc::new(tool)], ""),
                config(),
                requests,
                vec![
                    Script::message(text_assistant("intermediate update")),
                    Script::message(tool_use(vec![AssistantContent::ToolCall(tool_call(
                        "end-1",
                        "end_turn",
                        json!({"reason": "Finished requested work"}),
                    ))])),
                ],
            )
            .await
        }
        "continue_restored" => {
            let unfinished = tool_message("tool-restored", "echo", "restored");
            run_continue(
                context(
                    vec![
                        AgentMessage::User(user("Echo this")),
                        AgentMessage::Assistant(Box::new(unfinished)),
                    ],
                    vec![Arc::new(echo_tool())],
                    "",
                ),
                config(),
                requests,
                vec![Script::message(text_assistant("Done"))],
            )
            .await
        }
        "continue_partial_results" => {
            let interrupted = tool_use(vec![
                AssistantContent::ToolCall(tool_call("tool-complete", "echo", json!({"value": "complete"}))),
                AssistantContent::ToolCall(tool_call("tool-pending", "echo", json!({"value": "pending"}))),
            ]);
            let earlier = pi_ai::types::ToolResultMessage {
                role: pi_ai::types::ToolResultRole::Tag,
                tool_call_id: "tool-complete".to_owned(),
                tool_name: "echo".to_owned(),
                content: vec![TextOrImageContent::Text(TextContent::new("complete"))],
                details: None,
                is_error: false,
                timestamp: 1000.0,
            };
            let mut tool = echo_tool();
            tool.execute = Arc::new(|_id, params, _signal, _on_update| {
                let value = params["value"].as_string().expect("string tool argument").to_owned();
                Box::pin(async move { Ok(text_result(&value)) })
            });
            run_continue(
                context(
                    vec![
                        AgentMessage::User(user("Echo twice")),
                        AgentMessage::Assistant(Box::new(interrupted)),
                        AgentMessage::ToolResult(earlier),
                    ],
                    vec![Arc::new(tool)],
                    "",
                ),
                config(),
                requests,
                vec![Script::message(text_assistant("Done"))],
            )
            .await
        }
        "convert_custom" => {
            let custom = AgentMessage::Custom(
                (json!({"role": "notification", "text": "This is a notification", "timestamp": 1000})
                    .as_object()
                    .cloned()
                    .unwrap())
                .into_iter()
                .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
                .collect::<pi_js_compat::js_json::JsObject>(),
            );
            let mut cfg = config();
            cfg.convert_to_llm = Arc::new(|messages| {
                Box::pin(async move {
                    Ok(messages
                        .into_iter()
                        .filter_map(|message| match message {
                            AgentMessage::User(message) => Some(pi_ai::types::Message::User(message)),
                            AgentMessage::Assistant(message) => Some(pi_ai::types::Message::Assistant(message)),
                            AgentMessage::ToolResult(message) => Some(pi_ai::types::Message::ToolResult(message)),
                            AgentMessage::Custom(_) => None,
                        })
                        .collect())
                })
            });
            run_loop(
                vec![AgentMessage::User(user("Hello"))],
                context(vec![custom], Vec::new(), ""),
                cfg,
                requests,
                vec![Script::message(text_assistant("Response"))],
            )
            .await
        }
        "parallel_update" => {
            let mut tool = echo_tool();
            tool.execute = Arc::new(|_id, params, _signal, on_update| {
                let value = params["value"].as_string().expect("string tool argument").to_owned();
                if let Err(error) = on_update(AgentToolResult {
                    content: vec![TextOrImageContent::Text(TextContent::new("partial"))],
                    details: Some(JsValue::from(json!({"status": "partial"}))),
                    is_error: None,
                    terminate: None,
                }) {
                    return Box::pin(async move { Err(error) });
                }
                Box::pin(async move { Ok(text_result(&pi_js_compat::js_format!("echoed: {}", value))) })
            });
            run_loop(
                vec![AgentMessage::User(user("echo"))],
                context(Vec::new(), vec![Arc::new(tool)], ""),
                config(),
                requests,
                vec![
                    Script::message(tool_message("tool-1", "echo", "hello")),
                    Script::message(text_assistant("done")),
                ],
            )
            .await
        }
        "abort_stream" => {
            let controller_signal = pi_js_compat::abort::AbortController::new();
            let signal = controller_signal.signal();
            let stream_fn: StreamFn = {
                let requests = Arc::clone(&requests);
                Arc::new(move |_model, request_context, _options| {
                    requests.lock().unwrap().push(normalized(json!({
                        "systemPrompt": request_context.system_prompt,
                        "messages": request_context.messages,
                    })));
                    Box::pin(async {
                        let (sender, stream) = pi_ai::utils::event_stream::create_assistant_message_event_stream();
                        sender.push(pi_ai::types::AssistantMessageEvent::Start {
                            partial: AssistantMessage::empty("openai-responses", "openai", "mock", 1000.0),
                        });
                        std::mem::forget(sender);
                        Ok(stream)
                    })
                })
            };
            let mut stream = pi_agent_core::agent::agent_loop(
                vec![AgentMessage::User(user("work"))],
                context(Vec::new(), Vec::new(), ""),
                config(),
                Some(signal.clone()),
                Some(stream_fn),
            );
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                controller_signal.abort(None);
            });
            let mut events = Vec::new();
            let mut error = None;
            loop {
                match stream.next().await {
                    Some(Ok(event)) => events.push(normalized(serde_json::to_value(event).unwrap())),
                    Some(Err(failure)) => {
                        error = Some(failure.message().to_owned());
                        break;
                    }
                    None => break,
                }
            }
            if error.is_none()
                && let Err(failure) = stream.result().await
            {
                error = Some(failure.message().to_owned());
            }
            json!({"events": events, "error": error, "requests": requests.lock().unwrap().clone()})
        }
        "abort_tool" => {
            let started = Arc::new(tokio::sync::Notify::new());
            let started_hook = Arc::clone(&started);
            let mut tool = TestTool::no_args("stuck_tool", || text_result("unused"));
            tool.execute = Arc::new(move |_id, _params, _signal, _on_update| {
                let started_hook = Arc::clone(&started_hook);
                Box::pin(async move {
                    started_hook.notify_waiters();
                    std::future::pending::<()>().await;
                    Ok(text_result("unreachable"))
                })
            });
            let events = Arc::new(Mutex::new(Vec::<Value>::new()));
            let agent = Agent::new(AgentOptions {
                initial_state: Some(pi_agent_core::agent::AgentInitialState {
                    tools: Some(vec![Arc::new(tool)]),
                    ..Default::default()
                }),
                stream_fn: Some(recording_stream_fn(
                    Arc::clone(&requests),
                    vec![Script::message(tool_use(vec![AssistantContent::ToolCall(tool_call(
                        "call-1",
                        "stuck_tool",
                        json!({}),
                    ))]))],
                )),
                ..Default::default()
            })
            .unwrap();
            let events_hook = Arc::clone(&events);
            agent.subscribe(Arc::new(move |event, _signal| {
                let events_hook = Arc::clone(&events_hook);
                Box::pin(async move {
                    events_hook
                        .lock()
                        .unwrap()
                        .push(normalized(serde_json::to_value(event).unwrap()));
                    Ok(())
                })
            }));
            let agent_clone = agent.clone();
            let prompt = tokio::spawn(async move { agent_clone.prompt("run stuck tool").await });
            started.notified().await;
            agent.abort();
            prompt.await.unwrap().unwrap();
            agent_output(&agent, events.lock().unwrap().clone(), &requests)
        }
        "agent_text" => {
            let events = Arc::new(Mutex::new(Vec::<Value>::new()));
            let agent = Agent::new(AgentOptions {
                stream_fn: Some(recording_stream_fn(
                    Arc::clone(&requests),
                    vec![Script::Behavior(Arc::new(|_model, _context, _options| {
                        Box::pin(async {
                            let (sender, stream) = pi_ai::utils::event_stream::create_assistant_message_event_stream();
                            sender.push(pi_ai::types::AssistantMessageEvent::Done {
                                reason: pi_ai::types::DoneReason::Stop,
                                message: text_assistant("Hi"),
                            });
                            stream
                        })
                    }))],
                )),
                ..Default::default()
            })
            .unwrap();
            let events_hook = Arc::clone(&events);
            agent.subscribe(Arc::new(move |event, _signal| {
                let events_hook = Arc::clone(&events_hook);
                Box::pin(async move {
                    events_hook
                        .lock()
                        .unwrap()
                        .push(normalized(serde_json::to_value(event).unwrap()));
                    Ok(())
                })
            }));
            agent.prompt("Hello").await.unwrap();
            agent_output(&agent, events.lock().unwrap().clone(), &requests)
        }
        "agent_continue_queue" => {
            let events = Arc::new(Mutex::new(Vec::<Value>::new()));
            let agent = Agent::new(AgentOptions {
                stream_fn: Some(recording_stream_fn(
                    Arc::clone(&requests),
                    vec![Script::message(text_assistant("Processed"))],
                )),
                ..Default::default()
            })
            .unwrap();
            agent.set_messages(vec![
                AgentMessage::User(user_block("Initial")),
                AgentMessage::Assistant(Box::new(text_assistant("Initial response"))),
            ]);
            agent.follow_up(AgentMessage::User(user_block("Queued follow-up")));
            let events_hook = Arc::clone(&events);
            agent.subscribe(Arc::new(move |event, _signal| {
                let events_hook = Arc::clone(&events_hook);
                Box::pin(async move {
                    events_hook
                        .lock()
                        .unwrap()
                        .push(normalized(serde_json::to_value(event).unwrap()));
                    Ok(())
                })
            }));
            agent.continue_run(ContinueOptions::default()).await.unwrap();
            agent_output(&agent, events.lock().unwrap().clone(), &requests)
        }
        "listener_message_end_loop" => {
            run_loop_with_throwing_emit(
                "message_end",
                Vec::new(),
                vec![Script::message(text_assistant("Hi there!"))],
            )
            .await
        }
        "listener_agent_end_loop" => {
            run_loop_with_throwing_emit(
                "agent_end",
                Vec::new(),
                vec![Script::message(text_assistant("Hi there!"))],
            )
            .await
        }
        "listener_tool_execution_end_loop" => {
            run_loop_with_throwing_emit(
                "tool_execution_end",
                vec![Arc::new(echo_tool())],
                vec![
                    Script::message(tool_message("tool-1", "echo", "hello")),
                    Script::message(text_assistant("done")),
                ],
            )
            .await
        }
        "listener_message_end_agent" => {
            run_agent_with_throwing_listener(
                "message_end",
                Vec::new(),
                vec![Script::message(text_assistant("Hi there!"))],
            )
            .await
        }
        "listener_agent_end_agent" => {
            run_agent_with_throwing_listener(
                "agent_end",
                Vec::new(),
                vec![Script::message(text_assistant("Hi there!"))],
            )
            .await
        }
        "listener_tool_execution_end_agent" => {
            run_agent_with_throwing_listener(
                "tool_execution_end",
                vec![Arc::new(echo_tool())],
                vec![
                    Script::message(tool_message("tool-1", "echo", "hello")),
                    Script::message(text_assistant("done")),
                ],
            )
            .await
        }
        "tool_result_cap" => {
            let mut tool = TestTool::new("big", |_id, _params| text_result(""));
            tool.parameters = json!({
                "type": "object",
                "properties": {"mode": {"type": "string"}},
                "required": ["mode"]
            });
            tool.execute = Arc::new(|_id, params, _signal, on_update| {
                let mode = params["mode"].as_string().expect("string tool argument").to_owned();
                Box::pin(async move {
                    let text = big_text(&mode);
                    if mode == "lines" {
                        on_update(AgentToolResult {
                            content: vec![TextOrImageContent::Text(TextContent::new(text.clone()))],
                            details: Some(JsValue::from(json!({"value": "z".repeat(20000)}))),
                            is_error: None,
                            terminate: None,
                        })?;
                    }
                    Ok(AgentToolResult {
                        content: vec![TextOrImageContent::Text(TextContent::new(text))],
                        details: Some(JsValue::from(big_details(&mode))),
                        is_error: None,
                        terminate: None,
                    })
                })
            });
            let mut cfg = config();
            cfg.tool_execution = ToolExecutionMode::Sequential;
            run_loop(
                vec![AgentMessage::User(user("cap results"))],
                context(Vec::new(), vec![Arc::new(tool)], ""),
                cfg,
                requests,
                vec![
                    Script::message(tool_use(vec![
                        AssistantContent::ToolCall(tool_call("cap-lines", "big", json!({"mode": "lines"}))),
                        AssistantContent::ToolCall(tool_call("cap-single", "big", json!({"mode": "single"}))),
                        AssistantContent::ToolCall(tool_call("cap-emoji", "big", json!({"mode": "emoji"}))),
                    ])),
                    Script::message(text_assistant("done")),
                ],
            )
            .await
        }
        other => panic!("unknown scenario {other}"),
    }
}

fn agent_output(agent: &Agent, events: Vec<Value>, requests: &Arc<Mutex<Vec<Value>>>) -> Value {
    let state = agent.state();
    json!({
        "events": events,
        "messages": normalized(serde_json::to_value(state.messages).unwrap()),
        "state": {
            "isStreaming": state.is_streaming,
            "isModelRequestActive": state.is_model_request_active,
            "streamingMessage": normalized(serde_json::to_value(state.streaming_message).unwrap()),
            "pendingToolCalls": state.pending_tool_calls.iter().collect::<Vec<_>>(),
            "errorMessage": state.error_message,
        },
        "requests": requests.lock().unwrap().clone(),
    })
}

fn first_difference(path: &str, actual: &Value, expected: &Value) -> Option<String> {
    match (actual, expected) {
        (Value::Object(actual_map), Value::Object(expected_map)) => {
            let keys: std::collections::BTreeSet<&String> = actual_map.keys().chain(expected_map.keys()).collect();
            for key in keys {
                let child_path = format!("{path}.{key}");
                match (actual_map.get(key), expected_map.get(key)) {
                    (Some(actual), Some(expected)) => {
                        if let Some(difference) = first_difference(&child_path, actual, expected) {
                            return Some(difference);
                        }
                    }
                    (actual, expected) => {
                        return Some(format!("{child_path}: actual={actual:?} expected={expected:?}"));
                    }
                }
            }
            None
        }
        (Value::Array(actual_items), Value::Array(expected_items)) => {
            if actual_items.len() != expected_items.len() {
                return Some(format!(
                    "{path}.length: actual={} expected={}",
                    actual_items.len(),
                    expected_items.len()
                ));
            }
            for (index, (actual, expected)) in actual_items.iter().zip(expected_items).enumerate() {
                if let Some(difference) = first_difference(&format!("{path}[{index}]"), actual, expected) {
                    return Some(difference);
                }
            }
            None
        }
        _ => {
            if actual == expected {
                None
            } else {
                Some(format!("{path}: actual={actual} expected={expected}"))
            }
        }
    }
}

pub async fn run_differential() {
    let scenarios = [
        "text",
        "tools",
        "tool_error",
        "parallel_order",
        "sequential_config",
        "sequential_tool_mode",
        "blocked_terminate",
        "before_mutation",
        "terminate_all",
        "terminate_mixed",
        "after_tool_call",
        "prepare_arguments_error",
        "prepare_execution",
        "steering",
        "follow_up",
        "error_stop",
        "length_stop",
        "prepare_next_turn",
        "should_stop",
        "end_turn",
        "continue_restored",
        "continue_partial_results",
        "convert_custom",
        "parallel_update",
        "abort_stream",
        "abort_tool",
        "agent_text",
        "agent_continue_queue",
        "listener_message_end_loop",
        "listener_tool_execution_end_loop",
        "listener_agent_end_loop",
        "listener_message_end_agent",
        "listener_tool_execution_end_agent",
        "listener_agent_end_agent",
        "tool_result_cap",
    ];
    compare_scenarios(&scenarios).await;
}

pub async fn run_done_error_differential() {
    compare_scenarios(&["done_error"]).await;
}

pub async fn run_rejection_differential() {
    compare_scenarios(&["steering_error", "follow_up_error", "stream_acquisition_error"]).await;
}

async fn compare_scenarios(scenarios: &[&str]) {
    let mut oracle = oracle::TsOracle::spawn("agent-loop-oracle.ts", "packages/agent-core/src/agent-loop.ts");
    for scenario in scenarios {
        let response = oracle.request(&json!({"id": scenario, "scenario": scenario}));
        assert_eq!(response["ok"], json!(true), "oracle failed for {scenario}: {response}");
        let expected = response["output"].clone();
        let actual = run_scenario(scenario).await;
        if let Some(difference) = first_difference(scenario, &actual, &expected) {
            panic!("differential mismatch for {scenario}: {difference}");
        }
    }
}
