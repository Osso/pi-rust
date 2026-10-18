use std::sync::Arc;

use pi_ai::models::{CreateModelsOptions, Models, MutableModels, create_models};
use pi_ai::providers::faux::{
    FauxAssistantMessageOptions, FauxDeferredOptions, FauxModelDefinition, FauxProviderStateSnapshot,
    FauxResponseFactory, FauxResponseStep, FauxTokenSize, RegisterFauxProviderOptions, create_faux_core,
    faux_assistant_message, faux_provider, faux_text, faux_thinking, faux_tool_call,
};
use pi_ai::request::{CallbackError, DeferredRequest, SimpleStreamOptions, StreamOptions};
use pi_ai::types::{
    AssistantContent, AssistantMessage, AssistantMessageEvent, CacheRetention, Context, DeferredHandle, Message,
    Modality, StopReason, TextContent, TextOrImageContent, Tool, ToolCall, ToolResultMessage, ToolResultRole,
    UserContent, UserMessage, UserRole,
};
use serde_json::json;
use tokio_util::sync::CancellationToken;

fn context() -> Context {
    Context {
        system_prompt: None,
        messages: vec![Message::User(UserMessage {
            role: UserRole::Tag,
            content: UserContent::Text("hi".to_owned()),
            input_source: None,
            provider_native: None,
            timestamp: 1,
        })],
        tools: None,
    }
}

fn simple_options() -> SimpleStreamOptions {
    SimpleStreamOptions::default()
}

async fn collect(stream: &mut pi_ai::utils::event_stream::AssistantMessageEventStream) -> Vec<AssistantMessageEvent> {
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event.unwrap());
    }
    events
}

fn event_types(events: &[AssistantMessageEvent]) -> Vec<&'static str> {
    events
        .iter()
        .map(|event| match event {
            AssistantMessageEvent::Start { .. } => "start",
            AssistantMessageEvent::TextStart { .. } => "text_start",
            AssistantMessageEvent::TextDelta { .. } => "text_delta",
            AssistantMessageEvent::TextEnd { .. } => "text_end",
            AssistantMessageEvent::ThinkingStart { .. } => "thinking_start",
            AssistantMessageEvent::ThinkingDelta { .. } => "thinking_delta",
            AssistantMessageEvent::ThinkingEnd { .. } => "thinking_end",
            AssistantMessageEvent::ToolCallStart { .. } => "toolcall_start",
            AssistantMessageEvent::ToolCallDelta { .. } => "toolcall_delta",
            AssistantMessageEvent::ToolCallEnd { .. } => "toolcall_end",
            AssistantMessageEvent::Done { .. } => "done",
            AssistantMessageEvent::Error { .. } => "error",
        })
        .collect()
}

#[tokio::test]
async fn faux_provider_registers_and_estimates_usage() {
    let faux = faux_provider(RegisterFauxProviderOptions::default());
    let models = create_models(CreateModelsOptions::default());
    models.set_provider(faux.provider.clone());
    faux.set_responses(vec![FauxResponseStep::message(faux_assistant_message(
        "hello world",
        FauxAssistantMessageOptions::default(),
    ))]);
    let model = faux.get_model(None).unwrap();
    let response = models
        .complete_simple(model, context(), simple_options())
        .await
        .unwrap();
    assert_eq!(
        response.content,
        vec![AssistantContent::Text(TextContent::new("hello world"))]
    );
    assert!(response.usage.input > 0.0);
    assert!(response.usage.output > 0.0);
    assert_eq!(
        response.usage.total_tokens,
        response.usage.input + response.usage.output
    );
    assert_eq!(faux.state().lock().unwrap().call_count, 1);
}

