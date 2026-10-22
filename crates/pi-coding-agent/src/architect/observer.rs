//! Port of `packages/coding-agent/src/architect/observer.ts`.

use std::path::Path;
use std::sync::{Arc, Mutex};

use pi_ai::utils::uuid::random_uuid;
use pi_js_compat::collator::locale_compare_utf16;
use pi_js_compat::js_json::{JsString, encode_node_utf8};
use serde::{Deserialize, Serialize};

use crate::core::session_control_db::{
    ArchitectRequest, ControlDbError, claim_pending_architect_requests, complete_architect_request,
    renew_architect_request_claims,
};
use crate::core::sqlite::{configure_read_only_sqlite_database, create_read_only_sqlite_database};

pub const ARCHITECT_SESSION_ID: &str = "architect";

/// One live main session in the bounded observation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchitectSessionSnapshot {
    pub cwd: JsString,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal_json: Option<JsString>,
    pub id: JsString,
    pub is_subagent: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<JsString>,
}

/// Channel message carrying one architect request.
pub type ArchitectChannelMessage = ArchitectRequest;

/// Reason a bounded observation was produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchitectObservationReason {
    ArchitectRequest,
    SessionStateChanged,
}

/// Bounded structured observation handed to the Architect prompt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchitectObservation {
    /// Serialized first: the source object literal starts with `reason`.
    pub reason: ArchitectObservationReason,
    pub requests: Vec<ArchitectRequest>,
    pub sessions: Vec<ArchitectSessionSnapshot>,
}

/// Mutable observer state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ArchitectObserverState {
    pub last_observation: Option<ArchitectObservation>,
}

/// Read-only snapshot before request claiming.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ArchitectSnapshot {
    pub requests: Vec<ArchitectRequest>,
    pub sessions: Vec<ArchitectSessionSnapshot>,
}

pub type ArchitectSnapshotReader = Arc<dyn Fn() -> Result<ArchitectSnapshot, ControlDbError> + Send + Sync>;

/// `snapshotArchitectSessions`: copies and sorts by id.
pub fn snapshot_architect_sessions(metadata: &[ArchitectSessionSnapshot]) -> Vec<ArchitectSessionSnapshot> {
    let mut sessions = metadata.to_vec();
    sessions.sort_by(|left, right| locale_compare_utf16(left.id.units(), right.id.units()));
    sessions
}

/// `createArchitectObservation`.
pub fn create_architect_observation(
    previous: Option<&ArchitectObservation>,
    sessions: &[ArchitectSessionSnapshot],
    requests: &[ArchitectRequest],
) -> Option<ArchitectObservation> {
    if !requests.is_empty() {
        return Some(ArchitectObservation {
            reason: ArchitectObservationReason::ArchitectRequest,
            requests: requests.to_vec(),
            sessions: sessions.to_vec(),
        });
    }
    // `JSON.stringify(previous.sessions) === JSON.stringify(sessions)`: same fields in the same order.
    let unchanged = previous.is_some_and(|previous| previous.sessions == sessions);
    if unchanged {
        return None;
    }
    Some(ArchitectObservation {
        reason: ArchitectObservationReason::SessionStateChanged,
        requests: Vec::new(),
        sessions: sessions.to_vec(),
    })
}

/// `readArchitectSnapshot`: bounded live main sessions from `control.sqlite`.
pub fn read_architect_snapshot(control_db_path: &JsString) -> Result<ArchitectSnapshot, ControlDbError> {
    let fs_path = encode_node_utf8(control_db_path);
    if !Path::new(&fs_path).exists() {
        return Ok(ArchitectSnapshot::default());
    }
    let db = create_read_only_sqlite_database(&fs_path)?;
    configure_read_only_sqlite_database(&db, 5_000)?;
    let result = (|| {
        let mut statement = db.prepare(
            "WITH current_sessions AS (
				SELECT
					metadata.id,
					metadata.cwd,
					metadata.name,
					metadata.goal_json,
					metadata.is_subagent,
					ROW_NUMBER() OVER (
						PARTITION BY listener.pid
						ORDER BY listener.updated_at DESC, metadata.modified_at DESC,
							metadata.updated_at DESC, metadata.session_path DESC
					) AS row_number
				FROM session_metadata AS metadata
				INNER JOIN runtime_mailbox_listeners AS listener
					ON listener.recipient_session_id = metadata.id
					AND listener.recipient_agent_id_key = ''
				INNER JOIN session_health AS health
					ON health.session_id = metadata.id
					AND health.pid = listener.pid
				WHERE health.pid IS NOT NULL
					AND health.check_status = 'ok'
					AND health.checked_generation = health.agent_generation
					AND julianday(health.last_active_at) >= julianday('now', '-5 minutes')
					AND metadata.is_subagent = 0
					AND metadata.id <> ?
			)
			SELECT id, cwd, name, goal_json, is_subagent
			FROM current_sessions
			WHERE row_number = 1
			ORDER BY id ASC
			LIMIT 20",
        )?;
        let rows = statement
            .query_map([ARCHITECT_SESSION_ID], |row| {
                Ok(ArchitectSessionSnapshot {
                    cwd: row.get(1)?,
                    goal_json: row.get(3)?,
                    id: row.get(0)?,
                    is_subagent: row.get::<_, i64>(4)? == 1,
                    name: row.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ArchitectSnapshot {
            requests: Vec::new(),
            sessions: rows,
        })
    })();
    let _ = db.close();
    result
}

fn read_and_claim_architect_snapshot(
    control_db_path: &JsString,
    claim_token: &JsString,
) -> Result<ArchitectSnapshot, ControlDbError> {
    let snapshot = read_architect_snapshot(control_db_path)?;
    Ok(ArchitectSnapshot {
        requests: claim_pending_architect_requests(control_db_path.clone(), claim_token, 20)?,
        sessions: snapshot.sessions,
    })
}

/// `ArchitectObserver`.
pub struct ArchitectObserver {
    claim_token: JsString,
    control_db_path: JsString,
    read_snapshot: ArchitectSnapshotReader,
    state: Mutex<ArchitectObserverState>,
}

impl ArchitectObserver {
    pub fn new(control_db_path: &JsString, read_snapshot: Option<ArchitectSnapshotReader>) -> Self {
        let claim_token = JsString::from(random_uuid());
        let read_snapshot = read_snapshot.unwrap_or_else(|| {
            let control_db_path = control_db_path.clone();
            let claim_token = claim_token.clone();
            Arc::new(move || read_and_claim_architect_snapshot(&control_db_path, &claim_token))
        });
        Self {
            claim_token,
            control_db_path: control_db_path.clone(),
            read_snapshot,
            state: Mutex::new(ArchitectObserverState::default()),
        }
    }

    pub fn observe(&self) -> Result<Option<ArchitectObservation>, ControlDbError> {
        let snapshot = (self.read_snapshot)()?;
        let mut state = self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let observation = create_architect_observation(
            state.last_observation.as_ref(),
            &snapshot_architect_sessions(&snapshot.sessions),
            &snapshot.requests,
        );
        if let Some(observation) = &observation {
            state.last_observation = Some(observation.clone());
        }
        Ok(observation)
    }

    pub fn renew_requests(&self, request_ids: &[i64]) -> Result<(), ControlDbError> {
        renew_architect_request_claims(self.control_db_path.clone(), request_ids, &self.claim_token)
    }

    pub fn complete_request(&self, request_id: i64, sender_session_id: &JsString) -> Result<(), ControlDbError> {
        complete_architect_request(
            self.control_db_path.clone(),
            request_id,
            &self.claim_token,
            Some(sender_session_id),
        )
    }
}
