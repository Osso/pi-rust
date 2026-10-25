//! Runtime message markers (`core/runtime-message-markers.ts`).

use serde::{Deserialize, Serialize};

/// `RuntimeMessageMarker`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeMessageMarker {
    DuplicateTurnAssistant,
    DuplicateTurnGuard,
}

impl RuntimeMessageMarker {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DuplicateTurnAssistant => "duplicate_turn_assistant",
            Self::DuplicateTurnGuard => "duplicate_turn_guard",
        }
    }
}

/// `isDuplicateTurnAssistantMessage`.
///
/// `stored_marker` is the marker previously attached to the message (the
/// source's `WeakMap` lookup); `runtime_message_marker` is the marker carried by
/// the event. The Rust port has no object-identity map over owned messages, so
/// the owner passes the stored marker explicitly.
pub fn is_duplicate_turn_assistant_message(
    stored_marker: Option<RuntimeMessageMarker>,
    runtime_message_marker: Option<RuntimeMessageMarker>,
) -> bool {
    stored_marker == Some(RuntimeMessageMarker::DuplicateTurnAssistant)
        || runtime_message_marker == Some(RuntimeMessageMarker::DuplicateTurnAssistant)
}

/// `isDuplicateTurnGuardMessage`.
pub fn is_duplicate_turn_guard_message(
    stored_marker: Option<RuntimeMessageMarker>,
    runtime_message_marker: Option<RuntimeMessageMarker>,
) -> bool {
    stored_marker == Some(RuntimeMessageMarker::DuplicateTurnGuard)
        || runtime_message_marker == Some(RuntimeMessageMarker::DuplicateTurnGuard)
}