#[tokio::test]
async fn faux_provider_supports_thinking_text_and_tool_call_blocks() {
    let faux = faux_provider(RegisterFauxProviderOptions::default());
    let models = create_models(CreateModelsOptions::default());
    models.set_provider(faux.provider.clone());
    faux.set_responses(vec![FauxResponseStep::message(faux_assistant_message(
        vec![
            faux_thinking("think"),
            faux_tool_call("echo", json!({"text": "hi"}).as_object().unwrap().clone(), None),
            faux_text("done"),
        ],
        FauxAssistantMessageOptions {
            stop_reason: Some(StopReason::ToolUse),
            ..Default::default()
        },
    ))]);
    let response = models
        .complete_simple(faux.get_model(None).unwrap(), context(), simple_options())
        .await
        .unwrap();
    assert_eq!(response.content.len(), 3);
    assert_eq!(response.stop_reason, StopReason::ToolUse);
    assert!(matches!(response.content[0], AssistantContent::Thinking(_)));
    assert!(matches!(response.content[1], AssistantContent::ToolCall(_)));
    assert!(matches!(response.content[2], AssistantContent::Text(_)));
}

#[tokio::test]
async fn faux_provider_supports_multiple_models_and_model_aware_factories() {
    let faux = faux_provider(RegisterFauxProviderOptions {
        models: Some(vec![
            FauxModelDefinition {
                id: "faux-fast".to_owned(),
                name: Some("Faux Fast".to_owned()),
                reasoning: Some(false),
                ..Default::default()
            },
            FauxModelDefinition {
                id: "faux-thinker".to_owned(),
                name: Some("Faux Thinker".to_owned()),
                reasoning: Some(true),
                ..Default::default()
            },
        ]),
        ..Default::default()
    });
    let factory: FauxResponseFactory = Arc::new(|_context, _options, _state: FauxProviderStateSnapshot, model| {
        Box::pin(async move {
            Ok(faux_assistant_message(
                format!("{}:{}", model.id, model.reasoning),
                FauxAssistantMessageOptions::default(),
            ))
        })
    });
    faux.set_responses(vec![
        FauxResponseStep::Factory(Arc::clone(&factory)),
        FauxResponseStep::Factory(factory),
    ]);
    assert_eq!(
        faux.models.iter().map(|model| model.id.as_str()).collect::<Vec<_>>(),
        vec!["faux-fast", "faux-thinker"]
    );
    assert_eq!(faux.get_model(None).unwrap().id, "faux-fast");
    assert!(!faux.get_model(Some("faux-fast")).unwrap().reasoning);
    assert!(faux.get_model(Some("faux-thinker")).unwrap().reasoning);

    let models = create_models(CreateModelsOptions::default());
    models.set_provider(faux.provider.clone());
    let fast = models
        .complete_simple(faux.get_model(Some("faux-fast")).unwrap(), context(), simple_options())
        .await
        .unwrap();
    let thinker = models
        .complete_simple(
            faux.get_model(Some("faux-thinker")).unwrap(),
            context(),
            simple_options(),
        )
        .await
        .unwrap();
    assert_eq!(
        fast.content,
        vec![AssistantContent::Text(TextContent::new("faux-fast:false"))]
    );
    assert_eq!(
        thinker.content,
        vec![AssistantContent::Text(TextContent::new("faux-thinker:true"))]
    );
}

#[tokio::test]
async fn faux_provider_rewrites_api_provider_and_model() {
    let faux = faux_provider(RegisterFauxProviderOptions {
        api: Some("faux:test".to_owned()),
        provider: Some("faux-provider".to_owned()),
        models: Some(vec![FauxModelDefinition {
            id: "faux-model".to_owned(),
            ..Default::default()
        }]),
        ..Default::default()
    });
    faux.set_responses(vec![FauxResponseStep::message(faux_assistant_message(
        "hello",
        FauxAssistantMessageOptions::default(),
    ))]);
    let models = create_models(CreateModelsOptions::default());
    models.set_provider(faux.provider.clone());
    let response = models
        .complete_simple(faux.get_model(None).unwrap(), context(), simple_options())
        .await
        .unwrap();
    assert_eq!(response.api, "faux:test");
    assert_eq!(response.provider, "faux-provider");
    assert_eq!(response.model, "faux-model");
}

