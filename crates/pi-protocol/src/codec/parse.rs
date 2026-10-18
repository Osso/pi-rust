use std::collections::HashSet;
use std::str::FromStr;

use crate::CborValue;
use crate::cbor::is_safe_integer;
use crate::schemas::*;

type Parsed<T> = Option<T>;

pub(super) fn client_message(value: &CborValue) -> Parsed<ClientMessage> {
    match text(required(object(value)?, "type")?)? {
        "hello" => client_hello(value).map(ClientMessage::Hello),
        "request" => request_envelope(value).map(ClientMessage::Request),
        _ => None,
    }
}

pub(super) fn server_message(value: &CborValue) -> Parsed<ServerMessage> {
    match text(required(object(value)?, "type")?)? {
        "hello" => server_hello(value).map(ServerMessage::Hello),
        "hello_error" => server_hello_error(value).map(ServerMessage::HelloError),
        "response" => response_envelope(value).map(ServerMessage::Response),
        "event" => event_envelope(value).map(ServerMessage::Event),
        _ => None,
    }
}

fn client_hello(value: &CborValue) -> Parsed<ClientHello> {
    let entries = object(value)?;
    strict_object(entries, &["type", "version"])?;
    if text(required(entries, "type")?)? != "hello" {
        return None;
    }
    Some(ClientHello::new(safe_u64(required(entries, "version")?)?))
}

fn request_envelope(value: &CborValue) -> Parsed<RequestEnvelope> {
    let entries = object(value)?;
    strict_object(entries, &["type", "id", "request"])?;
    if text(required(entries, "type")?)? != "request" {
        return None;
    }
    RequestEnvelope::new(
        non_empty_text(required(entries, "id")?)?,
        command(required(entries, "request")?)?,
    )
    .ok()
}

fn command(value: &CborValue) -> Parsed<Command> {
    let entries = object(value)?;
    match text(required(entries, "command")?)? {
        "list" => {
            strict_object(entries, &["command"])?;
            Some(Command::List(ListCommand::new()))
        }
        "create" => {
            strict_object(entries, &["command", "cwd", "name", "model", "thinkingLevel"])?;
            let cwd = optional_field(entries, "cwd", non_empty_text)?;
            let name = optional_field(entries, "name", text_string)?;
            let model = optional_field(entries, "model", model_ref)?;
            let thinking_level = optional_field(entries, "thinkingLevel", thinking_level)?;
            CreateCommand::new(CreateCommandInput {
                cwd,
                name,
                model,
                thinking_level,
            })
            .ok()
            .map(Command::Create)
        }
        "attach" => session_command(entries, "attach", AttachCommand::new).map(Command::Attach),
        "detach" => session_command(entries, "detach", DetachCommand::new).map(Command::Detach),
        "prompt" => prompt_command(entries, "prompt", PromptCommand::new).map(Command::Prompt),
        "steer" => prompt_command(entries, "steer", SteerCommand::new).map(Command::Steer),
        "abort" => session_command(entries, "abort", AbortCommand::new).map(Command::Abort),
        "set_model" => {
            strict_object(entries, &["command", "sessionId", "model"])?;
            SetModelCommand::new(
                non_empty_text(required(entries, "sessionId")?)?,
                model_ref(required(entries, "model")?)?,
            )
            .ok()
            .map(Command::SetModel)
        }
        "set_thinking" => {
            strict_object(entries, &["command", "sessionId", "thinkingLevel"])?;
            SetThinkingCommand::new(
                non_empty_text(required(entries, "sessionId")?)?,
                thinking_level(required(entries, "thinkingLevel")?)?,
            )
            .ok()
            .map(Command::SetThinking)
        }
        _ => None,
    }
}

fn session_command<T>(
    entries: &[(String, CborValue)],
    name: &str,
    create: impl FnOnce(String) -> Result<T, CommandError>,
) -> Parsed<T> {
    strict_object(entries, &["command", "sessionId"])?;
    if text(required(entries, "command")?)? != name {
        return None;
    }
    create(non_empty_text(required(entries, "sessionId")?)?).ok()
}

