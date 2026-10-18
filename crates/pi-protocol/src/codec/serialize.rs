use crate::CborValue;
use crate::schemas::*;

pub(super) fn client_message(value: &ClientMessage) -> CborValue {
    match value {
        ClientMessage::Hello(hello) => map(vec![("type", text("hello")), ("version", unsigned(hello.version()))]),
        ClientMessage::Request(request) => map(vec![
            ("type", text("request")),
            ("id", text(request.id())),
            ("request", command(request.request())),
        ]),
    }
}

pub(super) fn server_message(value: &ServerMessage) -> CborValue {
    match value {
        ServerMessage::Hello(hello) => server_hello(hello),
        ServerMessage::HelloError(error) => server_hello_error(error),
        ServerMessage::Response(response) => response_envelope(response),
        ServerMessage::Event(event) => event_envelope(event),
    }
}

fn command(value: &Command) -> CborValue {
    match value {
        Command::List(_) => map(vec![("command", text("list"))]),
        Command::Create(command) => {
            let mut entries = vec![("command", text("create"))];
            push_optional(&mut entries, "cwd", command.cwd().map(text));
            push_optional(&mut entries, "name", command.name().map(text));
            push_optional(&mut entries, "model", command.model().map(model_ref));
            push_optional(
                &mut entries,
                "thinkingLevel",
                command.thinking_level().map(thinking_level),
            );
            map(entries)
        }
        Command::Attach(command) => session_command("attach", command.session_id()),
        Command::Detach(command) => session_command("detach", command.session_id()),
        Command::Prompt(command) => map(vec![
            ("command", text("prompt")),
            ("sessionId", text(command.session_id())),
            ("text", text(command.text())),
        ]),
        Command::Steer(command) => map(vec![
            ("command", text("steer")),
            ("sessionId", text(command.session_id())),
            ("text", text(command.text())),
        ]),
        Command::Abort(command) => session_command("abort", command.session_id()),
        Command::SetModel(command) => map(vec![
            ("command", text("set_model")),
            ("sessionId", text(command.session_id())),
            ("model", model_ref(command.model())),
        ]),
        Command::SetThinking(command) => map(vec![
            ("command", text("set_thinking")),
            ("sessionId", text(command.session_id())),
            ("thinkingLevel", thinking_level(command.thinking_level())),
        ]),
    }
}

fn session_command(name: &str, session_id: &str) -> CborValue {
    map(vec![("command", text(name)), ("sessionId", text(session_id))])
}

fn model_ref(value: &ModelRef) -> CborValue {
    map(vec![("provider", text(value.provider())), ("id", text(value.id()))])
}

fn model_cost(value: ModelCost) -> CborValue {
    map(vec![
        ("input", number(value.input())),
        ("output", number(value.output())),
        ("cacheRead", number(value.cache_read())),
        ("cacheWrite", number(value.cache_write())),
    ])
}

fn model_metadata(value: &ModelMetadata) -> CborValue {
    map(vec![
        ("provider", text(value.provider())),
        ("id", text(value.id())),
        ("name", text(value.name())),
        ("api", text(value.api())),
        ("reasoning", boolean(value.reasoning())),
        (
            "input",
            CborValue::Array(value.input().iter().map(|input| text(input.as_str())).collect()),
        ),
        ("contextWindow", unsigned(value.context_window())),
        ("maxTokens", unsigned(value.max_tokens())),
        ("cost", model_cost(value.cost())),
        (
            "supportedThinkingLevels",
            CborValue::Array(
                value
                    .supported_thinking_levels()
                    .iter()
                    .map(|level| text(level.as_str()))
                    .collect(),
            ),
        ),
        ("authenticated", boolean(value.authenticated())),
    ])
}

fn thinking_level(value: ThinkingLevel) -> CborValue {
    text(value.as_str())
}

fn usage_cost(value: UsageCost) -> CborValue {
    map(vec![
        ("input", number(value.input())),
        ("output", number(value.output())),
        ("cacheRead", number(value.cache_read())),
        ("cacheWrite", number(value.cache_write())),
        ("total", number(value.total())),
    ])
}