#[tokio::test]
async fn faux_provider_consumes_queued_responses_and_errors_when_exhausted() {
    let faux = faux_provider(RegisterFauxProviderOptions::default());
    let models = create_models(CreateModelsOptions::default());
    models.set_provider(faux.provider.clone());
    faux.set_responses(vec![
        FauxResponseStep::message(faux_assistant_message("first", FauxAssistantMessageOptions::default())),
        FauxResponseStep::message(faux_assistant_message("second", FauxAssistantMessageOptions::default())),
    ]);
    let model = faux.get_model(None).unwrap();
    let first = models
        .complete_simple(model.clone(), context(), simple_options())
        .await
        .unwrap();
    let second = models
        .complete_simple(model.clone(), context(), simple_options())
        .await
        .unwrap();
    let exhausted = models
        .complete_simple(model, context(), simple_options())
        .await
        .unwrap();
    assert_eq!(first.content, vec![AssistantContent::Text(TextContent::new("first"))]);
    assert_eq!(second.content, vec![AssistantContent::Text(TextContent::new("second"))]);
    assert_eq!(exhausted.stop_reason, StopReason::Error);
    assert_eq!(
        exhausted.error_message.as_deref(),
        Some("No more faux responses queued")
    );
    assert_eq!(faux.get_pending_response_count(), 0);
    assert_eq!(faux.state().lock().unwrap().call_count, 3);
}

#[tokio::test]
async fn faux_provider_can_replace_and_append_responses() {
    let faux = faux_provider(RegisterFauxProviderOptions::default());
    let models = create_models(CreateModelsOptions::default());
    models.set_provider(faux.provider.clone());
    let model = faux.get_model(None).unwrap();
    faux.set_responses(vec![FauxResponseStep::message(faux_assistant_message(
        "first",
        FauxAssistantMessageOptions::default(),
    ))]);
    assert_eq!(
        models
            .complete_simple(model.clone(), context(), simple_options())
            .await
            .unwrap()
            .content,
        vec![AssistantContent::Text(TextContent::new("first"))]
    );
    assert_eq!(faux.get_pending_response_count(), 0);

    faux.set_responses(vec![FauxResponseStep::message(faux_assistant_message(
        "second",
        FauxAssistantMessageOptions::default(),
    ))]);
    assert_eq!(faux.get_pending_response_count(), 1);
    assert_eq!(
        models
            .complete_simple(model.clone(), context(), simple_options())
            .await
            .unwrap()
            .content,
        vec![AssistantContent::Text(TextContent::new("second"))]
    );
    faux.append_responses(vec![
        FauxResponseStep::message(faux_assistant_message("third", FauxAssistantMessageOptions::default())),
        FauxResponseStep::message(faux_assistant_message("fourth", FauxAssistantMessageOptions::default())),
    ]);
    assert_eq!(faux.get_pending_response_count(), 2);
    assert_eq!(
        models
            .complete_simple(model.clone(), context(), simple_options())
            .await
            .unwrap()
            .content,
        vec![AssistantContent::Text(TextContent::new("third"))]
    );
    assert_eq!(
        models
            .complete_simple(model, context(), simple_options())
            .await
            .unwrap()
            .content,
        vec![AssistantContent::Text(TextContent::new("fourth"))]
    );
}

#[tokio::test]
async fn faux_provider_supports_async_factories_and_reports_factory_errors() {
    let faux = faux_provider(RegisterFauxProviderOptions::default());
    let models = create_models(CreateModelsOptions::default());
    models.set_provider(faux.provider.clone());
    let model = faux.get_model(None).unwrap();
    let factory: FauxResponseFactory = Arc::new(|context, _options, state: FauxProviderStateSnapshot, _model| {
        Box::pin(async move {
            Ok(faux_assistant_message(
                format!("{}:{}", context.messages.len(), state.call_count),
                FauxAssistantMessageOptions::default(),
            ))
        })
    });
    faux.set_responses(vec![FauxResponseStep::Factory(factory)]);
    let response = models
        .complete_simple(model.clone(), context(), simple_options())
        .await
        .unwrap();
    assert_eq!(response.content, vec![AssistantContent::Text(TextContent::new("1:1"))]);

    let throwing: FauxResponseFactory =
        Arc::new(|_context, _options, _state, _model| Box::pin(async { Err(CallbackError::new("boom")) }));
    faux.set_responses(vec![FauxResponseStep::Factory(throwing)]);
    let mut stream = models.stream_simple(model, context(), simple_options());
    let events = collect(&mut stream).await;
    assert_eq!(events.len(), 1);
    assert_eq!(event_types(&events), vec!["error"]);
    match &events[0] {
        AssistantMessageEvent::Error { error, .. } => {
            assert_eq!(error.stop_reason, StopReason::Error);
            assert_eq!(error.error_message.as_deref(), Some("boom"));
        }
        other => panic!("expected error, got {other:?}"),
    }
}

