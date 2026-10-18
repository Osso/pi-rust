use pi_protocol::{
    AbortCommand, AssistantContent, AssistantStopReason, AssistantTranscriptItem, AssistantTranscriptItemInput,
    AssistantTranscriptState, AttachCommand, ClientHello, ClientMessage, Command, CommandResult, CreateCommand,
    CreateCommandInput, DetachCommand, DetachResult, ErrorResponse, EventEnvelope, FinishedTranscriptItem,
    ImageContent, JsonValue, ListCommand, ListResult, ModelCost, ModelMetadata, ModelMetadataInput, ModelRef,
    PROTOCOL_VERSION, PromptCommand, ProtocolError, ProtocolErrorCode, RequestEnvelope, ResponseEnvelope, ServerEvent,
    ServerHello, ServerHelloError, ServerMessage, ServerSnapshot, SessionMetadata, SessionMetadataInput, SessionPhase,
    SessionProgress, SessionRemoved, SessionSnapshot, SessionSnapshotInput, SetModelCommand, SetThinkingCommand,
    SteerCommand, SuccessResponse, TextContent, ThinkingContent, ThinkingLevel, ToolCallContent, ToolContent,
    ToolTranscriptItem, ToolTranscriptItemInput, ToolTranscriptState, TranscriptDeltaKind, TranscriptProgress,
    UpdatedTranscriptItem, Usage, UsageCost, UsageInput, UserContent, UserTranscriptItem, decode_cbor,
    encode_client_message, encode_server_message, parse_client_message, parse_server_message,
};

fn model_ref() -> ModelRef {
    ModelRef::new("test".to_string(), "model".to_string()).expect("model ref must be valid")
}

fn usage() -> Usage {
    Usage::new(UsageInput {
        input: 1,
        output: 2,
        cache_read: 3,
        cache_write: 4,
        reasoning: Some(5),
        total_tokens: 15,
        cost: UsageCost::new(0.1, 0.2, 0.3, 0.4, 1.0).expect("cost must be valid"),
    })
}

fn text_content(value: &str) -> TextContent {
    TextContent::new(value.to_string())
}

fn user_item(id: &str) -> UserTranscriptItem {
    UserTranscriptItem::new(
        id.to_string(),
        vec![
            UserContent::Text(text_content("hello")),
            UserContent::Image(
                ImageContent::new("data".to_string(), "image/png".to_string()).expect("mime type must be non-empty"),
            ),
        ],
        1,
    )
    .expect("user item must be valid")
}

fn assistant_item(id: &str, state: AssistantTranscriptState) -> AssistantTranscriptItem {
    AssistantTranscriptItem::new(AssistantTranscriptItemInput {
        id: id.to_string(),
        content: vec![
            AssistantContent::Text(text_content("hello")),
            AssistantContent::Thinking(ThinkingContent::new("thinking".to_string(), Some(true))),
            AssistantContent::ToolCall(
                ToolCallContent::new(
                    "call-1".to_string(),
                    "read".to_string(),
                    JsonValue::Object(vec![("path".to_string(), JsonValue::String("/tmp/file".to_string()))]),
                )
                .expect("tool call must be valid"),
            ),
        ],
        model: model_ref(),
        response_model: Some("model-response".to_string()),
        usage: Some(usage()),
        timestamp: 2,
        state,
    })
    .expect("assistant item must be valid")
}

fn tool_item(id: &str, state: ToolTranscriptState) -> ToolTranscriptItem {
    ToolTranscriptItem::new(ToolTranscriptItemInput {
        id: id.to_string(),
        tool_call_id: "call-1".to_string(),
        tool_name: "read".to_string(),
        input: JsonValue::Object(vec![("path".to_string(), JsonValue::String("/tmp/file".to_string()))]),
        content: vec![
            ToolContent::Text(text_content("done")),
            ToolContent::Image(
                ImageContent::new("data".to_string(), "image/png".to_string()).expect("mime type must be non-empty"),
            ),
        ],
        details: Some(JsonValue::Object(vec![
            (
                "lines".to_string(),
                JsonValue::Array(vec![JsonValue::Number(1.0), JsonValue::Number(2.0)]),
            ),
            ("cached".to_string(), JsonValue::Bool(false)),
        ])),
        usage: Some(usage()),
        timestamp: 3,
        state,
    })
    .expect("tool item must be valid")
}

