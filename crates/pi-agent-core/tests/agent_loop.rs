pub mod support;

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use pi_agent_core::agent::{
    AfterToolCallResult, AgentContext, AgentEvent, AgentLoopConfig, AgentMessage, AgentToolResult,
};
use pi_agent_core::agent::{agent_loop, agent_loop_continue};
use pi_agent_core::harness::BeforeToolCallResult;
use pi_agent_core::harness::{ThinkingLevel, ToolExecutionMode};
use pi_ai::types::{
    AssistantContent, AssistantMessageEvent, StopReason, TextContent, TextOrImageContent, ToolCallExecution,
    UserContent,
};
use pi_ai::utils::event_stream::create_assistant_message_event_stream;
use pi_js_compat::{
    js_json as json,
    js_json::{JsString, JsValue},
};
use support::*;

fn context(messages: Vec<AgentMessage>, tools: Vec<Arc<dyn pi_agent_core::agent::AgentTool>>) -> AgentContext {
    AgentContext {
        system_prompt: JsString::default(),
        messages,
        tools: Some(tools),
    }
}

async fn collect_events_allowing_failure(
    mut stream: pi_ai::utils::event_stream::EventStream<AgentEvent, Vec<AgentMessage>>,
) -> (Vec<AgentEvent>, Option<JsString>) {
    let mut events = Vec::new();
    let mut failure = None;
    while let Some(event) = stream.next().await {
        match event {
            Ok(event) => events.push(event),
            Err(error) => failure = Some(error.message().to_owned()),
        }
    }
    (events, failure)
}

fn fixture_string(value: &JsValue) -> String {
    value
        .as_string()
        .expect("string fixture")
        .to_utf8()
        .expect("scalar fixture")
}

fn config() -> AgentLoopConfig {
    AgentLoopConfig::new(model(), identity_convert())
}

fn agent_message(message: pi_ai::types::UserMessage) -> AgentMessage {
    AgentMessage::User(message)
}

#[tokio::test]
async fn does_not_resume_tools_from_failed_restored_assistants() {
    for reason in [StopReason::Error, StopReason::Aborted] {
        let mut restored = tool_use(vec![AssistantContent::ToolCall(tool_call(
            "pending",
            "lookup",
            json!({}),
        ))]);
        restored.stop_reason = reason;
        let tool = TestTool::no_args("lookup", || panic!("failed assistant tools must not execute"));
        let stream = agent_loop_continue(
            context(vec![restored.into()], vec![Arc::new(tool)]),
            config(),
            None,
            Some(scripted_stream_fn(vec![Script::message(text_assistant("recovered"))])),
        )
        .unwrap();
        let (events, result) = collect_events(stream).await;
        assert!(result.is_ok());
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, AgentEvent::ToolExecutionStart { .. }))
        );
    }
}

#[tokio::test]
async fn emits_events_with_agent_message_types() {
    let stream = agent_loop(
        vec![agent_message(user("Hello"))],
        context(Vec::new(), Vec::new()),
        config(),
        None,
        Some(scripted_stream_fn(vec![Script::message(text_assistant("Hi there!"))])),
    );
    let (events, result) = collect_events(stream).await;
    let messages = result.unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].role(), "user");
    assert_eq!(messages[1].role(), "assistant");
    let types = event_types(&events);
    for expected in [
        "agent_start",
        "turn_start",
        "message_start",
        "message_end",
        "turn_end",
        "agent_end",
    ] {
        assert!(types.contains(&expected), "missing {expected} in {types:?}");
    }
}

#[tokio::test]
async fn terminalizes_when_stream_acquisition_ignores_abort() {
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let started_tx = Arc::new(Mutex::new(Some(started_tx)));
    let stream_fn: pi_agent_core::agent::StreamFn = Arc::new(move |_model, _context, _options| {
        let started_tx = Arc::clone(&started_tx);
        Box::pin(async move {
            if let Some(sender) = started_tx.lock().unwrap().take() {
                let _ = sender.send(());
            }
            std::future::pending().await
        })
    });
    let controller_signal = pi_js_compat::abort::AbortController::new();
    let signal = controller_signal.signal();
    let stream = agent_loop(
        vec![agent_message(user("Hello"))],
        context(Vec::new(), Vec::new()),
        config(),
        Some(signal.clone()),
        Some(stream_fn),
    );
    started_rx.await.unwrap();
    controller_signal.abort(None);
    let error = stream.result().await.expect_err("stream must fail");
    assert_eq!(error.message(), "Agent run aborted");
}

#[tokio::test]
async fn terminalizes_when_stream_iteration_ignores_abort() {
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let started_tx = Arc::new(Mutex::new(Some(started_tx)));
    let stream_fn: pi_agent_core::agent::StreamFn = Arc::new(move |_model, _context, _options| {
        let started_tx = Arc::clone(&started_tx);
        Box::pin(async move {
            if let Some(sender) = started_tx.lock().unwrap().take() {
                let _ = sender.send(());
            }
            let (sender, stream) = create_assistant_message_event_stream();
            std::mem::forget(sender);
            Ok(stream)
        })
    });
    let controller_signal = pi_js_compat::abort::AbortController::new();
    let signal = controller_signal.signal();
    let stream = agent_loop(
        vec![agent_message(user("Hello"))],
        context(Vec::new(), Vec::new()),
        config(),
        Some(signal.clone()),
        Some(stream_fn),
    );
    started_rx.await.unwrap();
    controller_signal.abort(None);
    let error = stream.result().await.expect_err("stream must fail");
    assert_eq!(error.message(), "Agent run aborted");
}

#[tokio::test]
async fn resolves_naturally_exhausted_stream() {
    let response = text_assistant("done");
    let stream = agent_loop(
        vec![agent_message(user("Hello"))],
        context(Vec::new(), Vec::new()),
        config(),
        None,
        Some(scripted_stream_fn(vec![Script::message(response.clone())])),
    );
    let (_, result) = collect_events(stream).await;
    let messages = result.unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[1], AgentMessage::Assistant(Box::new(response)));
}

#[tokio::test]
async fn preserves_next_rejection_without_cleanup() {
    let stream_fn: pi_agent_core::agent::StreamFn = Arc::new(|model, _context, _options| {
        let model = model.clone();
        Box::pin(async move {
            let (sender, stream) = create_assistant_message_event_stream();
            let _ = model;
            sender.fail(pi_ai::utils::event_stream::StreamFailure::new("next failed"));
            Ok(stream)
        })
    });
    let stream = agent_loop(
        vec![agent_message(user("Hello"))],
        context(Vec::new(), Vec::new()),
        config(),
        None,
        Some(stream_fn),
    );
    let error = stream.result().await.expect_err("stream must fail");
    assert_eq!(error.message(), "next failed");
}

#[tokio::test]
async fn handles_custom_message_types_via_convert_to_llm() {
    let notification = AgentMessage::Custom(
        json!({"role": "notification", "text": "This is a notification", "timestamp": 1000})
            .as_object()
            .cloned()
            .unwrap(),
    );
    let converted = Arc::new(Mutex::new(Vec::new()));
    let converted_hook = Arc::clone(&converted);
    let mut cfg = config();
    cfg.convert_to_llm = Arc::new(move |messages| {
        let converted_hook = Arc::clone(&converted_hook);
        Box::pin(async move {
            let messages: Vec<pi_ai::types::Message> = messages
                .into_iter()
                .filter_map(|message| match message {
                    AgentMessage::User(message) => Some(pi_ai::types::Message::User(message)),
                    AgentMessage::Assistant(message) => Some(pi_ai::types::Message::Assistant(message)),
                    AgentMessage::ToolResult(message) => Some(pi_ai::types::Message::ToolResult(message)),
                    AgentMessage::Custom(_) => None,
                })
                .collect();
            *converted_hook.lock().unwrap() = messages.clone();
            Ok(messages)
        })
    });
    let stream = agent_loop(
        vec![agent_message(user("Hello"))],
        context(vec![notification], Vec::new()),
        cfg,
        None,
        Some(scripted_stream_fn(vec![Script::message(text_assistant("Response"))])),
    );
    let (_, result) = collect_events(stream).await;
    result.unwrap();
    let converted = converted.lock().unwrap();
    assert_eq!(converted.len(), 1);
    assert!(matches!(converted[0], pi_ai::types::Message::User(_)));
}