fn usage(value: Usage) -> CborValue {
    let mut entries = vec![
        ("input", unsigned(value.input())),
        ("output", unsigned(value.output())),
        ("cacheRead", unsigned(value.cache_read())),
        ("cacheWrite", unsigned(value.cache_write())),
    ];
    push_optional(&mut entries, "reasoning", value.reasoning().map(unsigned));
    entries.push(("totalTokens", unsigned(value.total_tokens())));
    entries.push(("cost", usage_cost(value.cost())));
    map(entries)
}

fn text_content(value: &TextContent) -> CborValue {
    map(vec![("type", text("text")), ("text", text(value.text()))])
}

fn thinking_content(value: &ThinkingContent) -> CborValue {
    let mut entries = vec![("type", text("thinking")), ("thinking", text(value.thinking()))];
    push_optional(&mut entries, "redacted", value.redacted().map(boolean));
    map(entries)
}

fn image_content(value: &ImageContent) -> CborValue {
    map(vec![
        ("type", text("image")),
        ("data", text(value.data())),
        ("mimeType", text(value.mime_type())),
    ])
}

fn tool_call_content(value: &ToolCallContent) -> CborValue {
    map(vec![
        ("type", text("toolCall")),
        ("toolCallId", text(value.tool_call_id())),
        ("toolName", text(value.tool_name())),
        ("input", json_value(value.input())),
    ])
}

fn user_content(value: &UserContent) -> CborValue {
    match value {
        UserContent::Text(value) => text_content(value),
        UserContent::Image(value) => image_content(value),
    }
}

fn assistant_content(value: &AssistantContent) -> CborValue {
    match value {
        AssistantContent::Text(value) => text_content(value),
        AssistantContent::Thinking(value) => thinking_content(value),
        AssistantContent::ToolCall(value) => tool_call_content(value),
    }
}

fn tool_content(value: &ToolContent) -> CborValue {
    match value {
        ToolContent::Text(value) => text_content(value),
        ToolContent::Image(value) => image_content(value),
    }
}

fn user_transcript_item(value: &UserTranscriptItem) -> CborValue {
    map(vec![
        ("id", text(value.id())),
        ("role", text("user")),
        (
            "content",
            CborValue::Array(value.content().iter().map(user_content).collect()),
        ),
        ("timestamp", unsigned(value.timestamp())),
    ])
}

fn assistant_transcript_item(value: &AssistantTranscriptItem) -> CborValue {
    let mut entries = vec![
        ("id", text(value.id())),
        ("role", text("assistant")),
        (
            "content",
            CborValue::Array(value.content().iter().map(assistant_content).collect()),
        ),
        ("model", model_ref(value.model())),
    ];
    push_optional(&mut entries, "responseModel", value.response_model().map(text));
    push_optional(&mut entries, "usage", value.usage().map(usage));
    entries.push(("timestamp", unsigned(value.timestamp())));
    entries.push(("status", text(value.state().as_str())));
    push_optional(&mut entries, "stopReason", value.state().stop_reason().map(text));
    push_optional(&mut entries, "errorMessage", value.state().error_message().map(text));
    map(entries)
}

fn tool_transcript_item(value: &ToolTranscriptItem) -> CborValue {
    let mut entries = vec![
        ("id", text(value.id())),
        ("role", text("tool")),
        ("toolCallId", text(value.tool_call_id())),
        ("toolName", text(value.tool_name())),
        ("input", json_value(value.input())),
        (
            "content",
            CborValue::Array(value.content().iter().map(tool_content).collect()),
        ),
    ];
    push_optional(&mut entries, "details", value.details().map(json_value));
    push_optional(&mut entries, "usage", value.usage().map(usage));
    entries.push(("timestamp", unsigned(value.timestamp())));
    entries.push(("status", text(value.state().as_str())));
    entries.push(("isError", boolean(value.is_error())));
    map(entries)
}