fn session_metadata() -> SessionMetadata {
    SessionMetadata::new(SessionMetadataInput {
        id: "session-1".to_string(),
        created_at: 1,
        updated_at: Some(2),
        parent_session_id: Some("parent-1".to_string()),
        session_name: Some("Named session".to_string()),
        cwd: Some("/workspace".to_string()),
    })
    .expect("session metadata must be valid")
}

fn model_metadata() -> ModelMetadata {
    ModelMetadata::new(ModelMetadataInput {
        provider: "test".to_string(),
        id: "model".to_string(),
        name: "Model".to_string(),
        api: "api".to_string(),
        reasoning: true,
        input: vec![pi_protocol::ModelInput::Text, pi_protocol::ModelInput::Image],
        context_window: 1,
        max_tokens: 2,
        cost: ModelCost::new(0.1, 0.2, 0.3, 0.4).expect("cost must be valid"),
        supported_thinking_levels: vec![ThinkingLevel::Off, ThinkingLevel::Max],
        authenticated: true,
    })
    .expect("model metadata must be valid")
}

fn session_snapshot() -> SessionSnapshot {
    SessionSnapshot::new(SessionSnapshotInput {
        id: "session-1".to_string(),
        name: Some("Named session".to_string()),
        cwd: "/workspace".to_string(),
        created_at: 1,
        updated_at: 2,
        phase: SessionPhase::Turn,
        model: model_ref(),
        thinking_level: ThinkingLevel::High,
        attached: true,
        locked: false,
        revision: 7,
        transcript: vec![
            pi_protocol::TranscriptItem::User(user_item("user-1")),
            pi_protocol::TranscriptItem::Assistant(assistant_item(
                "assistant-streaming",
                AssistantTranscriptState::Streaming,
            )),
            pi_protocol::TranscriptItem::Assistant(assistant_item(
                "assistant-complete",
                AssistantTranscriptState::Complete {
                    stop_reason: AssistantStopReason::ToolUse,
                },
            )),
            pi_protocol::TranscriptItem::Assistant(assistant_item(
                "assistant-error",
                AssistantTranscriptState::Error {
                    error_message: Some("failed".to_string()),
                },
            )),
            pi_protocol::TranscriptItem::Assistant(assistant_item(
                "assistant-aborted",
                AssistantTranscriptState::Aborted { error_message: None },
            )),
            pi_protocol::TranscriptItem::Tool(tool_item("tool-running", ToolTranscriptState::Running)),
            pi_protocol::TranscriptItem::Tool(tool_item("tool-complete", ToolTranscriptState::Complete)),
            pi_protocol::TranscriptItem::Tool(tool_item("tool-error", ToolTranscriptState::Error)),
        ],
        queued_steer: vec![user_item("queued-1")],
        queued_steer_count: 1,
    })
    .expect("session snapshot must be valid")
}

fn server_snapshot() -> ServerSnapshot {
    ServerSnapshot::new(
        "server-1".to_string(),
        9,
        vec![session_metadata()],
        vec![model_metadata()],
    )
    .expect("server snapshot must be valid")
}

fn round_trip_server(message: &ServerMessage) {
    let frame = encode_server_message(message, None).expect("server message must encode");
    let payload = pi_protocol::FrameDecoder::new(None)
        .push(&frame)
        .expect("frame must decode")
        .pop()
        .expect("one frame must be produced");
    let parsed = parse_server_message(&decode_cbor(&payload, None).expect("payload must decode"))
        .expect("server message must parse");
    assert_eq!(&parsed, message);
}

fn round_trip_client(message: &ClientMessage) {
    let frame = encode_client_message(message, None).expect("client message must encode");
    let payload = pi_protocol::FrameDecoder::new(None)
        .push(&frame)
        .expect("frame must decode")
        .pop()
        .expect("one frame must be produced");
    let parsed = parse_client_message(&decode_cbor(&payload, None).expect("payload must decode"))
        .expect("client message must parse");
    assert_eq!(&parsed, message);
}

