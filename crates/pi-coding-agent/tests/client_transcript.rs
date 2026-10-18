//! Behavioral port of `test/client/transcript.test.ts`.

use pi_coding_agent::client::transcript::{
    TranscriptState, apply_transcript_progress, apply_transcript_snapshot, create_transcript_state, select_transcript,
};
use pi_protocol::{
    AssistantContent, AssistantStopReason, AssistantTranscriptItem, AssistantTranscriptItemInput,
    AssistantTranscriptState, JsonValue, ModelRef, SessionPhase, SessionSnapshot, SessionSnapshotInput, TextContent,
    ThinkingLevel, ToolCallContent, ToolContent, ToolTranscriptItem, ToolTranscriptItemInput, ToolTranscriptState,
    TranscriptDeltaKind, TranscriptItem, TranscriptProgress, UserContent, UserTranscriptItem,
};

fn model() -> ModelRef {
    ModelRef::new("faux".to_owned(), "faux-1".to_owned()).unwrap()
}

fn assistant_item(
    id: &str,
    content: Vec<AssistantContent>,
    state: AssistantTranscriptState,
) -> AssistantTranscriptItem {
    AssistantTranscriptItem::new(AssistantTranscriptItemInput {
        id: id.to_owned(),
        content,
        model: model(),
        response_model: None,
        usage: None,
        timestamp: 1,
        state,
    })
    .unwrap()
}

fn text_part(text: &str) -> AssistantContent {
    AssistantContent::Text(TextContent::new(text.to_owned()))
}

fn tool_call_part(input: JsonValue) -> AssistantContent {
    AssistantContent::ToolCall(ToolCallContent::new("call-1".to_owned(), "bash".to_owned(), input).unwrap())
}

fn snapshot(revision: u64, text: &str) -> SessionSnapshot {
    SessionSnapshot::new(SessionSnapshotInput {
        id: "session-1".to_owned(),
        name: None,
        cwd: "/workspace".to_owned(),
        created_at: 1,
        updated_at: revision + 1,
        phase: SessionPhase::Turn,
        model: model(),
        thinking_level: ThinkingLevel::Off,
        attached: true,
        locked: true,
        revision,
        transcript: vec![TranscriptItem::Assistant(assistant_item(
            "assistant-1",
            vec![text_part(text)],
            AssistantTranscriptState::Streaming,
        ))],
        queued_steer: Vec::new(),
        queued_steer_count: 0,
    })
    .unwrap()
}

fn snapshot_with_item(revision: u64, item: TranscriptItem) -> SessionSnapshot {
    SessionSnapshot::new(SessionSnapshotInput {
        id: "session-1".to_owned(),
        name: None,
        cwd: "/workspace".to_owned(),
        created_at: 1,
        updated_at: revision + 1,
        phase: SessionPhase::Turn,
        model: model(),
        thinking_level: ThinkingLevel::Off,
        attached: true,
        locked: true,
        revision,
        transcript: vec![item],
        queued_steer: Vec::new(),
        queued_steer_count: 0,
    })
    .unwrap()
}

fn text_delta(state: TranscriptState, index: u64, delta: &str) -> TranscriptState {
    apply_transcript_progress(
        state,
        &TranscriptProgress::AssistantDelta {
            message_id: "assistant-1".to_owned(),
            content_index: index,
            kind: TranscriptDeltaKind::Text,
            delta: delta.to_owned(),
        },
    )
}

fn tool_delta(state: TranscriptState, index: u64, delta: &str) -> TranscriptState {
    apply_transcript_progress(
        state,
        &TranscriptProgress::AssistantDelta {
            message_id: "assistant-1".to_owned(),
            content_index: index,
            kind: TranscriptDeltaKind::ToolCall,
            delta: delta.to_owned(),
        },
    )
}

fn first_assistant_content(state: &TranscriptState) -> Vec<AssistantContent> {
    match &select_transcript(state)[0] {
        TranscriptItem::Assistant(item) => item.content().to_vec(),
        other => panic!("expected assistant item, got {}", other.role()),
    }
}

fn first_text(state: &TranscriptState) -> String {
    match &first_assistant_content(state)[0] {
        AssistantContent::Text(text) => text.text().to_owned(),
        other => panic!("expected text content, got {}", other.as_str()),
    }
}

