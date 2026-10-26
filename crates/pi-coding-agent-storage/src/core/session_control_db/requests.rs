//! Architect and supervisor request queues.

use pi_js_compat::js_json::JsValue as Value;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use super::{ControlDbError, with_control_db, with_immediate_transaction};
use pi_js_compat::js_json::{self, JsObject, JsString, JsValue};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchitectRequestStatus {
    Pending,
    Claimed,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchitectRequest {
    pub id: i64,
    pub sender_session_id: JsString,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_cwd: Option<JsString>,
    pub body: JsString,
    pub status: ArchitectRequestStatus,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claimed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claim_token: Option<JsString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostArchitectRequestInput {
    pub sender_session_id: JsString,
    pub project_cwd: JsString,
    pub body: JsString,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupervisorRequestKind {
    ApprovalReview,
    GoalCompletionReview,
    GoalIdleReview,
    GoalSetReview,
    SupervisorAdvisory,
}

impl SupervisorRequestKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::ApprovalReview => "approval_review",
            Self::GoalCompletionReview => "goal_completion_review",
            Self::GoalIdleReview => "goal_idle_review",
            Self::GoalSetReview => "goal_set_review",
            Self::SupervisorAdvisory => "supervisor_advisory",
        }
    }

    fn parse(value: &str) -> Self {
        match value {
            "approval_review" => Self::ApprovalReview,
            "goal_completion_review" => Self::GoalCompletionReview,
            "goal_idle_review" => Self::GoalIdleReview,
            "goal_set_review" => Self::GoalSetReview,
            _ => Self::SupervisorAdvisory,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupervisorRequestStatus {
    Pending,
    Claimed,
    Completed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupervisorResponse {
    Approve { reason: JsString },
    Reject { reason: JsString },
    Complete { reason: JsString },
    Continue { instructions: JsString, reason: JsString },
    Pause { reason: JsString },
    Wait { reason: JsString },
    Set { objective: JsString, reason: JsString },
    Advisory { answer: JsString },
    Error { reason: JsString },
}

impl<'de> Deserialize<'de> for SupervisorResponse {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = JsValue::deserialize(deserializer)?;
        let text = |key: &str| {
            value
                .get(key)
                .and_then(JsValue::as_string)
                .cloned()
                .ok_or_else(|| serde::de::Error::custom(format!("expected supervisor response string {key}")))
        };
        let kind = text("kind")?.to_utf8().map_err(serde::de::Error::custom)?;
        Ok(match kind.as_str() {
            "approve" => Self::Approve {
                reason: text("reason")?,
            },
            "reject" => Self::Reject {
                reason: text("reason")?,
            },
            "complete" => Self::Complete {
                reason: text("reason")?,
            },
            "continue" => Self::Continue {
                instructions: text("instructions")?,
                reason: text("reason")?,
            },
            "pause" => Self::Pause {
                reason: text("reason")?,
            },
            "wait" => Self::Wait {
                reason: text("reason")?,
            },
            "set" => Self::Set {
                objective: text("objective")?,
                reason: text("reason")?,
            },
            "advisory" => Self::Advisory {
                answer: text("answer")?,
            },
            "error" => Self::Error {
                reason: text("reason")?,
            },
            _ => return Err(serde::de::Error::custom("unknown supervisor response kind")),
        })
    }
}

impl Serialize for SupervisorResponse {
    /// Serializes in the TypeScript object-literal key order for each decision branch.
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;

        let mut map = serializer.serialize_map(None)?;
        match self {
            Self::Approve { reason } => {
                map.serialize_entry("kind", "approve")?;
                map.serialize_entry("reason", reason)?;
            }
            Self::Reject { reason } => {
                map.serialize_entry("kind", "reject")?;
                map.serialize_entry("reason", reason)?;
            }
            Self::Complete { reason } => {
                map.serialize_entry("kind", "complete")?;
                map.serialize_entry("reason", reason)?;
            }
            Self::Continue { instructions, reason } => {
                map.serialize_entry("instructions", instructions)?;
                map.serialize_entry("kind", "continue")?;
                map.serialize_entry("reason", reason)?;
            }
            Self::Pause { reason } => {
                map.serialize_entry("kind", "pause")?;
                map.serialize_entry("reason", reason)?;
            }
            Self::Wait { reason } => {
                map.serialize_entry("kind", "wait")?;
                map.serialize_entry("reason", reason)?;
            }
            Self::Set { objective, reason } => {
                map.serialize_entry("kind", "set")?;
                map.serialize_entry("objective", objective)?;
                map.serialize_entry("reason", reason)?;
            }
            Self::Advisory { answer } => {
                map.serialize_entry("answer", answer)?;
                map.serialize_entry("kind", "advisory")?;
            }
            Self::Error { reason } => {
                map.serialize_entry("kind", "error")?;
                map.serialize_entry("reason", reason)?;
            }
        }
        map.end()
    }
}

impl SupervisorResponse {
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Approve { .. } => "approve",
            Self::Reject { .. } => "reject",
            Self::Complete { .. } => "complete",
            Self::Continue { .. } => "continue",
            Self::Pause { .. } => "pause",
            Self::Wait { .. } => "wait",
            Self::Set { .. } => "set",
            Self::Advisory { .. } => "advisory",
            Self::Error { .. } => "error",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupervisorRequest {
    pub id: i64,
    pub sender_session_id: JsString,
    pub project_id: JsString,
    pub kind: SupervisorRequestKind,
    #[serde(
        serialize_with = "js_json::serialize_object",
        deserialize_with = "js_json::deserialize_object"
    )]
    pub payload: JsObject,
    pub deadline_at: String,
    pub status: SupervisorRequestStatus,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claimed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claim_token: Option<JsString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<JsValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostSupervisorRequestInput {
    pub sender_session_id: JsString,
    pub project_id: JsString,
    pub kind: SupervisorRequestKind,
    #[serde(
        serialize_with = "js_json::serialize_object",
        deserialize_with = "js_json::deserialize_object"
    )]
    pub payload: JsObject,
    pub deadline_at: String,
}

