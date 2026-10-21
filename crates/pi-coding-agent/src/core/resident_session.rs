//! Predicate from frozen `core/resident-session.ts:8-14` only.
//! Preparation and live-process cleanup remain lifecycle-owner work.

use super::session_manager::SessionInfo;

pub fn is_resident_session(session: &SessionInfo) -> bool {
    matches!(session.id.as_str(), "supervisor" | "architect")
        || session
            .path
            .split(['/', '\\'])
            .any(|segment| matches!(segment, "supervisor-sessions" | "architect-sessions"))
}
