//! Port of `packages/coding-agent/src/client/transcript.ts`.
//!
//! Remote transcript projection: progress events are folded into a local view while the
//! authoritative snapshot stays untouched.

use indexmap::IndexMap;
use pi_protocol::{
    AssistantContent, FinishedTranscriptItem, JsonValue, SessionSnapshot, TextContent, ThinkingContent,
    ToolCallContent, TranscriptDeltaKind, TranscriptItem, TranscriptProgress, UpdatedTranscriptItem,
    UserTranscriptItem,
};
use serde_json::Value;

/// Snapshot plus transient progress items that have not reached the snapshot yet.
#[derive(Debug, Clone)]
pub struct TranscriptState {
    pub snapshot: SessionSnapshot,
    pub progress_items: IndexMap<String, TranscriptItem>,
    pub progress_order: Vec<String>,
    pub tool_call_buffers: IndexMap<String, String>,
}

fn parse_partial_tool_input(value: &str) -> JsonValue {
    match serde_json::from_str::<Value>(value) {
        Ok(parsed) => JsonValue::from_serde_json(&parsed),
        // Tool arguments are incomplete while streaming. Preserve their raw prefix until they
        // form valid JSON.
        Err(_) => JsonValue::String(value.to_owned()),
    }
}

/// `createTranscriptState`.
pub fn create_transcript_state(snapshot: &SessionSnapshot) -> TranscriptState {
    TranscriptState {
        snapshot: snapshot.clone(),
        progress_items: IndexMap::new(),
        progress_order: Vec::new(),
        tool_call_buffers: IndexMap::new(),
    }
}

/// `applyTranscriptSnapshot`: a lower revision for the same session is stale.
pub fn apply_transcript_snapshot(state: TranscriptState, snapshot: &SessionSnapshot) -> TranscriptState {
    if state.snapshot.id() == snapshot.id() && snapshot.revision() < state.snapshot.revision() {
        return state;
    }
    create_transcript_state(snapshot)
}

/// `applyTranscriptProgress`.
pub fn apply_transcript_progress(state: TranscriptState, progress: &TranscriptProgress) -> TranscriptState {
    match progress {
        TranscriptProgress::ItemStarted { item } => set_progress_item(state, item.clone()),
        TranscriptProgress::ItemUpdated { item } => set_progress_item(state, updated_item(item)),
        TranscriptProgress::ItemFinished { item } => {
            let finished = finished_item(item);
            let id = finished.id().to_owned();
            let mut tool_call_buffers = state.tool_call_buffers.clone();
            let prefix = format!("{id}:");
            tool_call_buffers.retain(|key, _| !key.starts_with(&prefix));
            set_progress_item(
                TranscriptState {
                    tool_call_buffers,
                    ..state
                },
                finished,
            )
        }
        TranscriptProgress::AssistantDelta {
            message_id,
            content_index,
            kind,
            delta,
        } => apply_assistant_delta(state, message_id, *content_index, *kind, delta),
    }
}

fn updated_item(item: &UpdatedTranscriptItem) -> TranscriptItem {
    match item {
        UpdatedTranscriptItem::Assistant(item) => TranscriptItem::Assistant(item.clone()),
        UpdatedTranscriptItem::Tool(item) => TranscriptItem::Tool(item.clone()),
    }
}

fn finished_item(item: &FinishedTranscriptItem) -> TranscriptItem {
    match item {
        FinishedTranscriptItem::Assistant(item) => TranscriptItem::Assistant(item.clone()),
        FinishedTranscriptItem::Tool(item) => TranscriptItem::Tool(item.clone()),
    }
}

fn apply_assistant_delta(
    mut state: TranscriptState,
    message_id: &str,
    content_index: u64,
    kind: TranscriptDeltaKind,
    delta: &str,
) -> TranscriptState {
    let item = state.progress_items.get(message_id).cloned().or_else(|| {
        state
            .snapshot
            .transcript()
            .iter()
            .find(|item| item.id() == message_id)
            .cloned()
    });
    let Some(TranscriptItem::Assistant(item)) = item else {
        return state;
    };
    let content = item
        .content()
        .iter()
        .enumerate()
        .map(|(index, part)| {
            if index as u64 != content_index {
                return part.clone();
            }
            apply_delta_part(&mut state, message_id, content_index, kind, delta, part)
        })
        .collect();
    set_progress_item(state, TranscriptItem::Assistant(item.replacing_content(content)))
}

fn apply_delta_part(
    state: &mut TranscriptState,
    message_id: &str,
    content_index: u64,
    kind: TranscriptDeltaKind,
    delta: &str,
    part: &AssistantContent,
) -> AssistantContent {
    match (kind, part) {
        (TranscriptDeltaKind::Text, AssistantContent::Text(text)) => {
            AssistantContent::Text(TextContent::new(format!("{}{delta}", text.text())))
        }
        (TranscriptDeltaKind::Thinking, AssistantContent::Thinking(thinking)) => AssistantContent::Thinking(
            ThinkingContent::new(format!("{}{delta}", thinking.thinking()), thinking.redacted()),
        ),
        (TranscriptDeltaKind::ToolCall, AssistantContent::ToolCall(tool_call)) => {
            let key = format!("{message_id}:{content_index}");
            let existing = state
                .tool_call_buffers
                .get(&key)
                .cloned()
                .unwrap_or_else(|| match tool_call.input() {
                    JsonValue::String(value) => value.clone(),
                    _ => String::new(),
                });
            let buffer = format!("{existing}{delta}");
            state.tool_call_buffers.insert(key, buffer.clone());
            AssistantContent::ToolCall(
                ToolCallContent::new(
                    tool_call.tool_call_id().to_owned(),
                    tool_call.tool_name().to_owned(),
                    parse_partial_tool_input(&buffer),
                )
                .expect("existing tool call identity stays valid"),
            )
        }
        _ => part.clone(),
    }
}

fn set_progress_item(mut state: TranscriptState, item: TranscriptItem) -> TranscriptState {
    let id = item.id().to_owned();
    if !state.progress_items.contains_key(&id) {
        state.progress_order.push(id.clone());
    }
    state.progress_items.insert(id, item);
    state
}

/// `selectTranscript`: snapshot order, then transient items, then queued steering.
pub fn select_transcript(state: &TranscriptState) -> Vec<TranscriptItem> {
    let mut transcript: Vec<TranscriptItem> = state
        .snapshot
        .transcript()
        .iter()
        .map(|item| {
            state
                .progress_items
                .get(item.id())
                .cloned()
                .unwrap_or_else(|| item.clone())
        })
        .collect();
    let mut ids: Vec<String> = transcript.iter().map(|item| item.id().to_owned()).collect();
    for id in &state.progress_order {
        if ids.contains(id) {
            continue;
        }
        if let Some(item) = state.progress_items.get(id) {
            transcript.push(item.clone());
            ids.push(id.clone());
        }
    }
    for item in state.snapshot.queued_steer() {
        if ids.contains(&item.id().to_owned()) {
            continue;
        }
        transcript.push(TranscriptItem::User(
            UserTranscriptItem::new(item.id().to_owned(), item.content().to_vec(), item.timestamp())
                .expect("existing queued steer item stays valid"),
        ));
        ids.push(item.id().to_owned());
    }
    transcript
}