fn transcript_item(value: &TranscriptItem) -> CborValue {
    match value {
        TranscriptItem::User(value) => user_transcript_item(value),
        TranscriptItem::Assistant(value) => assistant_transcript_item(value),
        TranscriptItem::Tool(value) => tool_transcript_item(value),
    }
}

fn transcript_progress(value: &TranscriptProgress) -> CborValue {
    match value {
        TranscriptProgress::ItemStarted { item } => {
            map(vec![("type", text("item_started")), ("item", transcript_item(item))])
        }
        TranscriptProgress::AssistantDelta {
            message_id,
            content_index,
            kind,
            delta,
        } => map(vec![
            ("type", text("assistant_delta")),
            ("messageId", text(message_id)),
            ("contentIndex", unsigned(*content_index)),
            ("kind", text(kind.as_str())),
            ("delta", text(delta)),
        ]),
        TranscriptProgress::ItemUpdated { item } => {
            map(vec![("type", text("item_updated")), ("item", updated_item(item))])
        }
        TranscriptProgress::ItemFinished { item } => {
            map(vec![("type", text("item_finished")), ("item", finished_item(item))])
        }
    }
}

fn updated_item(value: &UpdatedTranscriptItem) -> CborValue {
    match value {
        UpdatedTranscriptItem::Assistant(value) => assistant_transcript_item(value),
        UpdatedTranscriptItem::Tool(value) => tool_transcript_item(value),
    }
}

fn finished_item(value: &FinishedTranscriptItem) -> CborValue {
    match value {
        FinishedTranscriptItem::Assistant(value) => assistant_transcript_item(value),
        FinishedTranscriptItem::Tool(value) => tool_transcript_item(value),
    }
}

fn session_metadata(value: &SessionMetadata) -> CborValue {
    let mut entries = vec![("id", text(value.id())), ("createdAt", unsigned(value.created_at()))];
    push_optional(&mut entries, "updatedAt", value.updated_at().map(unsigned));
    push_optional(&mut entries, "parentSessionId", value.parent_session_id().map(text));
    push_optional(&mut entries, "sessionName", value.session_name().map(text));
    push_optional(&mut entries, "cwd", value.cwd().map(text));
    map(entries)
}

fn session_snapshot(value: &SessionSnapshot) -> CborValue {
    let mut entries = vec![("id", text(value.id()))];
    push_optional(&mut entries, "name", value.name().map(text));
    entries.extend([
        ("cwd", text(value.cwd())),
        ("createdAt", unsigned(value.created_at())),
        ("updatedAt", unsigned(value.updated_at())),
        ("phase", text(value.phase().as_str())),
        ("model", model_ref(value.model())),
        ("thinkingLevel", text(value.thinking_level().as_str())),
        ("attached", boolean(value.attached())),
        ("locked", boolean(value.locked())),
        ("revision", unsigned(value.revision())),
        (
            "transcript",
            CborValue::Array(value.transcript().iter().map(transcript_item).collect()),
        ),
        (
            "queuedSteer",
            CborValue::Array(value.queued_steer().iter().map(user_transcript_item).collect()),
        ),
        ("queuedSteerCount", unsigned(value.queued_steer_count())),
    ]);
    map(entries)
}

fn server_snapshot(value: &ServerSnapshot) -> CborValue {
    map(vec![
        ("serverId", text(value.server_id())),
        ("protocolVersion", unsigned(value.protocol_version())),
        ("revision", unsigned(value.revision())),
        (
            "sessions",
            CborValue::Array(value.sessions().iter().map(session_metadata).collect()),
        ),
        (
            "models",
            CborValue::Array(value.models().iter().map(model_metadata).collect()),
        ),
    ])
}

fn protocol_error(value: &ProtocolError) -> CborValue {
    let mut entries = vec![
        ("code", text(value.code().as_str())),
        ("message", text(value.message())),
    ];
    push_optional(&mut entries, "details", value.details().map(json_value));
    map(entries)
}

