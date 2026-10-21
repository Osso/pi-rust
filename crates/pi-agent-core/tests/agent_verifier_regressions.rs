mod steering {

    use pi_agent_core::agent::*;
    use pi_ai::request::BoxFuture;
    use pi_ai::types::*;
    use serde_json::{Value, json};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use tokio_util::sync::CancellationToken;
    struct EndTool(Value);
    impl AgentTool for EndTool {
        fn name(&self) -> &str {
            "end_turn"
        }
        fn label(&self) -> &str {
            "End"
        }
        fn description(&self) -> &str {
            "End"
        }
        fn parameters(&self) -> &Value {
            &self.0
        }
        fn execute(
            &self,
            _: String,
            _: Value,
            _: Option<CancellationToken>,
            _: AgentToolUpdateCallback,
            _: Option<AgentToolExecutionContext>,
        ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
            Box::pin(async { Ok(AgentToolResult::text("done")) })
        }
    }
    #[tokio::test]
    async fn steering_clears_end_turn_reminder() {
        let model = Agent::new(AgentOptions::default()).unwrap().model();
        let convert: ConvertToLlm = Arc::new(|messages| {
            Box::pin(async move {
                Ok(messages
                    .into_iter()
                    .filter_map(|m| match m {
                        AgentMessage::User(v) => Some(Message::User(v)),
                        AgentMessage::Assistant(v) => Some(Message::Assistant(v)),
                        _ => None,
                    })
                    .collect())
            })
        });
        let mut cfg = AgentLoopConfig::new(model, convert);
        let polls = Arc::new(AtomicUsize::new(0));
        cfg.get_steering_messages = Some(Arc::new(move || {
            let poll = polls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move {
                if poll == 1 {
                    vec![
                        UserMessage {
                            role: UserRole::Tag,
                            content: UserContent::Text("new user request".into()),
                            input_source: None,
                            provider_native: None,
                            timestamp: 0,
                        }
                        .into(),
                    ]
                } else {
                    vec![]
                }
            })
        }));
        let turns = Arc::new(AtomicUsize::new(0));
        cfg.should_stop_after_turn = Some(Arc::new(move |_| {
            let stop = turns.fetch_add(1, Ordering::SeqCst) >= 1;
            Box::pin(async move { Ok(stop) })
        }));
        let requests = Arc::new(Mutex::new(vec![]));
        let recorded = requests.clone();
        let stream: StreamFn = Arc::new(move |model, context, _| {
            recorded
                .lock()
                .unwrap()
                .push(serde_json::to_value(context.messages).unwrap());
            Box::pin(async move {
                let (sender, stream) = pi_ai::utils::event_stream::create_assistant_message_event_stream();
                let mut msg = AssistantMessage::empty(model.api, model.provider, model.id, 0);
                msg.stop_reason = StopReason::Stop;
                msg.content = vec![AssistantContent::Text(TextContent::new("text without end_turn"))];
                sender.push(AssistantMessageEvent::Done {
                    reason: DoneReason::Stop,
                    message: msg,
                });
                stream
            })
        });
        let ctx = AgentContext {
            system_prompt: String::new(),
            messages: vec![],
            tools: Some(vec![Arc::new(EndTool(json!({"type":"object"})))]),
        };
        let sink: AgentEventSink = Arc::new(|_| Box::pin(async { Ok(()) }));
        run_agent_loop(vec![], ctx, cfg, sink, None, Some(stream))
            .await
            .unwrap();
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[1].as_array().unwrap().len(), 2);
        assert_eq!(requests[1][1]["content"], "new user request");
    }
}
mod lifetime {