fn prompt_command<T>(
    entries: &[(String, CborValue)],
    name: &str,
    create: impl FnOnce(String, String) -> Result<T, CommandError>,
) -> Parsed<T> {
    strict_object(entries, &["command", "sessionId", "text"])?;
    if text(required(entries, "command")?)? != name {
        return None;
    }
    create(
        non_empty_text(required(entries, "sessionId")?)?,
        text_string(required(entries, "text")?)?,
    )
    .ok()
}

fn model_ref(value: &CborValue) -> Parsed<ModelRef> {
    let entries = object(value)?;
    strict_object(entries, &["provider", "id"])?;
    ModelRef::new(
        non_empty_text(required(entries, "provider")?)?,
        non_empty_text(required(entries, "id")?)?,
    )
    .ok()
}

fn model_cost(value: &CborValue) -> Parsed<ModelCost> {
    let entries = object(value)?;
    strict_object(entries, &["input", "output", "cacheRead", "cacheWrite"])?;
    ModelCost::new(
        non_negative_number(required(entries, "input")?)?,
        non_negative_number(required(entries, "output")?)?,
        non_negative_number(required(entries, "cacheRead")?)?,
        non_negative_number(required(entries, "cacheWrite")?)?,
    )
    .ok()
}

fn model_metadata(value: &CborValue) -> Parsed<ModelMetadata> {
    let entries = object(value)?;
    strict_object(
        entries,
        &[
            "provider",
            "id",
            "name",
            "api",
            "reasoning",
            "input",
            "contextWindow",
            "maxTokens",
            "cost",
            "supportedThinkingLevels",
            "authenticated",
        ],
    )?;
    let input = array(required(entries, "input")?)?
        .iter()
        .map(model_input)
        .collect::<Parsed<Vec<_>>>()?;
    let supported_thinking_levels = array(required(entries, "supportedThinkingLevels")?)?
        .iter()
        .map(thinking_level)
        .collect::<Parsed<Vec<_>>>()?;
    ModelMetadata::new(ModelMetadataInput {
        provider: non_empty_text(required(entries, "provider")?)?,
        id: non_empty_text(required(entries, "id")?)?,
        name: non_empty_text(required(entries, "name")?)?,
        api: non_empty_text(required(entries, "api")?)?,
        reasoning: boolean(required(entries, "reasoning")?)?,
        input,
        context_window: safe_u64(required(entries, "contextWindow")?)?,
        max_tokens: safe_u64(required(entries, "maxTokens")?)?,
        cost: model_cost(required(entries, "cost")?)?,
        supported_thinking_levels,
        authenticated: boolean(required(entries, "authenticated")?)?,
    })
    .ok()
}

fn model_input(value: &CborValue) -> Parsed<ModelInput> {
    match text(value)? {
        "text" => Some(ModelInput::Text),
        "image" => Some(ModelInput::Image),
        _ => None,
    }
}

fn thinking_level(value: &CborValue) -> Parsed<ThinkingLevel> {
    ThinkingLevel::from_str(text(value)?).ok()
}

fn session_phase(value: &CborValue) -> Parsed<SessionPhase> {
    SessionPhase::from_str(text(value)?).ok()
}

fn usage_cost(value: &CborValue) -> Parsed<UsageCost> {
    let entries = object(value)?;
    strict_object(entries, &["input", "output", "cacheRead", "cacheWrite", "total"])?;
    UsageCost::new(
        non_negative_number(required(entries, "input")?)?,
        non_negative_number(required(entries, "output")?)?,
        non_negative_number(required(entries, "cacheRead")?)?,
        non_negative_number(required(entries, "cacheWrite")?)?,
        non_negative_number(required(entries, "total")?)?,
    )
    .ok()
}

fn usage(value: &CborValue) -> Parsed<Usage> {
    let entries = object(value)?;
    strict_object(
        entries,
        &[
            "input",
            "output",
            "cacheRead",
            "cacheWrite",
            "reasoning",
            "totalTokens",
            "cost",
        ],
    )?;
    Some(Usage::new(UsageInput {
        input: safe_u64(required(entries, "input")?)?,
        output: safe_u64(required(entries, "output")?)?,
        cache_read: safe_u64(required(entries, "cacheRead")?)?,
        cache_write: safe_u64(required(entries, "cacheWrite")?)?,
        reasoning: optional_field(entries, "reasoning", safe_u64)?,
        total_tokens: safe_u64(required(entries, "totalTokens")?)?,
        cost: usage_cost(required(entries, "cost")?)?,
    }))
}

