pub use crate::shared_agent_support as support;

use pi_agent_core::agent::*;
use pi_agent_core::harness::{BeforeToolCallResult, ThinkingLevel};
use pi_ai::types::{AssistantContent, ToolCallExecution};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use std::sync::{Arc, Mutex};
use support::*;

fn context(messages: Vec<AgentMessage>, tools: Vec<Arc<dyn AgentTool>>) -> AgentContext {
    AgentContext {
        system_prompt: "original".into(),
        messages,
        tools: Some(tools),
    }
}

#[tokio::test]
async fn stop_hook_observes_replaced_context_for_prompt_and_restoration() {
    for restored in [false, true] {
        let mut cfg = AgentLoopConfig::new(model(), identity_convert());
        cfg.prepare_next_turn = Some(Arc::new(|ctx| {
            Box::pin(async move {
                let mut context = ctx.context;
                context.system_prompt = "replacement".into();
                Ok(Some(AgentLoopTurnUpdate {
                    context: Some(context),
                    model: None,
                    thinking_level: Some(ThinkingLevel::Off),
                }))
            })
        }));
        let observed = Arc::new(Mutex::new(Vec::new()));
        let observed_hook = observed.clone();
        cfg.should_stop_after_turn = Some(Arc::new(move |ctx: ShouldStopAfterTurnContext| {
            observed_hook.lock().unwrap().push(ctx.context.system_prompt);
            Box::pin(async { Ok(true) })
        }));
        let tool = TestTool::no_args("lookup", || text_result("found"));
        let ctx = context(
            if restored {
                vec![tool_use(vec![AssistantContent::ToolCall(tool_call("call", "lookup", json!({})))]).into()]
            } else {
                Vec::new()
            },
            vec![Arc::new(tool)],
        );
        let stream_fn = Some(scripted_stream_fn(vec![Script::message(text_assistant("done"))]));
        let stream = if restored {
            agent_loop_continue(ctx, cfg, None, stream_fn).unwrap()
        } else {
            agent_loop(vec![user("hello").into()], ctx, cfg, None, stream_fn)
        };
        collect_events(stream).await.1.unwrap();
        assert_eq!(*observed.lock().unwrap(), vec!["replacement"]);
    }
}

#[tokio::test]
async fn hooks_keep_original_tool_call_after_argument_preparation() {
    let mut tool = TestTool::new("lookup", |_, args| {
        assert_eq!(args, json!({"value":"prepared"}));
        text_result("found")
    });
    tool.parameters =
        serde_json::json!({"type":"object","properties":{"value":{"type":"string"}},"required":["value"]});
    tool.prepare_arguments = Some(Arc::new(|_| Ok(json!({"value":"prepared"}))));
    let mut cfg = AgentLoopConfig::new(model(), identity_convert());
    let observed = Arc::new(Mutex::new(Vec::new()));
    let observed_hook = observed.clone();
    cfg.after_tool_call = Some(Arc::new(move |ctx, _| {
        observed_hook
            .lock()
            .unwrap()
            .push(ctx.tool_call.arguments["value"].clone());
        Box::pin(async { Ok(None) })
    }));
    let response = tool_use(vec![AssistantContent::ToolCall(tool_call(
        "call",
        "lookup",
        json!({"value":"original"}),
    ))]);
    let stream = agent_loop(
        vec![user("hello").into()],
        context(Vec::new(), vec![Arc::new(tool)]),
        cfg,
        None,
        Some(scripted_stream_fn(vec![
            Script::message(response),
            Script::message(text_assistant("done")),
        ])),
    );
    collect_events(stream).await.1.unwrap();
    assert_eq!(*observed.lock().unwrap(), vec![json!("original")]);
}

#[tokio::test]
async fn after_override_rebuilds_result_without_old_is_error_hint() {
    let mut cfg = AgentLoopConfig::new(model(), identity_convert());
    cfg.after_tool_call = Some(Arc::new(|_, _| {
        Box::pin(async { Ok(Some(AfterToolCallResult::default())) })
    }));
    let tool = TestTool::no_args("lookup", || {
        let mut result = text_result("failed");
        result.is_error = Some(true);
        result
    });
    let response = tool_use(vec![AssistantContent::ToolCall(tool_call("call", "lookup", json!({})))]);
    let (events, result) = collect_events(agent_loop(
        vec![user("hello").into()],
        context(Vec::new(), vec![Arc::new(tool)]),
        cfg,
        None,
        Some(scripted_stream_fn(vec![
            Script::message(response),
            Script::message(text_assistant("done")),
        ])),
    ))
    .await;
    result.unwrap();
    let (result, is_error) = events
        .iter()
        .find_map(|event| match event {
            AgentEvent::ToolExecutionEnd { result, is_error, .. } => Some((result, is_error)),
            _ => None,
        })
        .unwrap();
    assert!(*is_error);
    assert_eq!(result.is_error, None);
}