#[tokio::test]
async fn applies_transform_context_before_convert_to_llm() {
    let mut cfg = config();
    let transformed_len = Arc::new(AtomicUsize::new(0));
    let converted_len = Arc::new(AtomicUsize::new(0));
    let transformed_hook = Arc::clone(&transformed_len);
    let converted_hook = Arc::clone(&converted_len);
    cfg.transform_context = Some(Arc::new(move |messages, _signal| {
        let transformed_hook = Arc::clone(&transformed_hook);
        Box::pin(async move {
            let pruned: Vec<AgentMessage> = messages
                .into_iter()
                .rev()
                .take(2)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            transformed_hook.store(pruned.len(), Ordering::SeqCst);
            Ok(pruned)
        })
    }));
    cfg.convert_to_llm = Arc::new(move |messages| {
        let converted_hook = Arc::clone(&converted_hook);
        Box::pin(async move {
            converted_hook.store(messages.len(), Ordering::SeqCst);
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
    let existing = vec![
        agent_message(user("old message 1")),
        AgentMessage::Assistant(Box::new(text_assistant("old response 1"))),
        agent_message(user("old message 2")),
        AgentMessage::Assistant(Box::new(text_assistant("old response 2"))),
    ];
    let stream = agent_loop(
        vec![agent_message(user("new message"))],
        context(existing, Vec::new()),
        cfg,
        None,
        Some(scripted_stream_fn(vec![Script::message(text_assistant("Response"))])),
    );
    collect_events(stream).await.1.unwrap();
    assert_eq!(transformed_len.load(Ordering::SeqCst), 2);
    assert_eq!(converted_len.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn handles_tool_calls_and_results() {
    let executed = Arc::new(Mutex::new(Vec::<String>::new()));
    let executed_hook = Arc::clone(&executed);
    let started_at = Arc::new(Mutex::new(None::<f64>));
    let started_hook = Arc::clone(&started_at);
    let mut tool = TestTool::new("echo", move |_id, params| {
        let value = fixture_string(&params["value"]).to_owned();
        executed_hook.lock().unwrap().push(value.clone());
        text_result(format!("echoed: {value}"))
    });
    tool.execute = {
        let started_hook = Arc::clone(&started_hook);
        let executed_hook = Arc::clone(&executed);
        Arc::new(move |_id, params, _signal, _on_update| {
            let value = fixture_string(&params["value"]).to_owned();
            executed_hook.lock().unwrap().push(value.clone());
            let started_hook = Arc::clone(&started_hook);
            Box::pin(async move {
                *started_hook.lock().unwrap() = Some(42.0);
                Ok(text_result(format!("echoed: {value}")))
            })
        })
    };
    let stream = agent_loop(
        vec![agent_message(user("echo something"))],
        context(Vec::new(), vec![Arc::new(tool)]),
        config(),
        None,
        Some(scripted_stream_fn(vec![
            Script::message(tool_use(vec![AssistantContent::ToolCall(tool_call(
                "tool-1",
                "echo",
                json!({"value": "hello"}),
            ))])),
            Script::message(text_assistant("done")),
        ])),
    );
    let (events, result) = collect_events(stream).await;
    result.unwrap();
    assert_eq!(*executed.lock().unwrap(), vec!["hello".to_owned()]);
    let start = events
        .iter()
        .find_map(|event| match event {
            AgentEvent::ToolExecutionStart { started_at, .. } => Some(*started_at),
            _ => None,
        })
        .expect("tool_execution_start");
    let end = events
        .iter()
        .find_map(|event| match event {
            AgentEvent::ToolExecutionEnd {
                is_error,
                started_at: end_started,
                finished_at,
                ..
            } => Some((*is_error, *end_started, *finished_at)),
            _ => None,
        })
        .expect("tool_execution_end");
    assert!(!end.0);
    assert_eq!(end.1, start);
    assert!(end.2 >= end.1);
}

#[tokio::test]
async fn marks_returned_tool_results_with_is_error_as_failed() {
    let mut tool = TestTool::new("echo", |_id, params| {
        let value = fixture_string(&params["value"]);
        AgentToolResult {
            is_error: Some(true),
            ..text_result(format!("failed: {value}"))
        }
    });
    tool.parameters = serde_json::json!({"type": "object", "properties": {"value": {"type": "string"}}});
    let stream = agent_loop(
        vec![agent_message(user("echo something"))],
        context(Vec::new(), vec![Arc::new(tool)]),
        config(),
        None,
        Some(scripted_stream_fn(vec![
            Script::message(tool_use(vec![AssistantContent::ToolCall(tool_call(
                "tool-1",
                "echo",
                json!({"value": "boom"}),
            ))])),
            Script::message(text_assistant("done")),
        ])),
    );
    let (events, _) = collect_events(stream).await;
    let end = events
        .iter()
        .find_map(|event| match event {
            AgentEvent::ToolExecutionEnd { is_error, result, .. } => Some((*is_error, result.clone())),
            _ => None,
        })
        .expect("tool_execution_end");
    assert!(end.0);
    assert_eq!(
        end.1.content,
        vec![TextOrImageContent::Text(TextContent::new("failed: boom"))]
    );
}

#[tokio::test]
async fn executes_mutated_before_tool_call_args_without_revalidation() {
    let executed = Arc::new(Mutex::new(Vec::<JsValue>::new()));
    let executed_hook = Arc::clone(&executed);
    let mut tool = TestTool::new("echo", move |_id, params| {
        executed_hook.lock().unwrap().push(params.clone());
        text_result("ok")
    });
    tool.parameters = serde_json::json!({"type": "object", "properties": {"value": {"type": "string"}}});
    let mut cfg = config();
    cfg.before_tool_call = Some(Arc::new(|ctx, _signal| {
        Box::pin(async move {
            ctx.set_args(json!({"value": 123}));
            Ok(None)
        })
    }));
    let stream = agent_loop(
        vec![agent_message(user("echo"))],
        context(Vec::new(), vec![Arc::new(tool)]),
        cfg,
        None,
        Some(scripted_stream_fn(vec![
            Script::message(tool_use(vec![AssistantContent::ToolCall(tool_call(
                "tool-1",
                "echo",
                json!({"value": "hello"}),
            ))])),
            Script::message(text_assistant("done")),
        ])),
    );
    let (events, _) = collect_events(stream).await;
    assert_eq!(*executed.lock().unwrap(), vec![json!({"value": 123})]);
    let start_args = events
        .iter()
        .find_map(|event| match event {
            AgentEvent::ToolExecutionStart { args, .. } => Some(args.clone()),
            _ => None,
        })
        .expect("tool_execution_start");
    assert_eq!(start_args, json!({"value": 123}));
}

#[tokio::test]
async fn prepares_tool_arguments_for_validation() {
    let executed = Arc::new(Mutex::new(Vec::<JsValue>::new()));
    let executed_hook = Arc::clone(&executed);
    let mut tool = TestTool::new("edit", move |_id, params| {
        executed_hook.lock().unwrap().push(params.clone());
        text_result("edited")
    });
    tool.parameters = serde_json::json!({
        "type": "object",
        "properties": {
            "edits": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {"oldText": {"type": "string"}, "newText": {"type": "string"}},
                    "required": ["oldText", "newText"]
                }
            }
        },
        "required": ["edits"]
    });
    tool.prepare_arguments = Some(Arc::new(|args: JsValue| {
        let Some(object) = args.as_object() else {
            return Ok(args);
        };
        let (Some(old_text), Some(new_text)) = (
            object.get("oldText").and_then(JsValue::as_string),
            object.get("newText").and_then(JsValue::as_string),
        ) else {
            return Ok(args);
        };
        let mut edits = object
            .get("edits")
            .and_then(|value| value.as_array())
            .cloned()
            .unwrap_or_default();
        edits.push(json!({"oldText": old_text, "newText": new_text}));
        Ok(json!({"edits": edits}))
    }));
    let stream = agent_loop(
        vec![agent_message(user("edit something"))],
        context(Vec::new(), vec![Arc::new(tool)]),
        config(),
        None,
        Some(scripted_stream_fn(vec![
            Script::message(tool_use(vec![AssistantContent::ToolCall(tool_call(
                "tool-1",
                "edit",
                json!({"oldText": "before", "newText": "after"}),
            ))])),
            Script::message(text_assistant("done")),
        ])),
    );
    collect_events(stream).await.1.unwrap();
    assert_eq!(
        *executed.lock().unwrap(),
        vec![json!({"edits": [{"oldText": "before", "newText": "after"}]})]
    );
}

#[tokio::test]
async fn prepare_arguments_errors_become_error_tool_results_and_the_run_continues() {
    let executed = Arc::new(AtomicBool::new(false));
    let executed_hook = Arc::clone(&executed);
    let mut tool = TestTool::new("edit", move |_id, _params| {
        executed_hook.store(true, Ordering::SeqCst);
        text_result("edited")
    });
    tool.parameters = serde_json::json!({"type": "object"});
    tool.prepare_arguments = Some(Arc::new(|_args: JsValue| {
        Err(pi_agent_core::agent::ToolError::new("prepareArguments exploded"))
    }));
    let stream = agent_loop(
        vec![agent_message(user("edit something"))],
        context(Vec::new(), vec![Arc::new(tool)]),
        config(),
        None,
        Some(scripted_stream_fn(vec![
            Script::message(tool_use(vec![AssistantContent::ToolCall(tool_call(
                "tool-1",
                "edit",
                json!({"value": 1}),
            ))])),
            Script::message(text_assistant("done")),
        ])),
    );
    let (_, result) = collect_events(stream).await;
    let messages = result.unwrap();
    let tool_result = messages
        .iter()
        .find_map(|message| message.as_tool_result())
        .expect("tool result");
    assert!(!executed.load(Ordering::SeqCst));
    assert!(tool_result.is_error);
    assert_eq!(
        tool_result.content,
        vec![TextOrImageContent::Text(TextContent::new("prepareArguments exploded"))]
    );
    assert_eq!(messages.iter().filter_map(|message| message.as_assistant()).count(), 2);
}

#[tokio::test]
async fn prepare_execution_errors_abort_the_run() {
    let mut tool = TestTool::new("echo", |_id, _params| text_result("ok"));
    tool.parameters = serde_json::json!({"type": "object", "properties": {"value": {"type": "string"}}});
    tool.prepare_execution = Some(Arc::new(|_id, _signal| {
        Box::pin(async { Err(pi_agent_core::agent::ToolError::new("prepareExecution exploded")) })
    }));
    let stream = agent_loop(
        vec![agent_message(user("echo"))],
        context(Vec::new(), vec![Arc::new(tool)]),
        config(),
        None,
        Some(scripted_stream_fn(vec![Script::message(tool_use(vec![
            AssistantContent::ToolCall(tool_call("tool-1", "echo", json!({"value": "x"}))),
        ]))])),
    );
    let (_, failure) = collect_events_allowing_failure(stream).await;
    assert_eq!(failure.as_ref(), Some(&JsString::from("prepareExecution exploded")));
}

#[tokio::test]
async fn persists_prepared_agent_id_and_passes_it_to_execution() {
    let executed_agent_ids = Arc::new(Mutex::new(Vec::<String>::new()));
    let executed_hook = Arc::clone(&executed_agent_ids);
    let preparation_calls = Arc::new(AtomicUsize::new(0));
    let preparation_hook = Arc::clone(&preparation_calls);
    let mut tool = TestTool::new("echo", |_id, params| text_result(fixture_string(&params["value"])));
    tool.parameters = serde_json::json!({"type": "object", "properties": {"value": {"type": "string"}}});
    tool.prepare_execution = Some(Arc::new(move |_id, _signal| {
        preparation_hook.fetch_add(1, Ordering::SeqCst);
        Box::pin(async {
            Ok(Some(ToolCallExecution {
                agent_id: "known-agent".to_owned(),
            }))
        })
    }));
    tool.execute = Arc::new(move |_id, _params, _signal, _on_update| {
        let executed_hook = Arc::clone(&executed_hook);
        Box::pin(async move {
            executed_hook.lock().unwrap().push("known-agent".to_owned());
            Ok(text_result("ok"))
        })
    });
    let mut cfg = config();
    cfg.model = model();
    let stream = agent_loop(
        vec![agent_message(user("echo"))],
        context(Vec::new(), vec![Arc::new(tool)]),
        cfg,
        None,
        Some(scripted_stream_fn(vec![
            Script::message(tool_use(vec![AssistantContent::ToolCall(tool_call(
                "provider-call",
                "echo",
                json!({"value": "hello"}),
            ))])),
            Script::message(text_assistant("done")),
        ])),
    );
    let (events, result) = collect_events(stream).await;
    let messages = result.unwrap();
    assert!(preparation_calls.load(Ordering::SeqCst) >= 1);
    let persisted = messages
        .iter()
        .filter_map(|message| message.as_assistant())
        .flat_map(|message| message.content.iter())
        .find_map(|content| match content {
            AssistantContent::ToolCall(call) => call.execution.clone(),
            _ => None,
        })
        .expect("persisted execution");
    assert_eq!(persisted.agent_id, "known-agent");
    assert!(events.iter().any(|event| matches!(
        event,
        AgentEvent::MessageEnd { message } if message.as_assistant().is_some_and(|assistant| assistant
            .content
            .iter()
            .any(|content| matches!(content, AssistantContent::ToolCall(call) if call.execution.is_some())))
    )));
}

#[tokio::test]
async fn emits_tool_execution_end_in_completion_order_and_results_in_source_order() {
    let first_resolved = Arc::new(AtomicBool::new(false));
    let parallel_observed = Arc::new(AtomicBool::new(false));
    let release_first = Arc::new(tokio::sync::Notify::new());
    let mut tool = TestTool::new("echo", |_id, _params| text_result("unused"));
    tool.parameters = serde_json::json!({"type": "object", "properties": {"value": {"type": "string"}}});
    tool.execute = {
        let first_resolved = Arc::clone(&first_resolved);
        let parallel_observed = Arc::clone(&parallel_observed);
        let release_first = Arc::clone(&release_first);
        Arc::new(move |_id, params, _signal, _on_update| {
            let value = fixture_string(&params["value"]).to_owned();
            let first_resolved = Arc::clone(&first_resolved);
            let parallel_observed = Arc::clone(&parallel_observed);
            let release_first = Arc::clone(&release_first);
            Box::pin(async move {
                if value == "first" {
                    release_first.notified().await;
                    first_resolved.store(true, Ordering::SeqCst);
                }
                if value == "second" && !first_resolved.load(Ordering::SeqCst) {
                    parallel_observed.store(true, Ordering::SeqCst);
                }
                Ok(text_result(format!("echoed: {value}")))
            })
        })
    };
    let release = Arc::clone(&release_first);
    let stream_fn: pi_agent_core::agent::StreamFn = {
        let scripts = Arc::new(Mutex::new(0));
        Arc::new(move |_model, _context, _options| {
            let mut call = scripts.lock().unwrap();
            *call += 1;
            let is_first = *call == 1;
            let release = Arc::clone(&release);
            Box::pin(async move {
                let (sender, stream) = create_assistant_message_event_stream();
                if is_first {
                    sender.push(AssistantMessageEvent::Done {
                        reason: pi_ai::types::DoneReason::ToolUse,
                        message: tool_use(vec![
                            AssistantContent::ToolCall(tool_call("tool-1", "echo", json!({"value": "first"}))),
                            AssistantContent::ToolCall(tool_call("tool-2", "echo", json!({"value": "second"}))),
                        ]),
                    });
                    tokio::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                        release.notify_waiters();
                    });
                } else {
                    sender.push(AssistantMessageEvent::Done {
                        reason: pi_ai::types::DoneReason::Stop,
                        message: text_assistant("done"),
                    });
                }
                Ok(stream)
            })
        })
    };
    let stream = agent_loop(
        vec![agent_message(user("echo both"))],
        context(Vec::new(), vec![Arc::new(tool)]),
        config(),
        None,
        Some(stream_fn),
    );
    let (events, _) = collect_events(stream).await;
    let execution_end_ids: Vec<&str> = events
        .iter()
        .filter_map(|event| match event {
            AgentEvent::ToolExecutionEnd { tool_call_id, .. } => Some(tool_call_id.as_str()),
            _ => None,
        })
        .collect();
    let result_ids: Vec<&str> = events
        .iter()
        .filter_map(|event| match event {
            AgentEvent::MessageEnd { message } => message.as_tool_result().map(|result| result.tool_call_id.as_str()),
            _ => None,
        })
        .collect();
    let turn_ids: Vec<&str> = events
        .iter()
        .filter_map(|event| match event {
            AgentEvent::TurnEnd { tool_results, .. } => Some(tool_results),
            _ => None,
        })
        .flatten()
        .map(|result| result.tool_call_id.as_str())
        .collect();
    assert!(parallel_observed.load(Ordering::SeqCst));
    assert_eq!(execution_end_ids, vec!["tool-2", "tool-1"]);
    assert_eq!(result_ids, vec!["tool-1", "tool-2"]);
    assert_eq!(turn_ids, vec!["tool-1", "tool-2"]);
}

#[tokio::test]
async fn injects_queued_messages_after_all_tool_calls_complete() {
    let executed = Arc::new(Mutex::new(Vec::<String>::new()));
    let executed_hook = Arc::clone(&executed);
    let saw_interrupt = Arc::new(AtomicBool::new(false));
    let saw_interrupt_hook = Arc::clone(&saw_interrupt);
    let mut tool = TestTool::new("echo", move |_id, params| {
        let value = fixture_string(&params["value"]).to_owned();
        executed_hook.lock().unwrap().push(value.clone());
        text_result(format!("ok:{value}"))
    });
    tool.parameters = serde_json::json!({"type": "object", "properties": {"value": {"type": "string"}}});
    let polls = Arc::new(AtomicUsize::new(0));
    let polls_hook = Arc::clone(&polls);
    let mut cfg = config();
    cfg.get_steering_messages = Some(Arc::new(move || {
        let polls = Arc::clone(&polls_hook);
        Box::pin(async move {
            let poll = polls.fetch_add(1, Ordering::SeqCst);
            if poll == 1 {
                Ok(vec![agent_message(user("interrupt"))])
            } else {
                Ok(Vec::new())
            }
        })
    }));
    let stream_fn: pi_agent_core::agent::StreamFn = {
        let saw_interrupt_hook = Arc::clone(&saw_interrupt_hook);
        let calls = Arc::new(AtomicUsize::new(0));
        Arc::new(move |_model, request_context, _options| {
            let call = calls.fetch_add(1, Ordering::SeqCst);
            if call > 0
                && request_context.messages.iter().any(|message| match message {
                    pi_ai::types::Message::User(user) => {
                        matches!(&user.content, UserContent::Text(text) if text == "interrupt")
                    }
                    _ => false,
                })
            {
                saw_interrupt_hook.store(true, Ordering::SeqCst);
            }
            Box::pin(async move {
                let (sender, stream) = create_assistant_message_event_stream();
                let message = if call == 0 {
                    tool_use(vec![
                        AssistantContent::ToolCall(tool_call("tool-1", "echo", json!({"value": "first"}))),
                        AssistantContent::ToolCall(tool_call("tool-2", "echo", json!({"value": "second"}))),
                    ])
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
    };
    let stream = agent_loop(
        vec![agent_message(user("echo both"))],
        context(Vec::new(), vec![Arc::new(tool)]),
        cfg,
        None,
        Some(stream_fn),
    );
    let (events, _) = collect_events(stream).await;
    assert_eq!(*executed.lock().unwrap(), vec!["first".to_owned(), "second".to_owned()]);
    let sequence: Vec<JsString> = events
        .iter()
        .filter_map(|event| match event {
            AgentEvent::MessageStart { message } => match message {
                AgentMessage::ToolResult(result) => Some(format!("tool:{}", result.tool_call_id).into()),
                AgentMessage::User(user) => match &user.content {
                    UserContent::Text(text) => Some(text.clone()),
                    UserContent::Blocks(_) => None,
                },
                _ => None,
            },
            _ => None,
        })
        .collect();
    let interrupt_index = sequence.iter().position(|item| item == "interrupt").expect("interrupt");
    assert!(sequence.iter().position(|item| item == "tool:tool-1").unwrap() < interrupt_index);
    assert!(sequence.iter().position(|item| item == "tool:tool-2").unwrap() < interrupt_index);
    assert!(saw_interrupt.load(Ordering::SeqCst));
}

fn echo_tool_with_mode(execution_mode: Option<ToolExecutionMode>) -> TestTool {
    let mut tool = TestTool::new("echo", |_id, params| {
        text_result(format!("echoed: {}", fixture_string(&params["value"])))
    });
    tool.parameters = serde_json::json!({"type": "object", "properties": {"value": {"type": "string"}}});
    tool.execution_mode = execution_mode;
    tool
}

#[tokio::test]
async fn forces_sequential_when_tool_declares_sequential_mode() {
    let mut tool = echo_tool_with_mode(Some(ToolExecutionMode::Sequential));
    let first_resolved = Arc::new(AtomicBool::new(false));
    let parallel_observed = Arc::new(AtomicBool::new(false));
    let release_first = Arc::new(tokio::sync::Notify::new());
    tool.execute = {
        let first_resolved = Arc::clone(&first_resolved);
        let parallel_observed = Arc::clone(&parallel_observed);
        let release_first = Arc::clone(&release_first);
        Arc::new(move |_id, params, _signal, _on_update| {
            let value = fixture_string(&params["value"]).to_owned();
            let first_resolved = Arc::clone(&first_resolved);
            let parallel_observed = Arc::clone(&parallel_observed);
            let release_first = Arc::clone(&release_first);
            Box::pin(async move {
                if value == "first" {
                    release_first.notified().await;
                    first_resolved.store(true, Ordering::SeqCst);
                }
                if value == "second" && !first_resolved.load(Ordering::SeqCst) {
                    parallel_observed.store(true, Ordering::SeqCst);
                }
                Ok(text_result(format!("slow: {value}")))
            })
        })
    };
    let release = Arc::clone(&release_first);
    let stream_fn: pi_agent_core::agent::StreamFn = {
        let calls = Arc::new(AtomicUsize::new(0));
        Arc::new(move |_model, _context, _options| {
            let call = calls.fetch_add(1, Ordering::SeqCst);
            let release = Arc::clone(&release);
            Box::pin(async move {
                let (sender, stream) = create_assistant_message_event_stream();
                if call == 0 {
                    sender.push(AssistantMessageEvent::Done {
                        reason: pi_ai::types::DoneReason::ToolUse,
                        message: tool_use(vec![
                            AssistantContent::ToolCall(tool_call("tool-1", "echo", json!({"value": "first"}))),
                            AssistantContent::ToolCall(tool_call("tool-2", "echo", json!({"value": "second"}))),
                        ]),
                    });
                    tokio::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                        release.notify_waiters();
                    });
                } else {
                    sender.push(AssistantMessageEvent::Done {
                        reason: pi_ai::types::DoneReason::Stop,
                        message: text_assistant("done"),
                    });
                }
                Ok(stream)
            })
        })
    };
    let stream = agent_loop(
        vec![agent_message(user("run both"))],
        context(Vec::new(), vec![Arc::new(tool)]),
        config(),
        None,
        Some(stream_fn),
    );
    let (events, _) = collect_events(stream).await;
    assert!(!parallel_observed.load(Ordering::SeqCst));
    let result_ids: Vec<&str> = events
        .iter()
        .filter_map(|event| match event {
            AgentEvent::MessageEnd { message } => message.as_tool_result().map(|result| result.tool_call_id.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(result_ids, vec!["tool-1", "tool-2"]);
}

#[tokio::test]
async fn forces_sequential_when_one_of_multiple_tools_is_sequential() {
    let execution_order = Arc::new(Mutex::new(Vec::<String>::new()));
    let release_slow = Arc::new(tokio::sync::Notify::new());
    let mut slow = echo_tool_with_mode(Some(ToolExecutionMode::Sequential));
    slow.name = "slow".to_owned();
    slow.execute = {
        let execution_order = Arc::clone(&execution_order);
        let release_slow = Arc::clone(&release_slow);
        Arc::new(move |_id, params, _signal, _on_update| {
            let value = fixture_string(&params["value"]).to_owned();
            let execution_order = Arc::clone(&execution_order);
            let release_slow = Arc::clone(&release_slow);
            Box::pin(async move {
                execution_order.lock().unwrap().push(format!("slow:{value}"));
                if value == "a" {
                    release_slow.notified().await;
                }
                Ok(text_result(format!("slow: {value}")))
            })
        })
    };
    let mut fast = echo_tool_with_mode(None);
    fast.name = "fast".to_owned();
    fast.execute = {
        let execution_order = Arc::clone(&execution_order);
        Arc::new(move |_id, params, _signal, _on_update| {
            let value = fixture_string(&params["value"]).to_owned();
            let execution_order = Arc::clone(&execution_order);
            Box::pin(async move {
                execution_order.lock().unwrap().push(format!("fast:{value}"));
                Ok(text_result(format!("fast: {value}")))
            })
        })
    };
    let release = Arc::clone(&release_slow);
    let stream_fn: pi_agent_core::agent::StreamFn = {
        let calls = Arc::new(AtomicUsize::new(0));
        Arc::new(move |_model, _context, _options| {
            let call = calls.fetch_add(1, Ordering::SeqCst);
            let release = Arc::clone(&release);
            Box::pin(async move {
                let (sender, stream) = create_assistant_message_event_stream();
                if call == 0 {
                    sender.push(AssistantMessageEvent::Done {
                        reason: pi_ai::types::DoneReason::ToolUse,
                        message: tool_use(vec![
                            AssistantContent::ToolCall(tool_call("tool-1", "slow", json!({"value": "a"}))),
                            AssistantContent::ToolCall(tool_call("tool-2", "fast", json!({"value": "b"}))),
                        ]),
                    });
                    tokio::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                        release.notify_waiters();
                    });
                } else {
                    sender.push(AssistantMessageEvent::Done {
                        reason: pi_ai::types::DoneReason::Stop,
                        message: text_assistant("done"),
                    });
                }
                Ok(stream)
            })
        })
    };
    let stream = agent_loop(
        vec![agent_message(user("run both"))],
        context(Vec::new(), vec![Arc::new(slow), Arc::new(fast)]),
        config(),
        None,
        Some(stream_fn),
    );
    collect_events(stream).await.1.unwrap();
    assert_eq!(
        execution_order.lock().unwrap().first().map(String::as_str),
        Some("slow:a")
    );
    assert!(execution_order.lock().unwrap().iter().any(|entry| entry == "fast:b"));
}

#[tokio::test]
async fn allows_parallel_execution_when_all_tools_declare_parallel() {
    let first_resolved = Arc::new(AtomicBool::new(false));
    let parallel_observed = Arc::new(AtomicBool::new(false));
    let release_first = Arc::new(tokio::sync::Notify::new());
    let mut tool = echo_tool_with_mode(Some(ToolExecutionMode::Parallel));
    tool.execute = {
        let first_resolved = Arc::clone(&first_resolved);
        let parallel_observed = Arc::clone(&parallel_observed);
        let release_first = Arc::clone(&release_first);
        Arc::new(move |_id, params, _signal, _on_update| {
            let value = fixture_string(&params["value"]).to_owned();
            let first_resolved = Arc::clone(&first_resolved);
            let parallel_observed = Arc::clone(&parallel_observed);
            let release_first = Arc::clone(&release_first);
            Box::pin(async move {
                if value == "first" {
                    release_first.notified().await;
                    first_resolved.store(true, Ordering::SeqCst);
                }
                if value == "second" && !first_resolved.load(Ordering::SeqCst) {
                    parallel_observed.store(true, Ordering::SeqCst);
                }
                Ok(text_result(format!("echoed: {value}")))
            })
        })
    };
    let release = Arc::clone(&release_first);
    let stream_fn: pi_agent_core::agent::StreamFn = {
        let calls = Arc::new(AtomicUsize::new(0));
        Arc::new(move |_model, _context, _options| {
            let call = calls.fetch_add(1, Ordering::SeqCst);
            let release = Arc::clone(&release);
            Box::pin(async move {
                let (sender, stream) = create_assistant_message_event_stream();
                if call == 0 {
                    sender.push(AssistantMessageEvent::Done {
                        reason: pi_ai::types::DoneReason::ToolUse,
                        message: tool_use(vec![
                            AssistantContent::ToolCall(tool_call("tool-1", "echo", json!({"value": "first"}))),
                            AssistantContent::ToolCall(tool_call("tool-2", "echo", json!({"value": "second"}))),
                        ]),
                    });
                    tokio::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                        release.notify_waiters();
                    });
                } else {
                    sender.push(AssistantMessageEvent::Done {
                        reason: pi_ai::types::DoneReason::Stop,
                        message: text_assistant("done"),
                    });
                }
                Ok(stream)
            })
        })
    };
    let stream = agent_loop(
        vec![agent_message(user("echo both"))],
        context(Vec::new(), vec![Arc::new(tool)]),
        config(),
        None,
        Some(stream_fn),
    );
    collect_events(stream).await.1.unwrap();
    assert!(parallel_observed.load(Ordering::SeqCst));
}

#[tokio::test]
async fn uses_prepare_next_turn_snapshot_before_continuing() {
    let mut tool = echo_tool_with_mode(None);
    tool.parameters = serde_json::json!({"type": "object", "properties": {"value": {"type": "string"}}});
    let prepared = Arc::new(AtomicBool::new(false));
    let prepared_hook = Arc::clone(&prepared);
    let mut cfg = config();
    cfg.prepare_next_turn = Some(Arc::new(move |turn_context| {
        let prepared_hook = Arc::clone(&prepared_hook);
        Box::pin(async move {
            if prepared_hook.swap(true, Ordering::SeqCst) {
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
    let system_prompts = Arc::new(Mutex::new(Vec::<Option<JsString>>::new()));
    let system_prompts_hook = Arc::clone(&system_prompts);
    let stream_fn: pi_agent_core::agent::StreamFn = {
        let calls = Arc::new(AtomicUsize::new(0));
        Arc::new(move |_model, request_context, _options| {
            let call = calls.fetch_add(1, Ordering::SeqCst);
            system_prompts_hook
                .lock()
                .unwrap()
                .push(request_context.system_prompt.clone());
            Box::pin(async move {
                let (sender, stream) = create_assistant_message_event_stream();
                if call == 0 {
                    sender.push(AssistantMessageEvent::Done {
                        reason: pi_ai::types::DoneReason::ToolUse,
                        message: tool_use(vec![AssistantContent::ToolCall(tool_call(
                            "tool-1",
                            "echo",
                            json!({"value": "hello"}),
                        ))]),
                    });
                } else {
                    sender.push(AssistantMessageEvent::Done {
                        reason: pi_ai::types::DoneReason::Stop,
                        message: text_assistant("done"),
                    });
                }
                Ok(stream)
            })
        })
    };
    let mut base = context(Vec::new(), vec![Arc::new(tool)]);
    base.system_prompt = JsString::from("first prompt");
    let stream = agent_loop(
        vec![agent_message(user("echo something"))],
        base,
        cfg,
        None,
        Some(stream_fn),
    );
    collect_events(stream).await.1.unwrap();
    let prompts = system_prompts.lock().unwrap();
    assert_eq!(prompts.len(), 2);
    assert_eq!(prompts[1].as_ref(), Some(&JsString::from("second prompt")));
}

#[tokio::test]
async fn stops_after_current_turn_when_should_stop_after_turn_returns_true() {
    let executed = Arc::new(Mutex::new(Vec::<String>::new()));
    let executed_hook = Arc::clone(&executed);
    let mut tool = echo_tool_with_mode(None);
    tool.execute = {
        let executed_hook = Arc::clone(&executed_hook);
        Arc::new(move |_id, params, _signal, _on_update| {
            let value = fixture_string(&params["value"]).to_owned();
            let executed_hook = Arc::clone(&executed_hook);
            Box::pin(async move {
                executed_hook.lock().unwrap().push(value.clone());
                Ok(text_result(format!("echoed: {value}")))
            })
        })
    };
    let steering_polls = Arc::new(AtomicUsize::new(0));
    let follow_up_polls = Arc::new(AtomicUsize::new(0));
    let callback_tool_result_ids = Arc::new(Mutex::new(Vec::<String>::new()));
    let callback_context_roles = Arc::new(Mutex::new(Vec::<JsString>::new()));
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
                Ok(vec![agent_message(user("follow up should stay queued"))])
            })
        }))
    };
    cfg.should_stop_after_turn = {
        let callback_tool_result_ids = Arc::clone(&callback_tool_result_ids);
        let callback_context_roles = Arc::clone(&callback_context_roles);
        Some(Arc::new(move |turn_context| {
            let callback_tool_result_ids = Arc::clone(&callback_tool_result_ids);
            let callback_context_roles = Arc::clone(&callback_context_roles);
            Box::pin(async move {
                assert_eq!(turn_context.message.stop_reason, StopReason::ToolUse);
                *callback_tool_result_ids.lock().unwrap() = turn_context
                    .tool_results
                    .iter()
                    .map(|result| result.tool_call_id.clone())
                    .collect();
                *callback_context_roles.lock().unwrap() = turn_context
                    .context
                    .messages
                    .iter()
                    .map(|message| message.role().to_owned())
                    .collect();
                Ok(true)
            })
        }))
    };
    let llm_calls = Arc::new(AtomicUsize::new(0));
    let stream_fn: pi_agent_core::agent::StreamFn = {
        let llm_calls = Arc::clone(&llm_calls);
        Arc::new(move |_model, _context, _options| {
            let call = llm_calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move {
                let (sender, stream) = create_assistant_message_event_stream();
                let message = if call == 0 {
                    tool_use(vec![AssistantContent::ToolCall(tool_call(
                        "tool-1",
                        "echo",
                        json!({"value": "hello"}),
                    ))])
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
    };
    let stream = agent_loop(
        vec![agent_message(user("echo something"))],
        context(Vec::new(), vec![Arc::new(tool)]),
        cfg,
        None,
        Some(stream_fn),
    );
    let (events, result) = collect_events(stream).await;
    let messages = result.unwrap();
    assert_eq!(llm_calls.load(Ordering::SeqCst), 1);
    assert_eq!(*executed.lock().unwrap(), vec!["hello".to_owned()]);
    assert_eq!(steering_polls.load(Ordering::SeqCst), 1);
    assert_eq!(follow_up_polls.load(Ordering::SeqCst), 0);
    assert_eq!(*callback_tool_result_ids.lock().unwrap(), vec!["tool-1".to_owned()]);
    assert_eq!(
        *callback_context_roles.lock().unwrap(),
        vec!["user".to_owned(), "assistant".to_owned(), "toolResult".to_owned()]
    );
    assert_eq!(
        messages.iter().map(|message| message.role()).collect::<Vec<_>>(),
        vec!["user", "assistant", "toolResult"]
    );
    assert_eq!(
        event_types(&events),
        vec![
            "agent_start",
            "turn_start",
            "message_start",
            "message_end",
            "model_request_start",
            "message_start",
            "message_end",
            "model_request_end",
            "tool_execution_start",
            "tool_execution_end",
            "message_start",
            "message_end",
            "turn_end",
            "agent_end",
        ]
    );
}

#[tokio::test]
async fn continues_after_assistant_text_until_terminating_tool_result() {
    let end_tool = TestTool::no_args("end_turn", || AgentToolResult {
        terminate: Some(true),
        ..text_result("Ended: Finished requested work")
    });
    let second_request_messages = Arc::new(Mutex::new(None::<Vec<pi_ai::types::Message>>));
    let second_hook = Arc::clone(&second_request_messages);
    let stream_fn: pi_agent_core::agent::StreamFn = {
        let calls = Arc::new(AtomicUsize::new(0));
        Arc::new(move |_model, request_context, _options| {
            let call = calls.fetch_add(1, Ordering::SeqCst);
            if call == 1 {
                *second_hook.lock().unwrap() = Some(request_context.messages.clone());
            }
            Box::pin(async move {
                let (sender, stream) = create_assistant_message_event_stream();
                let message = if call == 0 {
                    text_assistant("intermediate update")
                } else {
                    tool_use(vec![AssistantContent::ToolCall(tool_call(
                        "end-1",
                        "end_turn",
                        json!({}),
                    ))])
                };
                let reason = if call == 0 {
                    pi_ai::types::DoneReason::Stop
                } else {
                    pi_ai::types::DoneReason::ToolUse
                };
                sender.push(AssistantMessageEvent::Done { reason, message });
                Ok(stream)
            })
        })
    };
    let mut end_tool = end_tool;
    end_tool.parameters = serde_json::json!({"type": "object", "properties": {}});
    let stream = agent_loop(
        vec![agent_message(user("work"))],
        context(Vec::new(), vec![Arc::new(end_tool)]),
        config(),
        None,
        Some(stream_fn),
    );
    let (events, result) = collect_events(stream).await;
    let messages = result.unwrap();
    let instruction = "Your previous response was already delivered. Do not continue, repeat, or infer a new user request. Call `end_turn` now with a concise reason.";
    let second = second_request_messages.lock().unwrap().clone().expect("second request");
    assert!(second.iter().any(|message| match message {
        pi_ai::types::Message::User(user) => matches!(&user.content, UserContent::Text(text) if text == instruction),
        _ => false,
    }));
    assert_eq!(
        messages.iter().map(|message| message.role()).collect::<Vec<_>>(),
        vec!["user", "assistant", "assistant", "toolResult"]
    );
    assert!(!messages.iter().any(|message| match message {
        AgentMessage::User(user) => matches!(&user.content, UserContent::Text(text) if text == instruction),
        _ => false,
    }));
    assert!(!events.iter().any(|event| match event {
        AgentEvent::MessageStart {
            message: AgentMessage::User(user),
        } => matches!(&user.content, UserContent::Text(text) if text == instruction),
        _ => false,
    }));
}

#[tokio::test]
async fn stops_after_tool_batch_when_every_result_terminates() {
    let tool = TestTool::new("echo", |_id, params| AgentToolResult {
        terminate: Some(true),
        ..text_result(format!("echoed: {}", fixture_string(&params["value"])))
    });
    let calls = Arc::new(AtomicUsize::new(0));
    let stream_fn: pi_agent_core::agent::StreamFn = {
        let calls = Arc::clone(&calls);
        Arc::new(move |_model, _context, _options| {
            calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move {
                let (sender, stream) = create_assistant_message_event_stream();
                sender.push(AssistantMessageEvent::Done {
                    reason: pi_ai::types::DoneReason::ToolUse,
                    message: tool_use(vec![AssistantContent::ToolCall(tool_call(
                        "tool-1",
                        "echo",
                        json!({"value": "hello"}),
                    ))]),
                });
                Ok(stream)
            })
        })
    };
    let stream = agent_loop(
        vec![agent_message(user("echo something"))],
        context(Vec::new(), vec![Arc::new(tool)]),
        config(),
        None,
        Some(stream_fn),
    );
    let (events, result) = collect_events(stream).await;
    let messages = result.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        messages.iter().map(|message| message.role()).collect::<Vec<_>>(),
        vec!["user", "assistant", "toolResult"]
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, AgentEvent::TurnEnd { .. }))
            .count(),
        1
    );
}

#[tokio::test]
async fn continues_after_blocked_tool_call_without_termination_hint() {
    let executed = Arc::new(AtomicBool::new(false));
    let executed_hook = Arc::clone(&executed);
    let mut tool = TestTool::new("echo", move |_id, _params| {
        executed_hook.store(true, Ordering::SeqCst);
        text_result("should not execute")
    });
    tool.parameters = serde_json::json!({"type": "object", "properties": {"value": {"type": "string"}}});
    let mut cfg = config();
    cfg.before_tool_call = Some(Arc::new(|_ctx, _signal| {
        Box::pin(async move {
            Ok(Some(BeforeToolCallResult {
                block: Some(true),
                reason: Some("Blocked by policy".to_owned().into()),
            }))
        })
    }));
    let calls = Arc::new(AtomicUsize::new(0));
    let stream_fn: pi_agent_core::agent::StreamFn = {
        let calls = Arc::clone(&calls);
        Arc::new(move |_model, _context, _options| {
            let call = calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move {
                let (sender, stream) = create_assistant_message_event_stream();
                let message = if call == 0 {
                    tool_use(vec![AssistantContent::ToolCall(tool_call(
                        "tool-1",
                        "echo",
                        json!({"value": "hello"}),
                    ))])
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
    };
    let stream = agent_loop(
        vec![agent_message(user("echo something"))],
        context(Vec::new(), vec![Arc::new(tool)]),
        cfg,
        None,
        Some(stream_fn),
    );
    let (_, result) = collect_events(stream).await;
    let messages = result.unwrap();
    let tool_result = messages
        .iter()
        .find_map(|message| message.as_tool_result())
        .expect("tool result");
    assert!(!executed.load(Ordering::SeqCst));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert!(tool_result.is_error);
    assert_eq!(
        tool_result.content,
        vec![TextOrImageContent::Text(TextContent::new("Blocked by policy"))]
    );
}

#[tokio::test]
async fn continues_after_mixed_batch_with_one_terminating_blocked_call() {
    let executed = Arc::new(Mutex::new(Vec::<String>::new()));
    let executed_hook = Arc::clone(&executed);
    let mut tool = TestTool::new("echo", move |_id, params| {
        let value = fixture_string(&params["value"]).to_owned();
        executed_hook.lock().unwrap().push(value.clone());
        text_result(format!("echoed: {value}"))
    });
    tool.parameters = serde_json::json!({"type": "object", "properties": {"value": {"type": "string"}}});
    let mut cfg = config();
    cfg.before_tool_call = Some(Arc::new(|ctx, _signal| {
        Box::pin(async move {
            let args = ctx.args_snapshot();
            if args["value"] == json!("first") {
                Ok(Some(BeforeToolCallResult {
                    block: Some(true),
                    reason: Some("Blocked first".to_owned().into()),
                }))
            } else {
                Ok(None)
            }
        })
    }));
    let calls = Arc::new(AtomicUsize::new(0));
    let stream_fn: pi_agent_core::agent::StreamFn = {
        let calls = Arc::clone(&calls);
        Arc::new(move |_model, _context, _options| {
            let call = calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move {
                let (sender, stream) = create_assistant_message_event_stream();
                let message = if call == 0 {
                    tool_use(vec![
                        AssistantContent::ToolCall(tool_call("tool-1", "echo", json!({"value": "first"}))),
                        AssistantContent::ToolCall(tool_call("tool-2", "echo", json!({"value": "second"}))),
                    ])
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
    };
    let stream = agent_loop(
        vec![agent_message(user("echo both"))],
        context(Vec::new(), vec![Arc::new(tool)]),
        cfg,
        None,
        Some(stream_fn),
    );
    collect_events(stream).await.1.unwrap();
    assert_eq!(*executed.lock().unwrap(), vec!["second".to_owned()]);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn continues_after_parallel_tool_calls_when_not_all_results_terminate() {
    let mut tool = TestTool::new("echo", |_id, params| AgentToolResult {
        terminate: Some(params["value"] == json!("first")),
        ..text_result(format!("echoed: {}", fixture_string(&params["value"])))
    });
    tool.parameters = serde_json::json!({"type": "object", "properties": {"value": {"type": "string"}}});
    let calls = Arc::new(AtomicUsize::new(0));
    let stream_fn: pi_agent_core::agent::StreamFn = {
        let calls = Arc::clone(&calls);
        Arc::new(move |_model, _context, _options| {
            let call = calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move {
                let (sender, stream) = create_assistant_message_event_stream();
                let message = if call == 0 {
                    tool_use(vec![
                        AssistantContent::ToolCall(tool_call("tool-1", "echo", json!({"value": "first"}))),
                        AssistantContent::ToolCall(tool_call("tool-2", "echo", json!({"value": "second"}))),
                    ])
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
    };
    let stream = agent_loop(
        vec![agent_message(user("echo both"))],
        context(Vec::new(), vec![Arc::new(tool)]),
        config(),
        None,
        Some(stream_fn),
    );
    let (_, result) = collect_events(stream).await;
    let messages = result.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        messages.iter().map(|message| message.role()).collect::<Vec<_>>(),
        vec!["user", "assistant", "toolResult", "toolResult", "assistant"]
    );
}

#[tokio::test]
async fn after_tool_call_can_mark_batch_as_terminating() {
    let tool = TestTool::new("echo", |_id, _params| text_result("echoed"));
    let mut cfg = config();
    cfg.after_tool_call = Some(Arc::new(|_ctx, _signal| {
        Box::pin(async move {
            Ok(Some(AfterToolCallResult {
                terminate: Some(true),
                ..Default::default()
            }))
        })
    }));
    let calls = Arc::new(AtomicUsize::new(0));
    let stream_fn: pi_agent_core::agent::StreamFn = {
        let calls = Arc::clone(&calls);
        Arc::new(move |_model, _context, _options| {
            calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move {
                let (sender, stream) = create_assistant_message_event_stream();
                sender.push(AssistantMessageEvent::Done {
                    reason: pi_ai::types::DoneReason::ToolUse,
                    message: tool_use(vec![AssistantContent::ToolCall(tool_call(
                        "tool-1",
                        "echo",
                        json!({"value": "hello"}),
                    ))]),
                });
                Ok(stream)
            })
        })
    };
    let stream = agent_loop(
        vec![agent_message(user("echo something"))],
        context(Vec::new(), vec![Arc::new(tool)]),
        cfg,
        None,
        Some(stream_fn),
    );
    collect_events(stream).await.1.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn continue_throws_when_context_has_no_messages() {
    let error = agent_loop_continue(context(Vec::new(), Vec::new()), config(), None, None)
        .err()
        .unwrap();
    assert_eq!(error.to_string(), "Cannot continue: no messages in context");
}

#[tokio::test]
async fn restarts_unfinished_tool_call_before_requesting_the_model() {
    let executed = Arc::new(Mutex::new(Vec::<String>::new()));
    let executed_hook = Arc::clone(&executed);
    let mut tool = TestTool::new("echo", move |_id, params| {
        let value = fixture_string(&params["value"]).to_owned();
        executed_hook.lock().unwrap().push(value.clone());
        text_result(format!("echoed: {value}"))
    });
    tool.parameters = serde_json::json!({"type": "object", "properties": {"value": {"type": "string"}}});
    let unfinished = tool_use(vec![AssistantContent::ToolCall(tool_call(
        "tool-restored",
        "echo",
        json!({"value": "restored"}),
    ))]);
    let executed_before_model = Arc::new(Mutex::new(Vec::<String>::new()));
    let executed_before_model_hook = Arc::clone(&executed_before_model);
    let executed_hook = Arc::clone(&executed);
    let stream_fn: pi_agent_core::agent::StreamFn = Arc::new(move |_model, _context, _options| {
        *executed_before_model_hook.lock().unwrap() = executed_hook.lock().unwrap().clone();
        Box::pin(async move {
            let (sender, stream) = create_assistant_message_event_stream();
            sender.push(AssistantMessageEvent::Done {
                reason: pi_ai::types::DoneReason::Stop,
                message: text_assistant("Done"),
            });
            Ok(stream)
        })
    });
    let stream = agent_loop_continue(
        context(
            vec![
                agent_message(user("Echo this")),
                AgentMessage::Assistant(Box::new(unfinished)),
            ],
            vec![Arc::new(tool)],
        ),
        config(),
        None,
        Some(stream_fn),
    )
    .unwrap();
    let (events, result) = collect_events(stream).await;
    let messages = result.unwrap();
    assert_eq!(*executed_before_model.lock().unwrap(), vec!["restored".to_owned()]);
    assert_eq!(*executed.lock().unwrap(), vec!["restored".to_owned()]);
    assert_eq!(
        messages.iter().map(|message| message.role()).collect::<Vec<_>>(),
        vec!["toolResult", "assistant"]
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, AgentEvent::ToolExecutionStart { .. }))
            .count(),
        1
    );
}

#[tokio::test]
async fn reuses_persisted_agent_id_when_continuing_interrupted_tool_call() {
    let preparation_calls = Arc::new(AtomicUsize::new(0));
    let preparation_hook = Arc::clone(&preparation_calls);
    let executed_agent_ids = Arc::new(Mutex::new(Vec::<String>::new()));
    let executed_hook = Arc::clone(&executed_agent_ids);
    let mut tool = TestTool::new("echo", |_id, _params| text_result("ok"));
    tool.parameters = serde_json::json!({"type": "object", "properties": {"value": {"type": "string"}}});
    tool.prepare_execution = Some(Arc::new(move |_id, _signal| {
        preparation_hook.fetch_add(1, Ordering::SeqCst);
        Box::pin(async {
            Ok(Some(ToolCallExecution {
                agent_id: "new-agent".to_owned(),
            }))
        })
    }));
    tool.execute = Arc::new(move |_id, _params, _signal, _on_update| {
        let executed_hook = Arc::clone(&executed_hook);
        Box::pin(async move {
            executed_hook.lock().unwrap().push("known-agent".to_owned());
            Ok(text_result("ok"))
        })
    });
    let mut call = tool_call("provider-call", "echo", json!({"value": "restored"}));
    call.execution = Some(ToolCallExecution {
        agent_id: "known-agent".to_owned(),
    });
    let assistant = tool_use(vec![AssistantContent::ToolCall(call)]);
    let stream = agent_loop_continue(
        context(
            vec![
                agent_message(user("Echo this")),
                AgentMessage::Assistant(Box::new(assistant)),
            ],
            vec![Arc::new(tool)],
        ),
        config(),
        None,
        Some(scripted_stream_fn(vec![Script::message(text_assistant("Done"))])),
    )
    .unwrap();
    collect_events(stream).await.1.unwrap();
    assert_eq!(preparation_calls.load(Ordering::SeqCst), 0);
    assert_eq!(*executed_agent_ids.lock().unwrap(), vec!["known-agent".to_owned()]);
}

#[tokio::test]
async fn restarts_only_unfinished_tool_calls_when_earlier_result_exists() {
    let executed = Arc::new(Mutex::new(Vec::<String>::new()));
    let executed_hook = Arc::clone(&executed);
    let mut tool = TestTool::new("echo", move |_id, params| {
        let value = fixture_string(&params["value"]).to_owned();
        executed_hook.lock().unwrap().push(value.clone());
        text_result(&value)
    });
    tool.parameters = serde_json::json!({"type": "object", "properties": {"value": {"type": "string"}}});
    let interrupted = tool_use(vec![
        AssistantContent::ToolCall(tool_call("tool-complete", "echo", json!({"value": "complete"}))),
        AssistantContent::ToolCall(tool_call("tool-pending", "echo", json!({"value": "pending"}))),
    ]);
    let existing_result = pi_ai::types::ToolResultMessage {
        role: pi_ai::types::ToolResultRole::Tag,
        tool_call_id: "tool-complete".to_owned(),
        tool_name: "echo".to_owned(),
        content: vec![TextOrImageContent::Text(TextContent::new("complete"))],
        details: None,
        is_error: false,
        timestamp: 1000.0,
    };
    let stream = agent_loop_continue(
        context(
            vec![
                agent_message(user("Echo twice")),
                AgentMessage::Assistant(Box::new(interrupted)),
                AgentMessage::ToolResult(existing_result),
            ],
            vec![Arc::new(tool)],
        ),
        config(),
        None,
        Some(scripted_stream_fn(vec![Script::message(text_assistant("Done"))])),
    )
    .unwrap();
    let (_, result) = collect_events(stream).await;
    let messages = result.unwrap();
    assert_eq!(*executed.lock().unwrap(), vec!["pending".to_owned()]);
    let tool_results: Vec<_> = messages.iter().filter_map(|message| message.as_tool_result()).collect();
    assert_eq!(tool_results.len(), 1);
    assert_eq!(tool_results[0].tool_call_id, "tool-pending");
}

#[tokio::test]
async fn continues_from_existing_context_without_user_message_events() {
    let stream = agent_loop_continue(
        context(vec![agent_message(user("Hello"))], Vec::new()),
        config(),
        None,
        Some(scripted_stream_fn(vec![Script::message(text_assistant("Response"))])),
    )
    .unwrap();
    let (events, result) = collect_events(stream).await;
    let messages = result.unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].role(), "assistant");
    let message_end_events: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            AgentEvent::MessageEnd { message } => Some(message.role()),
            _ => None,
        })
        .collect();
    assert_eq!(message_end_events, vec!["assistant"]);
}

#[tokio::test]
async fn allows_custom_message_types_as_last_message() {
    let custom = AgentMessage::Custom(
        json!({"role": "custom", "text": "Hook content", "timestamp": 1000})
            .as_object()
            .cloned()
            .unwrap(),
    );
    let mut cfg = config();
    cfg.convert_to_llm = Arc::new(|messages| {
        Box::pin(async move {
            Ok(messages
                .into_iter()
                .map(|message| match message {
                    AgentMessage::Custom(object) => pi_ai::types::Message::User(pi_ai::types::UserMessage {
                        role: pi_ai::types::UserRole::Tag,
                        content: UserContent::Text(
                            object
                                .get("text")
                                .and_then(JsValue::as_string)
                                .expect("custom text fixture")
                                .clone(),
                        ),
                        input_source: None,
                        provider_native: None,
                        timestamp: 1000.0,
                    }),
                    AgentMessage::User(message) => pi_ai::types::Message::User(message),
                    AgentMessage::Assistant(message) => pi_ai::types::Message::Assistant(message),
                    AgentMessage::ToolResult(message) => pi_ai::types::Message::ToolResult(message),
                })
                .collect())
        })
    });
    let stream = agent_loop_continue(
        context(vec![custom], Vec::new()),
        cfg,
        None,
        Some(scripted_stream_fn(vec![Script::message(text_assistant(
            "Response to custom message",
        ))])),
    )
    .unwrap();
    let (_, result) = collect_events(stream).await;
    result.unwrap();
}

#[test]
fn thinking_level_maps_onto_reasoning() {
    assert!(pi_agent_core::agent::reasoning_for(ThinkingLevel::Off).is_none());
    assert_eq!(
        pi_agent_core::agent::reasoning_for(ThinkingLevel::Low),
        Some(pi_ai::types::ThinkingLevel::Low)
    );
}

const TOOL_RESULT_CAP_PREFIX: &str = "[tool output truncated: max 10240 bytes / 2000 lines]\n";

async fn tool_result_for(text: String, details: Option<JsValue>) -> pi_ai::types::ToolResultMessage {
    let tool = TestTool::new("big", move |_id, _params| AgentToolResult {
        content: vec![TextOrImageContent::Text(TextContent::new(text.clone()))],
        details: details.clone(),
        is_error: None,
        terminate: None,
    });
    let stream = agent_loop(
        vec![agent_message(user("run"))],
        context(Vec::new(), vec![Arc::new(tool)]),
        config(),
        None,
        Some(scripted_stream_fn(vec![
            Script::message(tool_use(vec![AssistantContent::ToolCall(tool_call(
                "call-1",
                "big",
                json!({"value": "x"}),
            ))])),
            Script::message(text_assistant("done")),
        ])),
    );
    let (_events, result) = collect_events(stream).await;
    result
        .unwrap()
        .into_iter()
        .find_map(|message| match message {
            AgentMessage::ToolResult(result) => Some(result),
            _ => None,
        })
        .expect("loop must produce a tool result")
}

fn tool_result_text(result: &pi_ai::types::ToolResultMessage) -> JsString {
    JsString::join(
        result.content.iter().map(|part| match part {
            TextOrImageContent::Text(text) => &text.text,
            TextOrImageContent::Image(_) => panic!("expected text content"),
        }),
        "",
    )
}

#[tokio::test]
async fn caps_tool_result_text_at_the_line_limit() {
    let lines: Vec<String> = (0..2001)
        .map(|index| char::from(b'a' + (index % 26) as u8).to_string())
        .collect();
    let result = tool_result_for(lines.join("\n"), None).await;
    let expected = format!("{TOOL_RESULT_CAP_PREFIX}{}", lines[1..].join("\n"));
    assert_eq!(tool_result_text(&result), expected);
    assert_eq!(result.details, None);
}

#[tokio::test]
async fn caps_tool_result_text_at_the_byte_limit() {
    let result = tool_result_for("a".repeat(20000), None).await;
    let expected = format!("{TOOL_RESULT_CAP_PREFIX}{}", "a".repeat(9567));
    assert_eq!(tool_result_text(&result), expected);
}

#[tokio::test]
async fn caps_tool_result_text_by_utf16_code_units() {
    let result = tool_result_for(format!("a{}", "😀".repeat(3000)), None).await;
    let mut expected = JsString::from(TOOL_RESULT_CAP_PREFIX);
    expected.push_unit(0xde00);
    expected.push_str("😀".repeat(2430));
    assert_eq!(tool_result_text(&result), expected);
}

#[tokio::test]
async fn caps_tool_result_details_with_a_capped_preview() {
    let result = tool_result_for("small".to_owned(), Some(json!({"value": "y".repeat(20000)}))).await;
    assert_eq!(tool_result_text(&result), "small");
    let details = result.details.expect("details must survive capping");
    assert_eq!(details["truncated"], json!(true));
    assert_eq!(details["reason"], json!("tool result details exceeded storage cap"));
    assert_eq!(details["maxBytes"], json!(10240));
    assert_eq!(details["maxLines"], json!(2000));
    let preview = details["preview"].as_string().expect("preview must be text");
    assert!(preview.starts_with(TOOL_RESULT_CAP_PREFIX));
    assert!(preview.ends_with("\"}"));
    assert_eq!(preview.len(), TOOL_RESULT_CAP_PREFIX.len() + 9574);
}

#[tokio::test]
async fn js_details_at_storage_boundary_are_not_truncated() {
    let details = serde_json::from_str::<JsValue>(r#"{"2":2.0,"1":1.0,"x":3.0}"#).unwrap();
    let mut details = details.as_object().unwrap().clone();
    // JS length is exactly 10240; serde's three .0 suffixes cross the cap.
    details.insert("pad".into(), json!("x".repeat(10240 - 28)));
    let details = JsValue::Object(details);
    assert_eq!(pi_js_compat::js_json::stringify_lossless(&details).len(), 10240);
    let result = tool_result_for("small".into(), Some(details.clone())).await;
    assert_eq!(result.details, Some(details));
}

#[tokio::test]
async fn leaves_small_tool_results_unchanged() {
    let result = tool_result_for("small".to_owned(), Some(json!({"ok": true}))).await;
    assert_eq!(tool_result_text(&result), "small");
    assert_eq!(result.details, Some(json!({"ok": true})));
}

#[tokio::test]
async fn rejects_stream_acquisition_without_fabricating_an_assistant_message() {
    let failure = JsString::from_utf16(vec![0xd800, 0x21, 0xdfff]);
    let stream_fn: pi_agent_core::agent::StreamFn = Arc::new({
        let failure = failure.clone();
        move |_, _, _| {
            let failure = failure.clone();
            Box::pin(async move { Err(pi_agent_core::agent::AgentError::new(failure)) })
        }
    });
    let stream = agent_loop(
        vec![agent_message(user("prompt"))],
        context(Vec::new(), Vec::new()),
        config(),
        None,
        Some(stream_fn),
    );
    let (events, error) = collect_events_allowing_failure(stream).await;
    assert_eq!(error, Some(failure));
    assert!(!events.iter().any(|event| matches!(
        event,
        AgentEvent::MessageStart {
            message: AgentMessage::Assistant(_)
        } | AgentEvent::MessageEnd {
            message: AgentMessage::Assistant(_)
        } | AgentEvent::TurnEnd { .. }
            | AgentEvent::AgentEnd { .. }
    )));
}

#[tokio::test]
async fn rejects_steering_and_follow_up_queue_failures_at_the_poll_boundary() {
    for steering in [true, false] {
        let failure = JsString::from_utf16(vec![0xd800, 0x21, 0xdfff]);
        let hook: pi_agent_core::agent::MessageQueueHook = Arc::new({
            let failure = failure.clone();
            move || {
                let failure = failure.clone();
                Box::pin(async move { Err(pi_agent_core::agent::AgentError::new(failure)) })
            }
        });
        let mut config = config();
        if steering {
            config.get_steering_messages = Some(hook);
        } else {
            config.get_follow_up_messages = Some(hook);
        }
        let stream = agent_loop(
            vec![agent_message(user("prompt"))],
            context(Vec::new(), Vec::new()),
            config,
            None,
            Some(scripted_stream_fn(vec![Script::message(text_assistant("answer"))])),
        );
        let (events, error) = collect_events_allowing_failure(stream).await;
        assert_eq!(error, Some(failure));
        assert!(!event_types(&events).contains(&"agent_end"));
        let assistant_ends = events
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    AgentEvent::MessageEnd {
                        message: AgentMessage::Assistant(_)
                    }
                )
            })
            .count();
        assert_eq!(assistant_ends, usize::from(!steering));
    }
}