fn text_content(value: &CborValue) -> Parsed<TextContent> {
    let entries = object(value)?;
    strict_object(entries, &["type", "text"])?;
    if text(required(entries, "type")?)? != "text" {
        return None;
    }
    Some(TextContent::new(text_string(required(entries, "text")?)?))
}

fn thinking_content(value: &CborValue) -> Parsed<ThinkingContent> {
    let entries = object(value)?;
    strict_object(entries, &["type", "thinking", "redacted"])?;
    if text(required(entries, "type")?)? != "thinking" {
        return None;
    }
    Some(ThinkingContent::new(
        text_string(required(entries, "thinking")?)?,
        optional_field(entries, "redacted", boolean)?,
    ))
}

fn image_content(value: &CborValue) -> Parsed<ImageContent> {
    let entries = object(value)?;
    strict_object(entries, &["type", "data", "mimeType"])?;
    if text(required(entries, "type")?)? != "image" {
        return None;
    }
    ImageContent::new(
        text_string(required(entries, "data")?)?,
        non_empty_text(required(entries, "mimeType")?)?,
    )
    .ok()
}

fn tool_call_content(value: &CborValue) -> Parsed<ToolCallContent> {
    let entries = object(value)?;
    strict_object(entries, &["type", "toolCallId", "toolName", "input"])?;
    if text(required(entries, "type")?)? != "toolCall" {
        return None;
    }
    ToolCallContent::new(
        non_empty_text(required(entries, "toolCallId")?)?,
        non_empty_text(required(entries, "toolName")?)?,
        json_value(required(entries, "input")?)?,
    )
    .ok()
}

fn user_content(value: &CborValue) -> Parsed<UserContent> {
    if let Some(content) = text_content(value) {
        return Some(UserContent::Text(content));
    }
    image_content(value).map(UserContent::Image)
}

fn assistant_content(value: &CborValue) -> Parsed<AssistantContent> {
    if let Some(content) = text_content(value) {
        return Some(AssistantContent::Text(content));
    }
    if let Some(content) = thinking_content(value) {
        return Some(AssistantContent::Thinking(content));
    }
    tool_call_content(value).map(AssistantContent::ToolCall)
}

fn tool_content(value: &CborValue) -> Parsed<ToolContent> {
    if let Some(content) = text_content(value) {
        return Some(ToolContent::Text(content));
    }
    image_content(value).map(ToolContent::Image)
}

fn user_transcript_item(value: &CborValue) -> Parsed<UserTranscriptItem> {
    let entries = object(value)?;
    strict_object(entries, &["id", "role", "content", "timestamp"])?;
    if text(required(entries, "role")?)? != "user" {
        return None;
    }
    let content = array(required(entries, "content")?)?
        .iter()
        .map(user_content)
        .collect::<Parsed<Vec<_>>>()?;
    UserTranscriptItem::new(
        non_empty_text(required(entries, "id")?)?,
        content,
        safe_u64(required(entries, "timestamp")?)?,
    )
    .ok()
}

fn assistant_transcript_item(value: &CborValue) -> Parsed<AssistantTranscriptItem> {
    let entries = object(value)?;
    let status = text(required(entries, "status")?)?;
    strict_object(entries, assistant_fields(status)?)?;
    if text(required(entries, "role")?)? != "assistant" {
        return None;
    }
    let content = array(required(entries, "content")?)?
        .iter()
        .map(assistant_content)
        .collect::<Parsed<Vec<_>>>()?;
    AssistantTranscriptItem::new(AssistantTranscriptItemInput {
        id: non_empty_text(required(entries, "id")?)?,
        content,
        model: model_ref(required(entries, "model")?)?,
        response_model: optional_field(entries, "responseModel", non_empty_text)?,
        usage: optional_field(entries, "usage", usage)?,
        timestamp: safe_u64(required(entries, "timestamp")?)?,
        state: assistant_state(entries, status)?,
    })
    .ok()
}