#[tokio::test]
async fn faux_provider_rejects_a_pending_stop_reason() {
    let faux = faux_provider(RegisterFauxProviderOptions::default());
    let models = create_models(CreateModelsOptions::default());
    models.set_provider(faux.provider.clone());
    faux.set_responses(vec![FauxResponseStep::message(faux_assistant_message(
        "partial",
        FauxAssistantMessageOptions {
            stop_reason: Some(StopReason::Pending),
            ..Default::default()
        },
    ))]);
    let mut stream = models.stream_simple(faux.get_model(None).unwrap(), context(), simple_options());
    let events = collect(&mut stream).await;
    assert!(!event_types(&events).contains(&"done"));
    let terminal = events.last().unwrap();
    match terminal {
        AssistantMessageEvent::Error { error, .. } => {
            assert_eq!(error.stop_reason, StopReason::Error);
            assert_eq!(
                error.error_message.as_deref(),
                Some("Faux response ended without a stop reason")
            );
        }
        other => panic!("expected terminal error, got {other:?}"),
    }
}

#[tokio::test]
async fn faux_provider_estimates_prompt_and_output_tokens_from_serialized_context() {
    let faux = faux_provider(RegisterFauxProviderOptions::default());
    faux.set_responses(vec![FauxResponseStep::message(faux_assistant_message(
        "done",
        FauxAssistantMessageOptions::default(),
    ))]);
    let tool = Tool {
        name: "echo".to_owned(),
        description: "Echo back text".to_owned(),
        parameters: json!({"type": "object", "properties": {"text": {"type": "string"}}}),
        constrained_sampling: None,
    };
    let prior = faux_assistant_message("prior", FauxAssistantMessageOptions::default());
    let context = Context {
        system_prompt: Some("sys".to_owned()),
        messages: vec![
            Message::User(UserMessage {
                role: UserRole::Tag,
                content: UserContent::Blocks(vec![
                    TextOrImageContent::Text(TextContent::new("hello")),
                    TextOrImageContent::Image(pi_ai::types::ImageContent::new("abcd", "image/png")),
                ]),
                input_source: None,
                provider_native: None,
                timestamp: 1,
            }),
            Message::Assistant(Box::new(prior)),
            Message::ToolResult(ToolResultMessage {
                role: ToolResultRole::Tag,
                tool_call_id: "tool-1".to_owned(),
                tool_name: "echo".to_owned(),
                content: vec![TextOrImageContent::Text(TextContent::new("tool out"))],
                details: None,
                usage: None,
                added_tool_names: None,
                is_error: false,
                timestamp: 2,
            }),
        ],
        tools: Some(vec![tool.clone()]),
    };
    let response = models_helper_complete(&faux, context).await;
    let prompt_text = [
        "system:sys".to_owned(),
        "user:hello\n[image:image/png:4]".to_owned(),
        "assistant:prior".to_owned(),
        "toolResult:echo\ntool out".to_owned(),
        format!("tools:{}", serde_json::to_string(&vec![tool]).unwrap()),
    ]
    .join("\n\n");
    let expected_prompt_tokens = (prompt_text.encode_utf16().count() as f64 / 4.0).ceil();
    let expected_output_tokens = (("done".encode_utf16().count()) as f64 / 4.0).ceil();
    assert_eq!(response.usage.input, expected_prompt_tokens);
    assert_eq!(response.usage.output, expected_output_tokens);
    assert_eq!(response.usage.cache_read, 0.0);
    assert_eq!(response.usage.cache_write, 0.0);
    assert_eq!(
        response.usage.total_tokens,
        expected_prompt_tokens + expected_output_tokens
    );
}