#[tokio::test]
async fn null_after_hook_details_keep_the_executed_details() {
    let mut cfg = AgentLoopConfig::new(model(), identity_convert());
    cfg.after_tool_call = Some(Arc::new(|_, _| {
        Box::pin(async {
            Ok(Some(AfterToolCallResult {
                details: Some(JsValue::from(serde_json::Value::Null)),
                ..Default::default()
            }))
        })
    }));
    let tool = TestTool::no_args("lookup", || {
        let mut result = text_result("found");
        result.details = Some(json!({"kept": true}));
        result
    });
    let response = tool_use(vec![AssistantContent::ToolCall(tool_call("call", "lookup", json!({})))]);
    let (_, messages) = collect_events(agent_loop(
        vec![user("hello").into()],
        context(Vec::new(), vec![Arc::new(tool)]),
        cfg,
        None,
        Some(scripted_stream_fn(vec![
            Script::message(response),
            Script::message(text_assistant("done")),
        ])),
    ))
    .await;
    assert_eq!(
        messages
            .unwrap()
            .iter()
            .find_map(AgentMessage::as_tool_result)
            .unwrap()
            .details,
        (Some(json!({"kept": true})))
    );
}

#[tokio::test]
async fn empty_block_reason_uses_default_text() {
    let mut cfg = AgentLoopConfig::new(model(), identity_convert());
    cfg.before_tool_call = Some(Arc::new(|_, _| {
        Box::pin(async {
            Ok(Some(BeforeToolCallResult {
                block: Some(true),
                reason: Some(JsString::default()),
            }))
        })
    }));
    let response = tool_use(vec![AssistantContent::ToolCall(tool_call("call", "lookup", json!({})))]);
    let stream = agent_loop(
        vec![user("hello").into()],
        context(
            Vec::new(),
            vec![Arc::new(TestTool::no_args("lookup", || panic!("blocked")))],
        ),
        cfg,
        None,
        Some(scripted_stream_fn(vec![
            Script::message(response),
            Script::message(text_assistant("done")),
        ])),
    );
    let messages = collect_events(stream).await.1.unwrap();
    let result = messages.iter().find_map(AgentMessage::as_tool_result).unwrap();
    assert_eq!(
        result.content,
        AgentToolResult::text("Tool execution was blocked").content
    );
}

#[tokio::test]
async fn rejected_parallel_sink_does_not_cancel_already_started_sibling_tools() {
    let release = Arc::new(tokio::sync::Notify::new());
    let completed = Arc::new(tokio::sync::Notify::new());
    let release_tool = release.clone();
    let completed_tool = completed.clone();
    let mut tool = TestTool::no_args("lookup", || text_result("found"));
    tool.execute = Arc::new(move |id, _, _, _| {
        let release = release_tool.clone();
        let completed = completed_tool.clone();
        Box::pin(async move {
            if id == "slow" {
                release.notified().await;
                completed.notify_one();
            }
            Ok(text_result("found"))
        })
    });
    let response = tool_use(vec![
        AssistantContent::ToolCall(tool_call("fast", "lookup", json!({}))),
        AssistantContent::ToolCall(tool_call("slow", "lookup", json!({}))),
    ]);
    let emit: AgentEventSink = Arc::new(|event| {
        Box::pin(async move {
            if matches!(event, AgentEvent::ToolExecutionEnd { tool_call_id, .. } if tool_call_id == "fast") {
                Err(AgentError::new("sink rejected"))
            } else {
                Ok(())
            }
        })
    });
    let result = run_agent_loop(
        vec![user("hello").into()],
        context(Vec::new(), vec![Arc::new(tool)]),
        AgentLoopConfig::new(model(), identity_convert()),
        emit,
        None,
        Some(scripted_stream_fn(vec![Script::message(response)])),
    )
    .await;
    assert_eq!(result.unwrap_err().to_string(), "sink rejected");
    release.notify_one();
    tokio::time::timeout(std::time::Duration::from_millis(200), completed.notified())
        .await
        .expect("already-started sibling must finish after Promise.all rejects");
}

#[tokio::test]
async fn execution_preparation_uses_last_duplicate_tool_definition() {
    let prepared = Arc::new(Mutex::new(Vec::new()));
    let tools = ["first", "last"]
        .into_iter()
        .map(|name| {
            let mut tool = TestTool::no_args("lookup", || text_result("found"));
            let prepared = prepared.clone();
            tool.prepare_execution = Some(Arc::new(move |_, _| {
                prepared.lock().unwrap().push(name);
                Box::pin(async move { Ok(Some(ToolCallExecution { agent_id: name.into() })) })
            }));
            Arc::new(tool) as Arc<dyn AgentTool>
        })
        .collect();
    let response = tool_use(vec![AssistantContent::ToolCall(tool_call("call", "lookup", json!({})))]);
    let stream = agent_loop(
        vec![user("hello").into()],
        context(Vec::new(), tools),
        AgentLoopConfig::new(model(), identity_convert()),
        None,
        Some(scripted_stream_fn(vec![
            Script::message(response),
            Script::message(text_assistant("done")),
        ])),
    );
    collect_events(stream).await.1.unwrap();
    assert_eq!(*prepared.lock().unwrap(), vec!["last"]);
}