fn assistant_fields(status: &str) -> Parsed<&'static [&'static str]> {
    const SHARED: &[&str] = &[
        "id",
        "role",
        "content",
        "model",
        "responseModel",
        "usage",
        "timestamp",
        "status",
    ];
    match status {
        "streaming" => Some(SHARED),
        "complete" => Some(&[
            "id",
            "role",
            "content",
            "model",
            "responseModel",
            "usage",
            "timestamp",
            "status",
            "stopReason",
        ]),
        "error" | "aborted" => Some(&[
            "id",
            "role",
            "content",
            "model",
            "responseModel",
            "usage",
            "timestamp",
            "status",
            "stopReason",
            "errorMessage",
        ]),
        _ => None,
    }
}

fn assistant_state(entries: &[(String, CborValue)], status: &str) -> Parsed<AssistantTranscriptState> {
    match status {
        "streaming" => Some(AssistantTranscriptState::Streaming),
        "complete" => Some(AssistantTranscriptState::Complete {
            stop_reason: assistant_stop_reason(required(entries, "stopReason")?)?,
        }),
        "error" => {
            if text(required(entries, "stopReason")?)? != "error" {
                return None;
            }
            Some(AssistantTranscriptState::Error {
                error_message: optional_field(entries, "errorMessage", non_empty_text)?,
            })
        }
        "aborted" => {
            if text(required(entries, "stopReason")?)? != "aborted" {
                return None;
            }
            Some(AssistantTranscriptState::Aborted {
                error_message: optional_field(entries, "errorMessage", text_string)?,
            })
        }
        _ => None,
    }
}

fn assistant_stop_reason(value: &CborValue) -> Parsed<AssistantStopReason> {
    match text(value)? {
        "stop" => Some(AssistantStopReason::Stop),
        "length" => Some(AssistantStopReason::Length),
        "toolUse" => Some(AssistantStopReason::ToolUse),
        _ => None,
    }
}

fn tool_transcript_item(value: &CborValue) -> Parsed<ToolTranscriptItem> {
    let entries = object(value)?;
    strict_object(
        entries,
        &[
            "id",
            "role",
            "toolCallId",
            "toolName",
            "input",
            "content",
            "details",
            "usage",
            "timestamp",
            "status",
            "isError",
        ],
    )?;
    if text(required(entries, "role")?)? != "tool" {
        return None;
    }
    let content = array(required(entries, "content")?)?
        .iter()
        .map(tool_content)
        .collect::<Parsed<Vec<_>>>()?;
    ToolTranscriptItem::new(ToolTranscriptItemInput {
        id: non_empty_text(required(entries, "id")?)?,
        tool_call_id: non_empty_text(required(entries, "toolCallId")?)?,
        tool_name: non_empty_text(required(entries, "toolName")?)?,
        input: json_value(required(entries, "input")?)?,
        content,
        details: optional_field(entries, "details", json_value)?,
        usage: optional_field(entries, "usage", usage)?,
        timestamp: safe_u64(required(entries, "timestamp")?)?,
        state: tool_state(
            text(required(entries, "status")?)?,
            boolean(required(entries, "isError")?)?,
        )?,
    })
    .ok()
}

fn tool_state(status: &str, is_error: bool) -> Parsed<ToolTranscriptState> {
    match (status, is_error) {
        ("running", false) => Some(ToolTranscriptState::Running),
        ("complete", false) => Some(ToolTranscriptState::Complete),
        ("error", true) => Some(ToolTranscriptState::Error),
        _ => None,
    }
}

fn transcript_item(value: &CborValue) -> Parsed<TranscriptItem> {
    let entries = object(value)?;
    match text(required(entries, "role")?)? {
        "user" => user_transcript_item(value).map(TranscriptItem::User),
        "assistant" => assistant_transcript_item(value).map(TranscriptItem::Assistant),
        "tool" => tool_transcript_item(value).map(TranscriptItem::Tool),
        _ => None,
    }
}