fn models_helper(faux: &pi_ai::providers::faux::FauxProviderHandle) -> pi_ai::models::ModelsCollection {
    let models = create_models(CreateModelsOptions::default());
    models.set_provider(faux.provider.clone());
    models
}

async fn models_helper_complete(
    faux: &pi_ai::providers::faux::FauxProviderHandle,
    context: Context,
) -> AssistantMessage {
    let models = models_helper(faux);
    models
        .complete_simple(faux.get_model(None).unwrap(), context, simple_options())
        .await
        .unwrap()
}

#[tokio::test]
async fn faux_provider_caches_prompts_per_session_and_honors_cache_retention_none() {
    let faux = faux_provider(RegisterFauxProviderOptions::default());
    let models = models_helper(&faux);
    faux.set_responses(vec![
        FauxResponseStep::message(faux_assistant_message("first", FauxAssistantMessageOptions::default())),
        FauxResponseStep::message(faux_assistant_message("second", FauxAssistantMessageOptions::default())),
        FauxResponseStep::message(faux_assistant_message("third", FauxAssistantMessageOptions::default())),
    ]);
    let model = faux.get_model(None).unwrap();
    let mut context = context();
    let first = models
        .complete_simple(
            model.clone(),
            context.clone(),
            SimpleStreamOptions {
                stream: StreamOptions {
                    session_id: Some("session-1".to_owned()),
                    cache_retention: Some(CacheRetention::Short),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert!(first.usage.cache_write > 0.0);
    assert_eq!(first.usage.cache_read, 0.0);

    context.messages.push(Message::Assistant(Box::new(first.clone())));
    context.messages.push(Message::User(UserMessage {
        role: UserRole::Tag,
        content: UserContent::Text("follow up".to_owned()),
        input_source: None,
        provider_native: None,
        timestamp: 2,
    }));
    let second = models
        .complete_simple(
            model.clone(),
            context.clone(),
            SimpleStreamOptions {
                stream: StreamOptions {
                    session_id: Some("session-1".to_owned()),
                    cache_retention: Some(CacheRetention::Short),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert!(second.usage.cache_read > 0.0);
    assert!(second.usage.input + second.usage.cache_read > second.usage.input);

    let mut no_cache_context = context;
    no_cache_context.messages.push(Message::Assistant(Box::new(second)));
    no_cache_context.messages.push(Message::User(UserMessage {
        role: UserRole::Tag,
        content: UserContent::Text("follow up".to_owned()),
        input_source: None,
        provider_native: None,
        timestamp: 3,
    }));
    let third = models
        .complete_simple(
            model,
            no_cache_context,
            SimpleStreamOptions {
                stream: StreamOptions {
                    session_id: Some("session-1".to_owned()),
                    cache_retention: Some(CacheRetention::None),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(third.usage.cache_read, 0.0);
    assert_eq!(third.usage.cache_write, 0.0);
}

#[tokio::test]
async fn faux_provider_streams_exact_event_order_for_fixed_chunks() {
    let faux = faux_provider(RegisterFauxProviderOptions {
        token_size: Some(FauxTokenSize {
            min: Some(1.0),
            max: Some(1.0),
        }),
        ..Default::default()
    });
    let models = models_helper(&faux);
    faux.set_responses(vec![FauxResponseStep::message(faux_assistant_message(
        vec![
            faux_thinking("go"),
            faux_text("ok"),
            faux_tool_call(
                "echo",
                json!({}).as_object().unwrap().clone(),
                Some("tool-1".to_owned()),
            ),
        ],
        FauxAssistantMessageOptions {
            stop_reason: Some(StopReason::ToolUse),
            ..Default::default()
        },
    ))]);
    let mut stream = models.stream_simple(faux.get_model(None).unwrap(), context(), simple_options());
    let events = collect(&mut stream).await;
    match &events[0] {
        AssistantMessageEvent::Start { partial } => assert_eq!(partial.stop_reason, StopReason::Pending),
        other => panic!("expected start, got {other:?}"),
    }
    assert_eq!(
        event_types(&events),
        vec![
            "start",
            "thinking_start",
            "thinking_delta",
            "thinking_end",
            "text_start",
            "text_delta",
            "text_end",
            "toolcall_start",
            "toolcall_delta",
            "toolcall_end",
            "done",
        ]
    );
}

#[tokio::test]
async fn faux_provider_streams_multiple_tool_calls() {
    let faux = faux_provider(RegisterFauxProviderOptions::default());
    let models = models_helper(&faux);
    faux.set_responses(vec![FauxResponseStep::message(faux_assistant_message(
        vec![
            faux_tool_call(
                "echo",
                json!({"text": "one"}).as_object().unwrap().clone(),
                Some("tool-1".to_owned()),
            ),
            faux_tool_call(
                "echo",
                json!({"text": "two"}).as_object().unwrap().clone(),
                Some("tool-2".to_owned()),
            ),
        ],
        FauxAssistantMessageOptions {
            stop_reason: Some(StopReason::ToolUse),
            ..Default::default()
        },
    ))]);
    let mut stream = models.stream_simple(faux.get_model(None).unwrap(), context(), simple_options());
    let events = collect(&mut stream).await;
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, AssistantMessageEvent::ToolCallStart { .. }))
            .count(),
        2
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, AssistantMessageEvent::ToolCallEnd { .. }))
            .count(),
        2
    );
}

#[tokio::test]
async fn faux_provider_streams_explicit_error_and_aborted_messages() {
    for (reason, message) in [
        (StopReason::Error, "upstream failed"),
        (StopReason::Aborted, "Request was aborted"),
    ] {
        let faux = faux_provider(RegisterFauxProviderOptions {
            token_size: Some(FauxTokenSize {
                min: Some(2.0),
                max: Some(2.0),
            }),
            ..Default::default()
        });
        let models = models_helper(&faux);
        faux.set_responses(vec![FauxResponseStep::message(faux_assistant_message(
            "partial",
            FauxAssistantMessageOptions {
                stop_reason: Some(reason),
                error_message: Some(message.to_owned()),
                ..Default::default()
            },
        ))]);
        let mut stream = models.stream_simple(faux.get_model(None).unwrap(), context(), simple_options());
        let events = collect(&mut stream).await;
        assert_eq!(
            event_types(&events),
            vec!["start", "text_start", "text_delta", "text_end", "error"]
        );
        match events.last().unwrap() {
            AssistantMessageEvent::Error {
                reason: error_reason,
                error,
            } => {
                assert_eq!(
                    *error_reason,
                    if reason == StopReason::Aborted {
                        pi_ai::types::ErrorReason::Aborted
                    } else {
                        pi_ai::types::ErrorReason::Error
                    }
                );
                assert_eq!(error.stop_reason, reason);
                assert_eq!(error.error_message.as_deref(), Some(message));
            }
            other => panic!("expected error, got {other:?}"),
        }
    }
}

#[tokio::test]
async fn faux_provider_aborts_before_the_first_chunk() {
    let faux = faux_provider(RegisterFauxProviderOptions {
        tokens_per_second: Some(50.0),
        token_size: Some(FauxTokenSize {
            min: Some(3.0),
            max: Some(3.0),
        }),
        ..Default::default()
    });
    faux.set_responses(vec![FauxResponseStep::message(faux_assistant_message(
        "abcdefghijklmnopqrstuvwxyz",
        FauxAssistantMessageOptions::default(),
    ))]);
    let signal = CancellationToken::new();
    signal.cancel();
    let mut stream = faux.provider.stream_simple(
        faux.get_model(None).unwrap(),
        context(),
        SimpleStreamOptions {
            stream: StreamOptions {
                request: pi_ai::request::ProviderRequestOptions {
                    signal: Some(signal),
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        },
    );
    let events = collect(&mut stream).await;
    assert_eq!(events.len(), 1);
    match &events[0] {
        AssistantMessageEvent::Error { reason, error } => {
            assert_eq!(*reason, pi_ai::types::ErrorReason::Aborted);
            assert_eq!(error.stop_reason, StopReason::Aborted);
        }
        other => panic!("expected aborted error, got {other:?}"),
    }
}

async fn abort_mid_stream(content: Vec<AssistantContent>, expected_delta: &str, forbidden_end: &str) {
    let faux = faux_provider(RegisterFauxProviderOptions {
        tokens_per_second: Some(100.0),
        token_size: Some(FauxTokenSize {
            min: Some(3.0),
            max: Some(3.0),
        }),
        ..Default::default()
    });
    let models = models_helper(&faux);
    faux.set_responses(vec![FauxResponseStep::message(faux_assistant_message(
        content,
        FauxAssistantMessageOptions {
            stop_reason: Some(StopReason::ToolUse),
            ..Default::default()
        },
    ))]);
    let signal = CancellationToken::new();
    let mut stream = models.stream_simple(
        faux.get_model(None).unwrap(),
        context(),
        SimpleStreamOptions {
            stream: StreamOptions {
                request: pi_ai::request::ProviderRequestOptions {
                    signal: Some(signal.clone()),
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        },
    );
    let mut types = Vec::new();
    let mut deltas = 0;
    while let Some(event) = stream.next().await {
        let event = event.unwrap();
        let kind = event_types(std::slice::from_ref(&event))[0];
        types.push(kind);
        if kind == expected_delta {
            deltas += 1;
            signal.cancel();
        }
    }
    assert_eq!(deltas, 1);
    assert!(types.contains(&expected_delta));
    assert!(types.contains(&"error"));
    assert!(!types.contains(&forbidden_end));
}

#[tokio::test]
async fn faux_provider_aborts_mid_text_stream() {
    abort_mid_stream(vec![faux_text("abcdefghijklmnopqrstuvwxyz")], "text_delta", "text_end").await;
}

#[tokio::test]
async fn faux_provider_aborts_mid_thinking_stream() {
    abort_mid_stream(
        vec![faux_thinking("abcdefghijklmnopqrstuvwxyz")],
        "thinking_delta",
        "thinking_end",
    )
    .await;
}

#[tokio::test]
async fn faux_provider_aborts_mid_toolcall_stream() {
    abort_mid_stream(
        vec![faux_tool_call(
            "echo",
            json!({"text": "abcdefghijklmnopqrstuvwxyz", "count": 123456789})
                .as_object()
                .unwrap()
                .clone(),
            Some("tool-1".to_owned()),
        )],
        "toolcall_delta",
        "toolcall_end",
    )
    .await;
}

#[tokio::test]
async fn faux_provider_submits_polls_and_redeems_deferred_responses() {
    let faux = faux_provider(RegisterFauxProviderOptions {
        deferred: Some(FauxDeferredOptions {
            pending_fetches: Some(1.0),
            poll_after_ms: Some(25.0),
        }),
        ..Default::default()
    });
    let models = models_helper(&faux);
    faux.set_responses(vec![FauxResponseStep::message(faux_assistant_message(
        "ready",
        FauxAssistantMessageOptions::default(),
    ))]);
    let model = faux.get_model(None).unwrap();
    let mut submission = models.stream_simple(
        model.clone(),
        context(),
        SimpleStreamOptions {
            deferred: Some(DeferredRequest::Enabled { window: None }),
            ..Default::default()
        },
    );
    let events = collect(&mut submission).await;
    assert_eq!(event_types(&events), vec!["start", "done"]);
    let deferred = submission.result().await.unwrap();
    assert_eq!(deferred.stop_reason, StopReason::Deferred);
    assert!(deferred.content.is_empty());
    let handle = deferred.deferred.clone().unwrap();
    assert_eq!(handle.provider, model.provider);
    assert_eq!(handle.model_id, model.id);
    assert_eq!(handle.api, model.api);
    assert_eq!(handle.poll_after_ms, Some(25.0));

    let pending = models
        .fetch_deferred(
            model.clone(),
            handle.clone(),
            pi_ai::request::DeferredFetchOptions::default(),
        )
        .await
        .unwrap();
    assert_eq!(pending.stop_reason, StopReason::Deferred);
    assert_eq!(pending.deferred, Some(handle.clone()));

    let ready = models
        .fetch_deferred(
            model,
            handle,
            pi_ai::request::DeferredFetchOptions {
                wait: Some(0.0),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(ready.stop_reason, StopReason::Stop);
    assert_eq!(ready.content, vec![AssistantContent::Text(TextContent::new("ready"))]);
    assert!(ready.usage.total_tokens > 0.0);
    let state = faux.state();
    let state = state.lock().unwrap();
    assert_eq!(state.call_count, 1);
    assert_eq!(state.deferred_fetch_count, 2);
}

#[tokio::test]
async fn faux_provider_records_cancellation_and_returns_deferred_failures_in_band() {
    let faux = faux_provider(RegisterFauxProviderOptions::default());
    let models = models_helper(&faux);
    let failing: FauxResponseFactory =
        Arc::new(|_context, _options, _state, _model| Box::pin(async { Err(CallbackError::new("deferred failed")) }));
    faux.set_responses(vec![
        FauxResponseStep::Factory(failing),
        FauxResponseStep::message(faux_assistant_message(
            "cancelled",
            FauxAssistantMessageOptions::default(),
        )),
    ]);
    let model = faux.get_model(None).unwrap();
    let failed_submission = models
        .complete_simple(
            model.clone(),
            context(),
            SimpleStreamOptions {
                deferred: Some(DeferredRequest::Enabled { window: None }),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let failed_handle = failed_submission.deferred.clone().unwrap();
    let failed = models
        .fetch_deferred(
            model.clone(),
            failed_handle,
            pi_ai::request::DeferredFetchOptions::default(),
        )
        .await
        .unwrap();
    assert_eq!(failed.stop_reason, StopReason::Error);
    assert_eq!(failed.error_message.as_deref(), Some("deferred failed"));

    let cancelled_submission = models
        .complete_simple(
            model.clone(),
            context(),
            SimpleStreamOptions {
                deferred: Some(DeferredRequest::Enabled { window: None }),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let cancelled_handle = cancelled_submission.deferred.clone().unwrap();
    models
        .cancel_deferred(model.clone(), cancelled_handle.clone(), Default::default())
        .await
        .unwrap();
    assert_eq!(
        faux.state().lock().unwrap().cancelled_deferred,
        vec![cancelled_handle.clone()]
    );
    let cancelled = models
        .fetch_deferred(model, cancelled_handle, pi_ai::request::DeferredFetchOptions::default())
        .await
        .unwrap();
    assert_eq!(cancelled.stop_reason, StopReason::Error);
    assert!(
        cancelled
            .error_message
            .as_deref()
            .is_some_and(|message| message.contains("was cancelled"))
    );
}

#[tokio::test]
async fn faux_core_exposes_deterministic_defaults() {
    let core = create_faux_core(RegisterFauxProviderOptions::default());
    assert!(core.api().starts_with("faux:"));
    assert_eq!(core.provider_id(), "faux");
    assert_eq!(core.models()[0].id, "faux-1");
    assert_eq!(core.models()[0].base_url, "http://localhost:0");
    assert_eq!(core.models()[0].input, vec![Modality::Text, Modality::Image]);
    assert_eq!(core.get_pending_response_count(), 0);
    assert_eq!(core.state_snapshot().call_count, 0);
    let _ = ToolCall::new("id", "name", Default::default());
    let _ = DeferredHandle {
        provider: "p".to_owned(),
        model_id: "m".to_owned(),
        api: "a".to_owned(),
        id: "i".to_owned(),
        expires_at: None,
        poll_after_ms: None,
        data: None,
    };
    let _: StreamOptions = StreamOptions::default();
}