type ArchitectRequestRow = (
    i64,
    JsString,
    Option<JsString>,
    JsString,
    String,
    String,
    Option<String>,
    Option<JsString>,
    Option<String>,
);

fn architect_request_from_row(row: ArchitectRequestRow) -> ArchitectRequest {
    ArchitectRequest {
        id: row.0,
        sender_session_id: row.1,
        project_cwd: row.2,
        body: row.3,
        status: parse_architect_status(&row.4),
        created_at: row.5,
        claimed_at: row.6,
        claim_token: row.7,
        completed_at: row.8,
    }
}

fn parse_architect_status(value: &str) -> ArchitectRequestStatus {
    match value {
        "pending" => ArchitectRequestStatus::Pending,
        "claimed" => ArchitectRequestStatus::Claimed,
        _ => ArchitectRequestStatus::Completed,
    }
}

const ARCHITECT_REQUEST_COLUMNS: &str =
    "id, sender_session_id, project_cwd, body, status, created_at, claimed_at, claim_token, completed_at";

/// `postArchitectRequest`.
pub fn post_architect_request(
    control_db_path: impl Into<JsString>,
    input: &PostArchitectRequestInput,
) -> Result<i64, ControlDbError> {
    let control_db_path = control_db_path.into();
    let body = input.body.trim();
    if body.is_empty() {
        return Err(ControlDbError::new("Architect request body must be non-empty"));
    }
    with_control_db(&control_db_path, |db| {
        let now = crate::utils::iso_time::now_iso_string();
        db.execute(
            "
				INSERT INTO architect_requests (sender_session_id, project_cwd, body, status, created_at)
				VALUES (?, ?, ?, 'pending', ?)
				",
            params![input.sender_session_id, input.project_cwd, body, now],
        )?;
        Ok(db.last_insert_rowid())
    })
}

/// `listPendingArchitectRequests`.
pub fn list_pending_architect_requests(
    control_db_path: impl Into<JsString>,
    limit: i64,
) -> Result<Vec<ArchitectRequest>, ControlDbError> {
    let control_db_path = control_db_path.into();
    with_control_db(&control_db_path, |db| {
        let mut statement = db.prepare(&format!(
            "SELECT {ARCHITECT_REQUEST_COLUMNS}
					 FROM architect_requests
					 WHERE status = 'pending'
					 ORDER BY id ASC
					 LIMIT ?"
        ))?;
        let rows = statement
            .query_map(params![limit.max(1)], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows.into_iter().map(architect_request_from_row).collect())
    })
}