fn transcript_progress(value: &CborValue) -> Parsed<TranscriptProgress> {
    let entries = object(value)?;
    match text(required(entries, "type")?)? {
        "item_started" => {
            strict_object(entries, &["type", "item"])?;
            Some(TranscriptProgress::item_started(transcript_item(required(
                entries, "item",
            )?)?))
        }
        "assistant_delta" => {
            strict_object(entries, &["type", "messageId", "contentIndex", "kind", "delta"])?;
            TranscriptProgress::assistant_delta(
                non_empty_text(required(entries, "messageId")?)?,
                safe_u64(required(entries, "contentIndex")?)?,
                transcript_delta_kind(required(entries, "kind")?)?,
                text_string(required(entries, "delta")?)?,
            )
            .ok()
        }
        "item_updated" => {
            strict_object(entries, &["type", "item"])?;
            TranscriptProgress::item_updated(transcript_item(required(entries, "item")?)?).ok()
        }
        "item_finished" => {
            strict_object(entries, &["type", "item"])?;
            TranscriptProgress::item_finished(transcript_item(required(entries, "item")?)?).ok()
        }
        _ => None,
    }
}

fn transcript_delta_kind(value: &CborValue) -> Parsed<TranscriptDeltaKind> {
    match text(value)? {
        "text" => Some(TranscriptDeltaKind::Text),
        "thinking" => Some(TranscriptDeltaKind::Thinking),
        "toolCall" => Some(TranscriptDeltaKind::ToolCall),
        _ => None,
    }
}

fn session_metadata(value: &CborValue) -> Parsed<SessionMetadata> {
    let entries = object(value)?;
    strict_object(
        entries,
        &["id", "createdAt", "updatedAt", "parentSessionId", "sessionName", "cwd"],
    )?;
    SessionMetadata::new(SessionMetadataInput {
        id: non_empty_text(required(entries, "id")?)?,
        created_at: safe_u64(required(entries, "createdAt")?)?,
        updated_at: optional_field(entries, "updatedAt", safe_u64)?,
        parent_session_id: optional_field(entries, "parentSessionId", non_empty_text)?,
        session_name: optional_field(entries, "sessionName", text_string)?,
        cwd: optional_field(entries, "cwd", non_empty_text)?,
    })
    .ok()
}

fn session_snapshot(value: &CborValue) -> Parsed<SessionSnapshot> {
    let entries = object(value)?;
    strict_object(
        entries,
        &[
            "id",
            "name",
            "cwd",
            "createdAt",
            "updatedAt",
            "phase",
            "model",
            "thinkingLevel",
            "attached",
            "locked",
            "revision",
            "transcript",
            "queuedSteer",
            "queuedSteerCount",
        ],
    )?;
    let transcript = array(required(entries, "transcript")?)?
        .iter()
        .map(transcript_item)
        .collect::<Parsed<Vec<_>>>()?;
    let queued_steer = array(required(entries, "queuedSteer")?)?
        .iter()
        .map(user_transcript_item)
        .collect::<Parsed<Vec<_>>>()?;
    SessionSnapshot::new(SessionSnapshotInput {
        id: non_empty_text(required(entries, "id")?)?,
        name: optional_field(entries, "name", text_string)?,
        cwd: non_empty_text(required(entries, "cwd")?)?,
        created_at: safe_u64(required(entries, "createdAt")?)?,
        updated_at: safe_u64(required(entries, "updatedAt")?)?,
        phase: session_phase(required(entries, "phase")?)?,
        model: model_ref(required(entries, "model")?)?,
        thinking_level: thinking_level(required(entries, "thinkingLevel")?)?,
        attached: boolean(required(entries, "attached")?)?,
        locked: boolean(required(entries, "locked")?)?,
        revision: safe_u64(required(entries, "revision")?)?,
        transcript,
        queued_steer,
        queued_steer_count: safe_u64(required(entries, "queuedSteerCount")?)?,
    })
    .ok()
}

fn server_snapshot(value: &CborValue) -> Parsed<ServerSnapshot> {
    let entries = object(value)?;
    strict_object(
        entries,
        &["serverId", "protocolVersion", "revision", "sessions", "models"],
    )?;
    if safe_u64(required(entries, "protocolVersion")?)? != PROTOCOL_VERSION {
        return None;
    }
    let sessions = array(required(entries, "sessions")?)?
        .iter()
        .map(session_metadata)
        .collect::<Parsed<Vec<_>>>()?;
    let models = array(required(entries, "models")?)?
        .iter()
        .map(model_metadata)
        .collect::<Parsed<Vec<_>>>()?;
    ServerSnapshot::new(
        non_empty_text(required(entries, "serverId")?)?,
        safe_u64(required(entries, "revision")?)?,
        sessions,
        models,
    )
    .ok()
}

