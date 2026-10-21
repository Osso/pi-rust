pub mod support;

use pi_agent_core::agent::*;
use pi_agent_core::harness::{BeforeToolCallResult, ThinkingLevel};
use pi_ai::types::{AssistantContent, ToolCallExecution};
use serde_json::json;
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
        cfg.should_stop_after_turn = Some(Arc::new(move |ctx, _| {
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
    tool.parameters = json!({"type":"object","properties":{"value":{"type":"string"}},"required":["value"]});
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
async fn empty_block_reason_uses_default_text() {
    let mut cfg = AgentLoopConfig::new(model(), identity_convert());
    cfg.before_tool_call = Some(Arc::new(|_, _| {
        Box::pin(async {
            Ok(Some(BeforeToolCallResult {
                block: Some(true),
                reason: Some(String::new()),
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