#[test]
fn projects_progress_without_mutating_the_authoritative_snapshot() {
    let mut state = create_transcript_state(&snapshot(1, "saved"));
    state = text_delta(state, 0, " response");

    let snapshot_text = match &state.snapshot.transcript()[0] {
        TranscriptItem::Assistant(item) => match &item.content()[0] {
            AssistantContent::Text(text) => text.text(),
            other => panic!("expected text, got {}", other.as_str()),
        },
        other => panic!("expected assistant, got {}", other.role()),
    };
    assert_eq!(snapshot_text, "saved");
    assert_eq!(first_text(&state), "saved response");
}

#[test]
fn applies_streamed_tool_call_argument_deltas() {
    let mut state = create_transcript_state(&snapshot_with_item(
        1,
        TranscriptItem::Assistant(assistant_item(
            "assistant-1",
            vec![tool_call_part(JsonValue::Null)],
            AssistantTranscriptState::Streaming,
        )),
    ));
    state = tool_delta(state, 0, r#"{"command":"#);
    match &first_assistant_content(&state)[0] {
        AssistantContent::ToolCall(call) => assert_eq!(call.input(), &JsonValue::String(r#"{"command":"#.to_owned())),
        other => panic!("expected tool call, got {}", other.as_str()),
    }

    state = apply_transcript_progress(
        state,
        &TranscriptProgress::item_updated(TranscriptItem::Assistant(assistant_item(
            "assistant-1",
            vec![tool_call_part(JsonValue::Null)],
            AssistantTranscriptState::Streaming,
        )))
        .unwrap(),
    );
    state = tool_delta(state, 0, r#""pwd"}"#);
    match &first_assistant_content(&state)[0] {
        AssistantContent::ToolCall(call) => assert_eq!(
            call.input(),
            &JsonValue::Object(vec![("command".to_owned(), JsonValue::String("pwd".to_owned()))])
        ),
        other => panic!("expected tool call, got {}", other.as_str()),
    }
}

#[test]
fn appends_tool_call_deltas_to_a_partial_input_restored_from_a_snapshot() {
    let mut state = create_transcript_state(&snapshot_with_item(
        1,
        TranscriptItem::Assistant(assistant_item(
            "assistant-1",
            vec![tool_call_part(JsonValue::String(r#"{"command":"#.to_owned()))],
            AssistantTranscriptState::Streaming,
        )),
    ));
    state = tool_delta(state, 0, r#""pwd"}"#);

    match &first_assistant_content(&state)[0] {
        AssistantContent::ToolCall(call) => assert_eq!(
            call.input(),
            &JsonValue::Object(vec![("command".to_owned(), JsonValue::String("pwd".to_owned()))])
        ),
        other => panic!("expected tool call, got {}", other.as_str()),
    }
}

fn running_tool(content: Vec<ToolContent>) -> TranscriptItem {
    TranscriptItem::Tool(
        ToolTranscriptItem::new(ToolTranscriptItemInput {
            id: "tool-call-1".to_owned(),
            tool_call_id: "call-1".to_owned(),
            tool_name: "bash".to_owned(),
            input: JsonValue::Object(vec![("command".to_owned(), JsonValue::String("printf hi".to_owned()))]),
            content,
            details: None,
            usage: None,
            timestamp: 2,
            state: ToolTranscriptState::Running,
        })
        .unwrap(),
    )
}

#[test]
fn appends_transient_tool_progress_and_replaces_it_by_id() {
    let mut state = create_transcript_state(&snapshot(1, "saved"));
    state = apply_transcript_progress(state, &TranscriptProgress::item_started(running_tool(Vec::new())));
    let transcript = select_transcript(&state);
    assert_eq!(transcript.len(), 2);
    assert_eq!(transcript[1].id(), "tool-call-1");
    assert_eq!(transcript[1].role(), "tool");

    state = apply_transcript_progress(
        state,
        &TranscriptProgress::item_updated(running_tool(vec![ToolContent::Text(TextContent::new("hi".to_owned()))]))
            .unwrap(),
    );
    let transcript = select_transcript(&state);
    assert_eq!(transcript.len(), 2);
    let TranscriptItem::Tool(item) = &transcript[1] else {
        panic!("expected tool item");
    };
    assert_eq!(item.state(), ToolTranscriptState::Running);
    assert_eq!(item.content(), &[ToolContent::Text(TextContent::new("hi".to_owned()))]);
}

#[test]
fn resets_revision_history_when_the_same_session_runtime_is_reacquired() {
    let _state = create_transcript_state(&snapshot(50, "old runtime"));
    let state = create_transcript_state(&snapshot(0, "new runtime"));

    assert_eq!(state.snapshot.revision(), 0);
    assert_eq!(first_text(&state), "new runtime");
}

#[test]
fn accepts_a_lower_revision_when_switching_to_a_different_session() {
    let state = create_transcript_state(&snapshot(50, "old session"));
    let mut next = snapshot(0, "new session");
    next = SessionSnapshot::new(SessionSnapshotInput {
        id: "session-2".to_owned(),
        name: next.name().map(str::to_owned),
        cwd: next.cwd().to_owned(),
        created_at: next.created_at(),
        updated_at: next.updated_at(),
        phase: next.phase(),
        model: next.model().clone(),
        thinking_level: next.thinking_level(),
        attached: next.attached(),
        locked: next.locked(),
        revision: next.revision(),
        transcript: next.transcript().to_vec(),
        queued_steer: next.queued_steer().to_vec(),
        queued_steer_count: next.queued_steer_count(),
    })
    .unwrap();
    let state = apply_transcript_snapshot(state, &next);

    assert_eq!(state.snapshot.id(), "session-2");
    assert_eq!(first_text(&state), "new session");
}

#[test]
fn renders_accepted_steering_messages_from_authoritative_queued_state() {
    let mut base = snapshot(2, "saved");
    base = SessionSnapshot::new(SessionSnapshotInput {
        id: base.id().to_owned(),
        name: None,
        cwd: base.cwd().to_owned(),
        created_at: base.created_at(),
        updated_at: base.updated_at(),
        phase: base.phase(),
        model: base.model().clone(),
        thinking_level: base.thinking_level(),
        attached: base.attached(),
        locked: base.locked(),
        revision: base.revision(),
        transcript: base.transcript().to_vec(),
        queued_steer: vec![
            UserTranscriptItem::new(
                "user-steer".to_owned(),
                vec![UserContent::Text(TextContent::new("adjust the approach".to_owned()))],
                2,
            )
            .unwrap(),
        ],
        queued_steer_count: 1,
    })
    .unwrap();
    let state = create_transcript_state(&base);

    let transcript = select_transcript(&state);
    let TranscriptItem::User(item) = transcript.last().unwrap() else {
        panic!("expected user item");
    };
    assert_eq!(item.id(), "user-steer");
    assert_eq!(
        item.content(),
        &[UserContent::Text(TextContent::new("adjust the approach".to_owned()))]
    );
}

#[test]
fn a_newer_snapshot_is_authoritative_and_stale_snapshots_are_ignored() {
    let mut state = create_transcript_state(&snapshot(3, "new"));
    state = text_delta(state, 0, " transient");
    state = apply_transcript_snapshot(state, &snapshot(4, "authoritative"));
    state = apply_transcript_snapshot(state, &snapshot(2, "stale"));

    assert_eq!(state.snapshot.revision(), 4);
    assert_eq!(first_text(&state), "authoritative");
}

#[test]
fn finished_items_clear_their_tool_call_buffers() {
    let mut state = create_transcript_state(&snapshot_with_item(
        1,
        TranscriptItem::Assistant(assistant_item(
            "assistant-1",
            vec![tool_call_part(JsonValue::Null)],
            AssistantTranscriptState::Streaming,
        )),
    ));
    state = tool_delta(state, 0, r#"{"command":"#);
    assert_eq!(
        state.tool_call_buffers.get("assistant-1:0").map(String::as_str),
        Some(r#"{"command":"#)
    );

    let finished = assistant_item(
        "assistant-1",
        vec![tool_call_part(JsonValue::Null)],
        AssistantTranscriptState::Complete {
            stop_reason: AssistantStopReason::ToolUse,
        },
    );
    state = apply_transcript_progress(
        state,
        &TranscriptProgress::item_finished(TranscriptItem::Assistant(finished)).unwrap(),
    );

    assert!(state.tool_call_buffers.is_empty());
}
