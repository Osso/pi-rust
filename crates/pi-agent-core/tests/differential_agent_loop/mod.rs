#[path = "support.rs"]
mod oracle;
#[path = "../support/mod.rs"]
mod support;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use pi_agent_core::agent::{
    AfterToolCallResult, Agent, AgentContext, AgentLoopConfig, AgentMessage, AgentOptions, AgentTool, AgentToolResult,
    ContinueOptions, StreamFn,
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
        system_prompt: system_prompt.to_owned(),
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
        text_result(&format!("echoed: {}", params["value"].as_str().unwrap_or_default()))
    });
    tool.parameters = value_tool_schema();
    tool
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
                    let value = params["value"].as_str().unwrap_or_default().to_owned();
                    let release_first = Arc::clone(&release_first);
                    Box::pin(async move {
                        if value == "first" {
                            release_first.notified().await;
                        }
                        if value == "second" {
                            release_first.notify_waiters();
                        }
                        Ok(text_result(&format!("echoed: {value}")))
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
                        reason: Some("Blocked by policy".to_owned()),
                        terminate: Some(true),
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
            let seen = Arc::new(Mutex::new(Vec::<Value>::new()));
            let mut tool = echo_tool();
            tool.execute = {
                let seen = Arc::clone(&seen);
                Arc::new(move |_id, params, _signal, _on_update| {
                    let seen = Arc::clone(&seen);
                    Box::pin(async move {
                        seen.lock().unwrap().push(params.clone());
                        Ok(text_result(&format!("echoed: {}", params["value"])))
                    })
                })
            };
            let mut cfg = config();
            cfg.before_tool_call = Some(Arc::new(|ctx, _signal| {
                Box::pin(async move {
                    ctx.set_args(json!({"value": 123}));
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
            output["toolArgs"] = Value::Array(seen.lock().unwrap().clone());
            output
        }
        "terminate_all" => {
            let mut tool = echo_tool();
            tool.execute = Arc::new(|_id, params, _signal, _on_update| {
                let value = params["value"].as_str().unwrap_or_default().to_owned();
                Box::pin(async move {
                    Ok(AgentToolResult {
                        terminate: Some(true),
                        ..text_result(&format!("echoed: {value}"))
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
                let value = params["value"].as_str().unwrap_or_default().to_owned();
                Box::pin(async move {
                    Ok(AgentToolResult {
                        terminate: Some(value == "first"),
                        ..text_result(&format!("echoed: {value}"))
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
                        details: Some(json!({"overridden": true})),
                        is_error: Some(true),
                        usage: None,
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
        "prepare_execution" => {
            let mut tool = echo_tool();
            tool.prepare_execution = Some(Arc::new(|_id, _signal| {
                Box::pin(async {
                    Some(ToolCallExecution {
                        agent_id: "known-agent".to_owned(),
                    })
                })
            }));
            tool.execute = Arc::new(|_id, params, _signal, _on_update| {
                let value = params["value"].as_str().unwrap_or_default().to_owned();
                Box::pin(async move { Ok(text_result(&format!("{value}:known-agent"))) })
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
        "steering" => {
            let polls = Arc::new(AtomicUsize::new(0));
            let mut cfg = config();
            cfg.get_steering_messages = Some(Arc::new(move || {
                let polls = Arc::clone(&polls);
                Box::pin(async move {
                    if polls.fetch_add(1, Ordering::SeqCst) == 1 {
                        vec![AgentMessage::User(user("interrupt"))]
                    } else {
                        Vec::new()
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
                        vec![AgentMessage::User(user("follow up"))]
                    } else {
                        Vec::new()
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
        "error_stop" => {
            let mut message = text_assistant("");
            message.stop_reason = StopReason::Error;
            run_loop(
                vec![AgentMessage::User(user("work"))],
                context(Vec::new(), Vec::new(), ""),
                config(),
                requests,
                vec![Script::message(message)],
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
                            system_prompt: "second prompt".to_owned(),
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
                        Vec::new()
                    })
                }))
            };
            cfg.get_follow_up_messages = {
                let follow_up_polls = Arc::clone(&follow_up_polls);
                Some(Arc::new(move || {
                    let follow_up_polls = Arc::clone(&follow_up_polls);
                    Box::pin(async move {
                        follow_up_polls.fetch_add(1, Ordering::SeqCst);
                        vec![AgentMessage::User(user("stays queued"))]
                    })
                }))
            };
            cfg.should_stop_after_turn = Some(Arc::new(|_context, _signal| Box::pin(async { Ok(true) })));
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
                usage: None,
                added_tool_names: None,
                is_error: false,
                timestamp: 1000,
            };
            let mut tool = echo_tool();
            tool.execute = Arc::new(|_id, params, _signal, _on_update| {
                let value = params["value"].as_str().unwrap_or_default().to_owned();
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
                json!({"role": "notification", "text": "This is a notification", "timestamp": 1000})
                    .as_object()
                    .cloned()
                    .unwrap(),
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
                let value = params["value"].as_str().unwrap_or_default().to_owned();
                on_update(AgentToolResult {
                    content: vec![TextOrImageContent::Text(TextContent::new("partial"))],
                    details: Some(json!({"status": "partial"})),
                    usage: None,
                    added_tool_names: None,
                    is_error: None,
                    terminate: None,
                });
                Box::pin(async move { Ok(text_result(&format!("echoed: {value}"))) })
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
                })
            }));
            agent.continue_run(ContinueOptions::default()).await.unwrap();
            agent_output(&agent, events.lock().unwrap().clone(), &requests)
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
    let mut oracle = oracle::TsOracle::spawn("agent-loop-oracle.ts", "packages/agent/src/agent-loop.ts");
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
        "agent_text",
        "agent_continue_queue",
    ];
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