/// `claimPendingArchitectRequests`.
pub fn claim_pending_architect_requests(
    control_db_path: impl Into<JsString>,
    claim_token: &JsString,
    limit: i64,
) -> Result<Vec<ArchitectRequest>, ControlDbError> {
    let control_db_path = control_db_path.into();
    let now_iso = crate::utils::iso_time::now_iso_string();
    let claim_limit = limit.max(1);
    with_control_db(&control_db_path, |db| {
        if !has_architect_claim_work(db, &now_iso)? {
            return Ok(Vec::new());
        }
        let rows = with_immediate_transaction(db, |db| {
            claim_pending_architect_requests_in_transaction(db, claim_token, &now_iso, claim_limit)
        })?;
        Ok(rows
            .into_iter()
            .map(|mut row| {
                row.4 = "claimed".to_owned();
                row.6 = Some(now_iso.clone());
                row.7 = Some(claim_token.clone());
                architect_request_from_row(row)
            })
            .collect())
    })
}

fn claim_pending_architect_requests_in_transaction(
    db: &Connection,
    claim_token: &JsString,
    now_iso: &str,
    claim_limit: i64,
) -> Result<Vec<ArchitectRequestRow>, ControlDbError> {
    db.execute(
        "UPDATE architect_requests
		 SET status = 'pending', claimed_at = NULL, claim_token = NULL
		 WHERE status = 'claimed' AND julianday(claimed_at) < julianday(?, '-2 minutes')",
        params![now_iso],
    )?;
    let mut statement = db.prepare(&format!(
        "SELECT {ARCHITECT_REQUEST_COLUMNS}
			 FROM architect_requests
			 WHERE status = 'pending'
			 ORDER BY id ASC
			 LIMIT ?"
    ))?;
    let rows = statement
        .query_map(params![claim_limit], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
                row.get(8)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for row in &rows {
        db.execute(
            "UPDATE architect_requests
		 SET status = 'claimed', claimed_at = ?, claim_token = ?
		 WHERE id = ? AND status = 'pending'",
            params![now_iso, claim_token, row.0],
        )?;
    }
    Ok(rows)
}

fn has_architect_claim_work(db: &Connection, now_iso: &str) -> Result<bool, ControlDbError> {
    Ok(db
        .query_row(
            "SELECT 1 FROM architect_requests
				 WHERE status = 'pending'
				 OR (status = 'claimed' AND julianday(claimed_at) < julianday(?, '-2 minutes'))
				 LIMIT 1",
            params![now_iso],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

/// `renewArchitectRequestClaims`.
pub fn renew_architect_request_claims(
    control_db_path: impl Into<JsString>,
    request_ids: &[i64],
    claim_token: &JsString,
) -> Result<(), ControlDbError> {
    let control_db_path = control_db_path.into();
    let unique_request_ids: Vec<i64> = {
        let mut seen = std::collections::HashSet::new();
        request_ids.iter().copied().filter(|id| seen.insert(*id)).collect()
    };
    if unique_request_ids.is_empty() {
        return Ok(());
    }
    let request_ids_json = js_json::stringify_lossless(&Value::Array(
        unique_request_ids.iter().map(|id| Value::Number(*id as f64)).collect(),
    ));
    let now = crate::utils::iso_time::now_iso_string();
    with_control_db(&control_db_path, |db| {
        with_immediate_transaction(db, |db| {
            db.execute(
                "UPDATE architect_requests
				 SET claimed_at = ?
				 WHERE id IN (SELECT CAST(value AS INTEGER) FROM json_each(?))
				 AND status = 'claimed' AND claim_token = ?",
                params![now, request_ids_json, claim_token],
            )?;
            let lost: Option<i64> = db
                .query_row(
                    "SELECT CAST(request_ids.value AS INTEGER) AS id
					 FROM json_each(?) AS request_ids
					 LEFT JOIN architect_requests AS requests
					 ON requests.id = CAST(request_ids.value AS INTEGER)
					 WHERE requests.status IS NULL
					 OR (requests.status <> 'completed'
						AND (requests.status <> 'claimed' OR requests.claim_token <> ?))
					 ORDER BY CAST(request_ids.key AS INTEGER)
					 LIMIT 1",
                    params![request_ids_json, claim_token],
                    |row| row.get(0),
                )
                .optional()?;
            if let Some(lost) = lost {
                return Err(ControlDbError::new(format!("Architect request claim lost: {lost}")));
            }
            Ok(())
        })
    })
}

/// `completeArchitectRequest`.
pub fn complete_architect_request(
    control_db_path: impl Into<JsString>,
    request_id: i64,
    claim_token: &JsString,
    sender_session_id: Option<&JsString>,
) -> Result<(), ControlDbError> {
    let control_db_path = control_db_path.into();
    with_control_db(&control_db_path, |db| {
        db.execute(
            "UPDATE architect_requests
			 SET status = 'completed', completed_at = ?
			 WHERE id = ? AND status = 'claimed' AND claim_token = ?
			   AND (? IS NULL OR sender_session_id = ?)",
            params![
                crate::utils::iso_time::now_iso_string(),
                request_id,
                claim_token,
                sender_session_id,
                sender_session_id
            ],
        )?;
        Ok(())
    })
}

type SupervisorRequestRow = (
    i64,
    JsString,
    JsString,
    String,
    String,
    String,
    String,
    String,
    Option<String>,
    Option<JsString>,
    Option<String>,
    Option<String>,
);

fn supervisor_request_from_row(row: SupervisorRequestRow) -> Result<SupervisorRequest, ControlDbError> {
    let payload = js_json::parse_lossless(&row.4).map_err(|error| ControlDbError::new(error.to_string()))?;
    let JsValue::Object(payload) = payload else {
        return Err(ControlDbError::new(format!(
            "Invalid persisted JSON at supervisor request {}",
            row.0
        )));
    };
    let response = match row.11 {
        None => None,
        Some(response_json) => {
            Some(js_json::parse_lossless(&response_json).map_err(|error| ControlDbError::new(error.to_string()))?)
        }
    };
    Ok(SupervisorRequest {
        id: row.0,
        sender_session_id: row.1,
        project_id: row.2,
        kind: SupervisorRequestKind::parse(&row.3),
        payload,
        deadline_at: row.5,
        status: parse_supervisor_status(&row.6),
        created_at: row.7,
        claimed_at: row.8,
        claim_token: row.9,
        completed_at: row.10,
        response,
    })
}

/// `cancelSupervisorRequest`: sender-fenced cancellation clears claim ownership.
pub fn cancel_supervisor_request(
    control_db_path: impl Into<JsString>,
    request_id: i64,
    sender_session_id: &JsString,
    reason: &str,
) -> Result<bool, ControlDbError> {
    let control_db_path = control_db_path.into();
    with_control_db(&control_db_path, |db| {
        with_immediate_transaction(db, |db| {
            let response = js_json::stringify_lossless(&pi_js_compat::js_json!({"kind":"error","reason":reason}));
            Ok(db.execute("UPDATE supervisor_requests SET status = 'cancelled', completed_at = ?, claim_token = NULL, response_json = ?
            WHERE id = ? AND sender_session_id = ? AND status IN ('pending','claimed')",params![crate::utils::iso_time::now_iso_string(),response,request_id,sender_session_id])? == 1)
        })
    })
}

fn parse_supervisor_status(value: &str) -> SupervisorRequestStatus {
    match value {
        "pending" => SupervisorRequestStatus::Pending,
        "claimed" => SupervisorRequestStatus::Claimed,
        "cancelled" => SupervisorRequestStatus::Cancelled,
        _ => SupervisorRequestStatus::Completed,
    }
}

const SUPERVISOR_REQUEST_COLUMNS: &str = "id, sender_session_id, project_id, kind, payload_json, deadline_at,
	status, created_at, claimed_at, claim_token, completed_at, response_json";

fn query_supervisor_request_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SupervisorRequestRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
        row.get(9)?,
        row.get(10)?,
        row.get(11)?,
    ))
}

/// `postSupervisorRequest`.
pub fn post_supervisor_request(
    control_db_path: impl Into<JsString>,
    input: &PostSupervisorRequestInput,
) -> Result<i64, ControlDbError> {
    let control_db_path = control_db_path.into();
    with_control_db(&control_db_path, |db| {
        db.execute(
            "INSERT INTO supervisor_requests
				 (sender_session_id, project_id, kind, payload_json, deadline_at, status, created_at)
				 VALUES (?, ?, ?, ?, ?, 'pending', ?)",
            params![
                input.sender_session_id,
                input.project_id,
                input.kind.as_str(),
                js_json::stringify_lossless(&JsValue::Object(input.payload.clone())),
                input.deadline_at,
                crate::utils::iso_time::now_iso_string()
            ],
        )?;
        Ok(db.last_insert_rowid())
    })
}

/// `readSupervisorRequest`.
pub fn read_supervisor_request(
    control_db_path: impl Into<JsString>,
    request_id: i64,
) -> Result<Option<SupervisorRequest>, ControlDbError> {
    let control_db_path = control_db_path.into();
    with_control_db(&control_db_path, |db| {
        let row = db
            .query_row(
                &format!("SELECT {SUPERVISOR_REQUEST_COLUMNS} FROM supervisor_requests WHERE id = ?"),
                params![request_id],
                query_supervisor_request_row,
            )
            .optional()?;
        row.map(supervisor_request_from_row).transpose()
    })
}

/// `claimNextSupervisorRequest`.
pub fn claim_next_supervisor_request(
    control_db_path: impl Into<JsString>,
    claim_token: &JsString,
) -> Result<Option<SupervisorRequest>, ControlDbError> {
    let control_db_path = control_db_path.into();
    with_control_db(&control_db_path, |db| {
        if read_next_supervisor_request_to_claim(db)?.is_none() {
            return Ok(None);
        }
        let claimed_at = crate::utils::iso_time::now_iso_string();
        let row = with_immediate_transaction(db, |db| {
            claim_next_supervisor_request_in_transaction(db, claim_token, &claimed_at)
        })?;
        row.map(supervisor_request_from_row).transpose()
    })
}

fn claim_next_supervisor_request_in_transaction(
    db: &Connection,
    claim_token: &JsString,
    claimed_at: &str,
) -> Result<Option<SupervisorRequestRow>, ControlDbError> {
    let Some(row) = read_next_supervisor_request_to_claim(db)? else {
        return Ok(None);
    };
    let row = db
        .query_row(
            &format!(
                "UPDATE supervisor_requests
			 SET status = 'claimed', claimed_at = ?, claim_token = ?
			 WHERE id = ? AND status = 'pending'
			 RETURNING {SUPERVISOR_REQUEST_COLUMNS}"
            ),
            params![claimed_at, claim_token, row.0],
            query_supervisor_request_row,
        )
        .optional()?;
    Ok(row)
}

fn read_next_supervisor_request_to_claim(db: &Connection) -> Result<Option<SupervisorRequestRow>, ControlDbError> {
    let active_claim: Option<i64> = db
        .query_row(
            "SELECT 1 FROM supervisor_requests WHERE status = 'claimed' LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if active_claim.is_some() {
        return Ok(None);
    }
    Ok(db
        .query_row(
            &format!(
                "SELECT {SUPERVISOR_REQUEST_COLUMNS}
			 FROM supervisor_requests
			 WHERE status = 'pending'
			 ORDER BY CASE kind
				 WHEN 'approval_review' THEN 0
				 WHEN 'supervisor_advisory' THEN 2
				 ELSE 1
			 END, id ASC
			 LIMIT 1"
            ),
            [],
            query_supervisor_request_row,
        )
        .optional()?)
}

/// `recoverSupervisorRequests`.
pub fn recover_supervisor_requests(control_db_path: impl Into<JsString>) -> Result<(), ControlDbError> {
    let control_db_path = control_db_path.into();
    let now_iso = crate::utils::iso_time::now_iso_string();
    with_control_db(&control_db_path, |db| {
        if !has_supervisor_recovery_work(db, &now_iso)? {
            return Ok(());
        }
        let error_response = js_json::stringify_lossless(&pi_js_compat::js_json!({
            "kind": "error",
            "reason": "Supervisor request deadline expired",
        }));
        with_immediate_transaction(db, |db| {
            db.execute(
                "UPDATE supervisor_requests
				 SET status = 'completed', completed_at = ?, response_json = ?, claim_token = NULL
				 WHERE status IN ('pending', 'claimed') AND deadline_at <= ?",
                params![now_iso, error_response, now_iso],
            )?;
            db.execute(
                "UPDATE supervisor_requests
				 SET status = 'pending', claimed_at = NULL, claim_token = NULL
				 WHERE status = 'claimed'",
                [],
            )?;
            Ok(())
        })
    })
}

fn has_supervisor_recovery_work(db: &Connection, now_iso: &str) -> Result<bool, ControlDbError> {
    Ok(db
        .query_row(
            "SELECT 1 FROM supervisor_requests
				 WHERE status = 'claimed' OR (status = 'pending' AND deadline_at <= ?)
				 LIMIT 1",
            params![now_iso],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

/// `hasPendingSupervisorApprovalRequest`.
pub fn has_pending_supervisor_approval_request(control_db_path: impl Into<JsString>) -> Result<bool, ControlDbError> {
    let control_db_path = control_db_path.into();
    with_control_db(&control_db_path, |db| {
        Ok(db
            .query_row(
                "SELECT 1 FROM supervisor_requests WHERE status = 'pending' AND kind = 'approval_review' LIMIT 1",
                [],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    })
}

/// `requeueSupervisorRequest`.
pub fn requeue_supervisor_request(
    control_db_path: impl Into<JsString>,
    request_id: i64,
    claim_token: &JsString,
) -> Result<(), ControlDbError> {
    let control_db_path = control_db_path.into();
    with_control_db(&control_db_path, |db| {
        let changed = db.execute(
            "UPDATE supervisor_requests
				 SET status = 'pending', claimed_at = NULL, claim_token = NULL
				 WHERE id = ? AND status = 'claimed' AND claim_token = ?",
            params![request_id, claim_token],
        )?;
        if changed != 1 {
            return Err(ControlDbError::new(format!(
                "Supervisor request claim lost: {request_id}"
            )));
        }
        Ok(())
    })
}

fn is_supervisor_response_valid_for_request(request_kind: SupervisorRequestKind, response_kind: &str) -> bool {
    if response_kind == "error" {
        return true;
    }
    if request_kind == SupervisorRequestKind::ApprovalReview {
        return response_kind == "approve" || response_kind == "reject";
    }
    if request_kind == SupervisorRequestKind::GoalSetReview {
        return response_kind == "set";
    }
    if request_kind == SupervisorRequestKind::SupervisorAdvisory {
        return response_kind == "advisory";
    }
    matches!(response_kind, "complete" | "continue" | "pause" | "wait")
}

/// `completeSupervisorRequest`.
pub fn complete_supervisor_request(
    control_db_path: impl Into<JsString>,
    request_id: i64,
    claim_token: &JsString,
    response: &JsValue,
) -> Result<(), ControlDbError> {
    let control_db_path = control_db_path.into();
    with_control_db(&control_db_path, |db| {
        let request_kind: Option<String> = db
            .query_row(
                "SELECT kind FROM supervisor_requests WHERE id = ? AND status = 'claimed' AND claim_token = ?",
                params![request_id, claim_token],
                |row| row.get(0),
            )
            .optional()?;
        let Some(request_kind) = request_kind else {
            return Err(ControlDbError::new(format!(
                "Supervisor request claim lost: {request_id}"
            )));
        };
        let request_kind = SupervisorRequestKind::parse(&request_kind);
        let response_kind = response
            .get("kind")
            .and_then(JsValue::as_string)
            .ok_or_else(|| ControlDbError::new("expected supervisor response string kind"))?
            .to_utf8()
            .map_err(|error| ControlDbError::new(error.to_string()))?;
        if !is_supervisor_response_valid_for_request(request_kind, &response_kind) {
            return Err(ControlDbError::new(format!(
                "Invalid Supervisor response kind {} for {}",
                response_kind,
                request_kind.as_str()
            )));
        }
        let response_json = js_json::stringify_lossless(response);
        let changed = db.execute(
            "UPDATE supervisor_requests
				 SET status = 'completed', completed_at = ?, response_json = ?
				 WHERE id = ? AND status = 'claimed' AND claim_token = ?",
            params![
                crate::utils::iso_time::now_iso_string(),
                response_json,
                request_id,
                claim_token
            ],
        )?;
        if changed != 1 {
            return Err(ControlDbError::new(format!(
                "Supervisor request claim lost: {request_id}"
            )));
        }
        Ok(())
    })
}