fn command_result(value: &CommandResult) -> CborValue {
    match value {
        CommandResult::List(result) => map(vec![
            ("command", text("list")),
            (
                "sessions",
                CborValue::Array(result.sessions().iter().map(session_metadata).collect()),
            ),
        ]),
        CommandResult::Detach(result) => map(vec![
            ("command", text("detach")),
            ("sessionId", text(result.session_id())),
        ]),
        CommandResult::Create(session) => session_result("create", session),
        CommandResult::Attach(session) => session_result("attach", session),
        CommandResult::Prompt(session) => session_result("prompt", session),
        CommandResult::Steer(session) => session_result("steer", session),
        CommandResult::Abort(session) => session_result("abort", session),
        CommandResult::SetModel(session) => session_result("set_model", session),
        CommandResult::SetThinking(session) => session_result("set_thinking", session),
    }
}

fn session_result(name: &str, session: &SessionSnapshot) -> CborValue {
    map(vec![("command", text(name)), ("session", session_snapshot(session))])
}

fn server_event(value: &ServerEvent) -> CborValue {
    match value {
        ServerEvent::ServerSnapshot(snapshot) => map(vec![
            ("type", text("server_snapshot")),
            ("snapshot", server_snapshot(snapshot)),
        ]),
        ServerEvent::SessionSnapshot(snapshot) => map(vec![
            ("type", text("session_snapshot")),
            ("snapshot", session_snapshot(snapshot)),
        ]),
        ServerEvent::SessionProgress(progress) => map(vec![
            ("type", text("session_progress")),
            ("sessionId", text(progress.session_id())),
            ("progress", transcript_progress(progress.progress())),
        ]),
        ServerEvent::SessionRemoved(removed) => map(vec![
            ("type", text("session_removed")),
            ("sessionId", text(removed.session_id())),
        ]),
    }
}

fn server_hello(value: &ServerHello) -> CborValue {
    map(vec![
        ("type", text("hello")),
        ("version", unsigned(PROTOCOL_VERSION)),
        ("connectionId", text(value.connection_id())),
        ("snapshot", server_snapshot(value.snapshot())),
    ])
}

fn server_hello_error(value: &ServerHelloError) -> CborValue {
    map(vec![
        ("type", text("hello_error")),
        ("error", protocol_error(value.error())),
    ])
}

fn response_envelope(value: &ResponseEnvelope) -> CborValue {
    match value {
        ResponseEnvelope::Success(response) => map(vec![
            ("type", text("response")),
            ("id", text(response.id())),
            ("ok", boolean(true)),
            ("result", command_result(response.result())),
        ]),
        ResponseEnvelope::Error(response) => map(vec![
            ("type", text("response")),
            ("id", text(response.id())),
            ("ok", boolean(false)),
            ("error", protocol_error(response.error())),
        ]),
    }
}

fn event_envelope(value: &EventEnvelope) -> CborValue {
    map(vec![("type", text("event")), ("event", server_event(value.event()))])
}

fn json_value(value: &JsonValue) -> CborValue {
    match value {
        JsonValue::Null => CborValue::Null,
        JsonValue::Bool(value) => boolean(*value),
        JsonValue::Number(value) => number(*value),
        JsonValue::String(value) => text(value),
        JsonValue::Array(values) => CborValue::Array(values.iter().map(json_value).collect()),
        JsonValue::Object(entries) => CborValue::Map(
            entries
                .iter()
                .map(|(key, value)| (key.clone(), json_value(value)))
                .collect(),
        ),
    }
}

fn map(entries: Vec<(&str, CborValue)>) -> CborValue {
    CborValue::Map(
        entries
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect(),
    )
}

fn push_optional(entries: &mut Vec<(&'static str, CborValue)>, key: &'static str, value: Option<CborValue>) {
    if let Some(value) = value {
        entries.push((key, value));
    }
}

fn text(value: &str) -> CborValue {
    CborValue::Text(value.to_string())
}

fn unsigned(value: u64) -> CborValue {
    number(value as f64)
}

fn number(value: f64) -> CborValue {
    CborValue::Number(value)
}

fn boolean(value: bool) -> CborValue {
    CborValue::Bool(value)
}