    use pi_agent_core::agent::*;
    use pi_ai::request::BoxFuture;
    use pi_ai::types::*;
    use serde_json::{Value, json};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use tokio_util::sync::CancellationToken;
    struct Tool {
        schema: Value,
        mode: bool,
        effect: Arc<AtomicBool>,
    }
    impl AgentTool for Tool {
        fn name(&self) -> &str {
            "echo"
        }
        fn label(&self) -> &str {
            "Echo"
        }
        fn description(&self) -> &str {
            "Echo"
        }
        fn parameters(&self) -> &Value {
            &self.schema
        }
        fn execute(
            &self,
            _: String,
            _: Value,
            _: Option<CancellationToken>,
            update: AgentToolUpdateCallback,
            _: Option<AgentToolExecutionContext>,
        ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
            let mode = self.mode;
            let effect = self.effect.clone();
            Box::pin(async move {
                if mode {
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    effect.store(true, Ordering::SeqCst);
                } else {
                    update(AgentToolResult::text("one"));
                    update(AgentToolResult::text("two"));
                }
                Ok(AgentToolResult::text("done"))
            })
        }
    }
    async fn run_case(abort: bool) -> Value {
        let model = Agent::new(AgentOptions::default()).unwrap().model();
        let convert: ConvertToLlm = Arc::new(|_| Box::pin(async { Ok(vec![]) }));
        let mut cfg = AgentLoopConfig::new(model, convert);
        cfg.tool_execution = ToolExecutionMode::Sequential;
        cfg.should_stop_after_turn = Some(Arc::new(|_| Box::pin(async { Ok(true) })));
        let effect = Arc::new(AtomicBool::new(false));
        let tool = Tool {
            schema: json!({"type":"object"}),
            mode: abort,
            effect: effect.clone(),
        };
        let ctx = AgentContext {
            system_prompt: "".into(),
            messages: vec![],
            tools: Some(vec![Arc::new(tool)]),
        };
        let stream: StreamFn = Arc::new(|m, _, _| {
            Box::pin(async move {
                let (sender, stream) = pi_ai::utils::event_stream::create_assistant_message_event_stream();
                let mut msg = AssistantMessage::empty(m.api, m.provider, m.id, 0);
                msg.stop_reason = StopReason::ToolUse;
                msg.content = vec![AssistantContent::ToolCall(ToolCall::new(
                    "call",
                    "echo",
                    serde_json::Map::new(),
                ))];
                sender.push(AssistantMessageEvent::Done {
                    reason: DoneReason::ToolUse,
                    message: msg,
                });
                stream
            })
        });
        let updates = Arc::new(AtomicUsize::new(0));
        let sink: AgentEventSink = Arc::new(move |event| {
            let update = matches!(event, AgentEvent::ToolExecutionUpdate { .. });
            let index = if update {
                updates.fetch_add(1, Ordering::SeqCst)
            } else {
                99
            };
            Box::pin(async move {
                if update && !abort {
                    if index == 0 {
                        std::future::pending::<()>().await;
                    }
                    return Err(AgentError::new("update rejected"));
                }
                Ok(())
            })
        });
        let signal = CancellationToken::new();
        if abort {
            let signal = signal.clone();
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                signal.cancel();
            });
        }
        let outcome = tokio::time::timeout(
            std::time::Duration::from_millis(150),
            run_agent_loop(vec![], ctx, cfg, sink, Some(signal), Some(stream)),
        )
        .await;
        let status = match outcome {
            Ok(Ok(_)) => "ok".to_owned(),
            Ok(Err(e)) => e.to_string(),
            Err(_) => "timeout".to_owned(),
        };
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        json!({"outcome":status,"effect":effect.load(Ordering::SeqCst)})
    }
    #[tokio::test]
    async fn rejected_update_does_not_wait_for_pending_sink() {
        assert_eq!(run_case(false).await["outcome"], "update rejected");
    }
    #[tokio::test]
    async fn aborted_tool_keeps_ignored_signal_side_effect() {
        let result = run_case(true).await;
        assert_eq!(result["outcome"], "Agent run aborted");
        assert_eq!(result["effect"], true);
    }
}
mod proxy_body {

    use pi_agent_core::agent::*;
    use pi_ai::request::{BoxFuture, CallbackError, Fetch, HttpBody, HttpRequest, HttpResponse};
    use pi_ai::types::Context;
    use std::sync::Arc;
    use tokio_util::sync::CancellationToken;
    struct BrokenBody;
    impl HttpBody for BrokenBody {
        fn next_chunk(&mut self) -> BoxFuture<'_, Option<Result<Vec<u8>, CallbackError>>> {
            Box::pin(async { Some(Err(CallbackError::new("body read failed"))) })
        }
    }
    struct BrokenFetch;
    impl Fetch for BrokenFetch {
        fn fetch(
            &self,
            _: HttpRequest,
            _: Option<CancellationToken>,
        ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
            Box::pin(async {
                Ok(HttpResponse {
                    status: 502,
                    status_text: "Bad Gateway".into(),
                    headers: vec![],
                    body: Box::new(BrokenBody),
                })
            })
        }
    }
    #[tokio::test]
    async fn failed_error_body_preserves_http_status() {
        let m = Agent::new(AgentOptions::default()).unwrap().model();
        let mut s = stream_proxy(
            m,
            Context {
                system_prompt: None,
                messages: vec![],
                tools: None,
            },
            ProxyStreamOptions {
                proxy_url: "https://invalid".into(),
                fetch: Some(Arc::new(BrokenFetch)),
                ..Default::default()
            },
        );
        while s.next().await.is_some() {}
        let result = s.result().await.unwrap();
        assert_eq!(result.error_message.as_deref(), Some("Proxy error: 502 Bad Gateway"));
    }
}