fn protocol_error(value: &CborValue) -> Parsed<ProtocolError> {
    let entries = object(value)?;
    strict_object(entries, &["code", "message", "details"])?;
    Some(ProtocolError::new(
        ProtocolErrorCode::from_str(text(required(entries, "code")?)?).ok()?,
        text_string(required(entries, "message")?)?,
        optional_field(entries, "details", json_value)?,
    ))
}

fn command_result(value: &CborValue) -> Parsed<CommandResult> {
    let entries = object(value)?;
    match text(required(entries, "command")?)? {
        "list" => {
            strict_object(entries, &["command", "sessions"])?;
            let sessions = array(required(entries, "sessions")?)?
                .iter()
                .map(session_metadata)
                .collect::<Parsed<Vec<_>>>()?;
            Some(CommandResult::List(ListResult::new(sessions)))
        }
        "detach" => {
            strict_object(entries, &["command", "sessionId"])?;
            DetachResult::new(non_empty_text(required(entries, "sessionId")?)?)
                .ok()
                .map(CommandResult::Detach)
        }
        "create" | "attach" | "prompt" | "steer" | "abort" | "set_model" | "set_thinking" => {
            strict_object(entries, &["command", "session"])?;
            let session = session_snapshot(required(entries, "session")?)?;
            match text(required(entries, "command")?)? {
                "create" => Some(CommandResult::Create(session)),
                "attach" => Some(CommandResult::Attach(session)),
                "prompt" => Some(CommandResult::Prompt(session)),
                "steer" => Some(CommandResult::Steer(session)),
                "abort" => Some(CommandResult::Abort(session)),
                "set_model" => Some(CommandResult::SetModel(session)),
                "set_thinking" => Some(CommandResult::SetThinking(session)),
                _ => None,
            }
        }
        _ => None,
    }
}

fn server_event(value: &CborValue) -> Parsed<ServerEvent> {
    let entries = object(value)?;
    match text(required(entries, "type")?)? {
        "server_snapshot" => {
            strict_object(entries, &["type", "snapshot"])?;
            server_snapshot(required(entries, "snapshot")?).map(ServerEvent::ServerSnapshot)
        }
        "session_snapshot" => {
            strict_object(entries, &["type", "snapshot"])?;
            session_snapshot(required(entries, "snapshot")?).map(ServerEvent::SessionSnapshot)
        }
        "session_progress" => {
            strict_object(entries, &["type", "sessionId", "progress"])?;
            SessionProgress::new(
                non_empty_text(required(entries, "sessionId")?)?,
                transcript_progress(required(entries, "progress")?)?,
            )
            .ok()
            .map(ServerEvent::SessionProgress)
        }
        "session_removed" => {
            strict_object(entries, &["type", "sessionId"])?;
            SessionRemoved::new(non_empty_text(required(entries, "sessionId")?)?)
                .ok()
                .map(ServerEvent::SessionRemoved)
        }
        _ => None,
    }
}

fn server_hello(value: &CborValue) -> Parsed<ServerHello> {
    let entries = object(value)?;
    strict_object(entries, &["type", "version", "connectionId", "snapshot"])?;
    if text(required(entries, "type")?)? != "hello" {
        return None;
    }
    if safe_u64(required(entries, "version")?)? != PROTOCOL_VERSION {
        return None;
    }
    ServerHello::new(
        non_empty_text(required(entries, "connectionId")?)?,
        server_snapshot(required(entries, "snapshot")?)?,
    )
    .ok()
}

fn server_hello_error(value: &CborValue) -> Parsed<ServerHelloError> {
    let entries = object(value)?;
    strict_object(entries, &["type", "error"])?;
    if text(required(entries, "type")?)? != "hello_error" {
        return None;
    }
    Some(ServerHelloError::new(protocol_error(required(entries, "error")?)?))
}