#[test]
fn round_trips_every_client_command() {
    let commands = vec![
        Command::List(ListCommand::new()),
        Command::Create(
            CreateCommand::new(CreateCommandInput {
                cwd: Some("/workspace".to_string()),
                name: Some("named".to_string()),
                model: Some(model_ref()),
                thinking_level: Some(ThinkingLevel::Low),
            })
            .expect("create command must be valid"),
        ),
        Command::Attach(AttachCommand::new("session-1".to_string()).expect("session id must be non-empty")),
        Command::Detach(DetachCommand::new("session-1".to_string()).expect("session id must be non-empty")),
        Command::Prompt(
            PromptCommand::new("session-1".to_string(), "inspect".to_string()).expect("session id must be non-empty"),
        ),
        Command::Steer(
            SteerCommand::new("session-1".to_string(), "adjust".to_string()).expect("session id must be non-empty"),
        ),
        Command::Abort(AbortCommand::new("session-1".to_string()).expect("session id must be non-empty")),
        Command::SetModel(
            SetModelCommand::new("session-1".to_string(), model_ref()).expect("session id must be non-empty"),
        ),
        Command::SetThinking(
            SetThinkingCommand::new("session-1".to_string(), ThinkingLevel::Max).expect("session id must be non-empty"),
        ),
    ];

    round_trip_client(&ClientMessage::Hello(ClientHello::new(PROTOCOL_VERSION)));
    for command in commands {
        round_trip_client(&ClientMessage::Request(
            RequestEnvelope::new("request-1".to_string(), command).expect("request id must be non-empty"),
        ));
    }
}

#[test]
fn round_trips_every_server_message_and_event_variant() {
    round_trip_server(&ServerMessage::Hello(
        ServerHello::new("connection-1".to_string(), server_snapshot()).expect("connection id must be non-empty"),
    ));
    round_trip_server(&ServerMessage::HelloError(ServerHelloError::new(ProtocolError::new(
        ProtocolErrorCode::Version,
        "Unsupported protocol version".to_string(),
        None,
    ))));
    round_trip_server(&ServerMessage::Response(ResponseEnvelope::Error(
        ErrorResponse::new(
            "request-1".to_string(),
            ProtocolError::new(
                ProtocolErrorCode::InvalidRequest,
                "invalid".to_string(),
                Some(JsonValue::Object(vec![(
                    "nested".to_string(),
                    JsonValue::Array(vec![JsonValue::Null, JsonValue::Bool(true)]),
                )])),
            ),
        )
        .expect("request id must be non-empty"),
    )));

    let events = vec![
        ServerEvent::ServerSnapshot(server_snapshot()),
        ServerEvent::SessionSnapshot(session_snapshot()),
        ServerEvent::SessionProgress(
            SessionProgress::new(
                "session-1".to_string(),
                TranscriptProgress::ItemStarted {
                    item: pi_protocol::TranscriptItem::Assistant(assistant_item(
                        "assistant-streaming",
                        AssistantTranscriptState::Streaming,
                    )),
                },
            )
            .expect("session id must be non-empty"),
        ),
        ServerEvent::SessionProgress(
            SessionProgress::new(
                "session-1".to_string(),
                TranscriptProgress::AssistantDelta {
                    message_id: "assistant-1".to_string(),
                    content_index: 2,
                    kind: TranscriptDeltaKind::ToolCall,
                    delta: "partial".to_string(),
                },
            )
            .expect("session id must be non-empty"),
        ),
        ServerEvent::SessionProgress(
            SessionProgress::new(
                "session-1".to_string(),
                TranscriptProgress::ItemUpdated {
                    item: UpdatedTranscriptItem::Tool(tool_item("tool-running", ToolTranscriptState::Running)),
                },
            )
            .expect("session id must be non-empty"),
        ),
        ServerEvent::SessionProgress(
            SessionProgress::new(
                "session-1".to_string(),
                TranscriptProgress::ItemFinished {
                    item: FinishedTranscriptItem::Assistant(assistant_item(
                        "assistant-aborted",
                        AssistantTranscriptState::Aborted {
                            error_message: Some("aborted".to_string()),
                        },
                    )),
                },
            )
            .expect("session id must be non-empty"),
        ),
        ServerEvent::SessionRemoved(
            SessionRemoved::new("session-1".to_string()).expect("session id must be non-empty"),
        ),
    ];
    for event in events {
        round_trip_server(&ServerMessage::Event(EventEnvelope::new(event)));
    }
}

#[test]
fn round_trips_every_command_result_variant() {
    let results = vec![
        CommandResult::List(ListResult::new(vec![session_metadata()])),
        CommandResult::Create(session_snapshot()),
        CommandResult::Attach(session_snapshot()),
        CommandResult::Detach(DetachResult::new("session-1".to_string()).expect("session id must be non-empty")),
        CommandResult::Prompt(session_snapshot()),
        CommandResult::Steer(session_snapshot()),
        CommandResult::Abort(session_snapshot()),
        CommandResult::SetModel(session_snapshot()),
        CommandResult::SetThinking(session_snapshot()),
    ];
    for result in results {
        round_trip_server(&ServerMessage::Response(ResponseEnvelope::Success(
            SuccessResponse::new("request-1".to_string(), result).expect("request id must be non-empty"),
        )));
    }
}

