//! Session health records and status derivation (`core/session-health.ts`).

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const SESSION_CHECK_STALE_MS: i64 = 5 * 60 * 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionCheckStatus {
    Ok,
    Dead,
    Timeout,
    Never,
}

impl SessionCheckStatus {
    /// Returns the exact TypeScript source literal.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Dead => "dead",
            Self::Timeout => "timeout",
            Self::Never => "never",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionLiveStatus {
    Running,
    Idle,
    Ended,
}

impl SessionLiveStatus {
    /// Returns the exact TypeScript source literal.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Idle => "idle",
            Self::Ended => "ended",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionHealthRecord {
    pub session_id: String,
    pub agent_generation: u64,
    pub pid: Option<u32>,
    pub last_active_at: Option<String>,
    pub last_checked_at: Option<String>,
    pub check_status: SessionCheckStatus,
    pub checked_generation: Option<u64>,
    pub check_latency_ms: Option<i64>,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionDirectoryEntry {
    pub session_id: String,
    pub session_path: String,
    pub pid: Option<u32>,
    pub status: SessionLiveStatus,
    pub started_at: String,
    pub last_active_at: Option<String>,
    pub name: Option<String>,
    pub goal: Option<String>,
    pub cwd: Option<String>,
    pub last_checked_at: Option<String>,
    pub check_status: SessionCheckStatus,
    pub check_latency_ms: Option<i64>,
    pub agent_generation: u64,
    pub checked_generation: Option<u64>,
    pub eligible_to_receive: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionBroadcastOutcome {
    Sent,
    SkippedDead,
    SkippedFilter,
    CheckFailed,
    SendFailed,
    Timeout,
}

impl SessionBroadcastOutcome {
    /// Returns the exact TypeScript source literal.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sent => "sent",
            Self::SkippedDead => "skipped_dead",
            Self::SkippedFilter => "skipped_filter",
            Self::CheckFailed => "check_failed",
            Self::SendFailed => "send_failed",
            Self::Timeout => "timeout",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionBroadcastResult {
    pub session_id: String,
    pub outcome: SessionBroadcastOutcome,
    pub check_status: SessionCheckStatus,
    pub error: Option<String>,
}

pub fn empty_session_health(session_id: impl Into<String>, now_iso: &str) -> SessionHealthRecord {
    SessionHealthRecord {
        session_id: session_id.into(),
        agent_generation: 0,
        pid: None,
        last_active_at: None,
        last_checked_at: None,
        check_status: SessionCheckStatus::Never,
        checked_generation: None,
        check_latency_ms: None,
        updated_at: now_iso.to_owned(),
    }
}

pub fn is_sticky_dead(health: &SessionHealthRecord) -> bool {
    health.check_status == SessionCheckStatus::Dead
        && health.checked_generation.is_some()
        && health.checked_generation == Some(health.agent_generation)
}

pub fn is_session_eligible_to_receive(health: &SessionHealthRecord) -> bool {
    health.check_status == SessionCheckStatus::Ok && !is_sticky_dead(health)
}

/// `last_active_at` is the value the caller resolved, not the record's own field.
pub fn derive_live_status(
    health: &SessionHealthRecord,
    last_active_at: Option<&str>,
    now_ms: i64,
    stale_ms: i64,
) -> SessionLiveStatus {
    if health.check_status != SessionCheckStatus::Ok || health.pid.is_none() {
        return SessionLiveStatus::Ended;
    }
    let last_active_ms = last_active_at
        .map(pi_js_compat::date::date_parse)
        .filter(|millis| !millis.is_nan())
        .map(|millis| millis as i64);
    match last_active_ms {
        Some(last_active_ms) if now_ms - last_active_ms <= stale_ms => SessionLiveStatus::Running,
        _ => SessionLiveStatus::Idle,
    }
}

/// `None` is both a missing and a non-string goal value.
pub fn parse_goal_objective(goal: Option<&Value>) -> Option<String> {
    let trimmed = goal?.as_str()?.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(objective) = json_objective(trimmed) {
        return Some(objective);
    }
    Some(trimmed.to_owned())
}

fn json_objective(trimmed: &str) -> Option<String> {
    let parsed: Value = serde_json::from_str(trimmed).ok()?;
    let objective = parsed.as_object()?.get("objective")?.as_str()?.trim();
    (!objective.is_empty()).then(|| objective.to_owned())
}

pub fn end_session_health(health: &SessionHealthRecord, now_iso: &str) -> SessionHealthRecord {
    SessionHealthRecord {
        pid: None,
        last_checked_at: Some(now_iso.to_owned()),
        check_status: SessionCheckStatus::Dead,
        checked_generation: Some(health.agent_generation),
        check_latency_ms: Some(0),
        updated_at: now_iso.to_owned(),
        ..health.clone()
    }
}

pub fn advance_generation_for_pid(health: &SessionHealthRecord, pid: u32, now_iso: &str) -> SessionHealthRecord {
    let same_process = health.pid == Some(pid) && health.agent_generation > 0;
    SessionHealthRecord {
        pid: Some(pid),
        agent_generation: if same_process {
            health.agent_generation
        } else {
            health.agent_generation + 1
        },
        last_active_at: Some(now_iso.to_owned()),
        // A new generation invalidates sticky death via the checkedGeneration mismatch.
        updated_at: now_iso.to_owned(),
        ..health.clone()
    }
}