fn response_envelope(value: &CborValue) -> Parsed<ResponseEnvelope> {
    let entries = object(value)?;
    if text(required(entries, "type")?)? != "response" {
        return None;
    }
    let id = non_empty_text(required(entries, "id")?)?;
    if boolean(required(entries, "ok")?)? {
        strict_object(entries, &["type", "id", "ok", "result"])?;
        SuccessResponse::new(id, command_result(required(entries, "result")?)?)
            .ok()
            .map(ResponseEnvelope::Success)
    } else {
        strict_object(entries, &["type", "id", "ok", "error"])?;
        ErrorResponse::new(id, protocol_error(required(entries, "error")?)?)
            .ok()
            .map(ResponseEnvelope::Error)
    }
}

fn event_envelope(value: &CborValue) -> Parsed<EventEnvelope> {
    let entries = object(value)?;
    strict_object(entries, &["type", "event"])?;
    if text(required(entries, "type")?)? != "event" {
        return None;
    }
    Some(EventEnvelope::new(server_event(required(entries, "event")?)?))
}

fn json_value(value: &CborValue) -> Parsed<JsonValue> {
    match value {
        CborValue::Null => Some(JsonValue::Null),
        CborValue::Bool(value) => Some(JsonValue::Bool(*value)),
        CborValue::Number(value) if value.is_finite() => Some(JsonValue::Number(*value)),
        CborValue::Number(_) => None,
        CborValue::Text(value) => Some(JsonValue::String(value.clone())),
        CborValue::Array(values) => values
            .iter()
            .map(json_value)
            .collect::<Parsed<Vec<_>>>()
            .map(JsonValue::Array),
        CborValue::Map(entries) => {
            let mut keys = HashSet::new();
            let mut values = Vec::new();
            for (key, value) in entries {
                if !keys.insert(key) {
                    return None;
                }
                values.push((key.clone(), json_value(value)?));
            }
            Some(JsonValue::Object(values))
        }
        CborValue::Bytes(_) => None,
    }
}

fn object(value: &CborValue) -> Parsed<&[(String, CborValue)]> {
    match value {
        CborValue::Map(entries) => Some(entries),
        _ => None,
    }
}

fn required<'a>(entries: &'a [(String, CborValue)], name: &str) -> Parsed<&'a CborValue> {
    entries.iter().find(|(key, _)| key == name).map(|(_, value)| value)
}

fn optional_field<T>(
    entries: &[(String, CborValue)],
    name: &str,
    parse: impl FnOnce(&CborValue) -> Parsed<T>,
) -> Parsed<Option<T>> {
    match required(entries, name) {
        Some(value) => parse(value).map(Some),
        None => Some(None),
    }
}

fn strict_object(entries: &[(String, CborValue)], allowed: &[&str]) -> Parsed<()> {
    let mut keys = HashSet::new();
    for (key, _) in entries {
        if !allowed.contains(&key.as_str()) || !keys.insert(key.as_str()) {
            return None;
        }
    }
    Some(())
}

fn text(value: &CborValue) -> Parsed<&str> {
    match value {
        CborValue::Text(value) => Some(value),
        _ => None,
    }
}

fn text_string(value: &CborValue) -> Parsed<String> {
    text(value).map(str::to_string)
}

fn non_empty_text(value: &CborValue) -> Parsed<String> {
    text(value).filter(|value| !value.is_empty()).map(str::to_string)
}

fn boolean(value: &CborValue) -> Parsed<bool> {
    match value {
        CborValue::Bool(value) => Some(*value),
        _ => None,
    }
}

fn array(value: &CborValue) -> Parsed<&[CborValue]> {
    match value {
        CborValue::Array(values) => Some(values),
        _ => None,
    }
}

fn safe_u64(value: &CborValue) -> Parsed<u64> {
    match value {
        CborValue::Number(value) if is_safe_integer(*value) && *value >= 0.0 => Some(*value as u64),
        _ => None,
    }
}

fn non_negative_number(value: &CborValue) -> Parsed<f64> {
    match value {
        CborValue::Number(value) if value.is_finite() && *value >= 0.0 => Some(*value),
        _ => None,
    }
}