#[test]
fn rejects_empty_identifiers_in_typed_records() {
    assert!(ToolCallContent::new(String::new(), "read".to_string(), JsonValue::Null).is_err());
    assert!(ToolCallContent::new("call".to_string(), String::new(), JsonValue::Null).is_err());
    assert!(UserTranscriptItem::new(String::new(), Vec::new(), 0).is_err());
    assert!(
        SessionSnapshot::new(SessionSnapshotInput {
            id: String::new(),
            name: None,
            cwd: "/workspace".to_string(),
            created_at: 0,
            updated_at: 0,
            phase: SessionPhase::Idle,
            model: model_ref(),
            thinking_level: ThinkingLevel::Off,
            attached: false,
            locked: false,
            revision: 0,
            transcript: Vec::new(),
            queued_steer: Vec::new(),
            queued_steer_count: 0,
        })
        .is_err()
    );
    assert!(ServerSnapshot::new(String::new(), 0, Vec::new(), Vec::new()).is_err());
    assert!(DetachResult::new(String::new()).is_err());
    assert!(ServerHello::new(String::new(), server_snapshot()).is_err());
    assert!(SuccessResponse::new(String::new(), CommandResult::List(ListResult::new(Vec::new()))).is_err());
    assert!(
        ErrorResponse::new(
            String::new(),
            ProtocolError::new(ProtocolErrorCode::Busy, String::new(), None)
        )
        .is_err()
    );
    assert!(
        SessionProgress::new(
            String::new(),
            TranscriptProgress::item_started(pi_protocol::TranscriptItem::User(user_item("user-1")))
        )
        .is_err()
    );
    assert!(SessionRemoved::new(String::new()).is_err());
}

#[test]
fn rejects_empty_optional_strings_where_the_schema_requires_content() {
    assert!(
        AssistantTranscriptItem::new(AssistantTranscriptItemInput {
            id: "assistant-1".to_string(),
            content: Vec::new(),
            model: model_ref(),
            response_model: Some(String::new()),
            usage: None,
            timestamp: 0,
            state: AssistantTranscriptState::Streaming,
        })
        .is_err()
    );
    assert!(
        AssistantTranscriptItem::new(AssistantTranscriptItemInput {
            id: "assistant-1".to_string(),
            content: Vec::new(),
            model: model_ref(),
            response_model: None,
            usage: None,
            timestamp: 0,
            state: AssistantTranscriptState::Error {
                error_message: Some(String::new()),
            },
        })
        .is_err()
    );
    assert!(
        ToolTranscriptItem::new(ToolTranscriptItemInput {
            id: "tool-1".to_string(),
            tool_call_id: String::new(),
            tool_name: "read".to_string(),
            input: JsonValue::Null,
            content: Vec::new(),
            details: None,
            usage: None,
            timestamp: 0,
            state: ToolTranscriptState::Running,
        })
        .is_err()
    );
}

#[test]
fn finished_items_require_terminal_states() {
    assert!(FinishedTranscriptItem::new(pi_protocol::TranscriptItem::User(user_item("user-1"))).is_err());
    assert!(
        FinishedTranscriptItem::new(pi_protocol::TranscriptItem::Assistant(assistant_item(
            "assistant-streaming",
            AssistantTranscriptState::Streaming,
        )))
        .is_err()
    );
    assert!(
        FinishedTranscriptItem::new(pi_protocol::TranscriptItem::Tool(tool_item(
            "tool-running",
            ToolTranscriptState::Running,
        )))
        .is_err()
    );
    assert!(
        FinishedTranscriptItem::new(pi_protocol::TranscriptItem::Tool(tool_item(
            "tool-complete",
            ToolTranscriptState::Complete,
        )))
        .is_ok()
    );
}

#[test]
fn updated_items_reject_user_items_and_empty_delta_ids() {
    assert!(TranscriptProgress::item_updated(pi_protocol::TranscriptItem::User(user_item("user-1"))).is_err());
    assert!(
        TranscriptProgress::assistant_delta(String::new(), 0, TranscriptDeltaKind::Text, "delta".to_string()).is_err()
    );
    assert!(
        TranscriptProgress::item_updated(pi_protocol::TranscriptItem::Assistant(assistant_item(
            "assistant-1",
            AssistantTranscriptState::Streaming,
        )))
        .is_ok()
    );
}
