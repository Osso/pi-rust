//! Session metadata, names, prompt history, and last-message storage.

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use super::{ControlDbError, with_control_db, with_immediate_transaction};
use crate::core::permissions::presets::SandboxProfileName;
use pi_js_compat::js_format;
use pi_js_compat::js_json::{self, JsString, JsValue, encode_node_utf8};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LastControlMessage {
    pub role: String,
    pub content: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionNameState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<JsString>,
    pub has_stored_value: bool,
}

/// `normalizeSessionName`: collapse CR/LF runs, JS-trim, and treat empty as unset.
pub fn normalize_session_name(name: Option<&JsString>) -> Option<JsString> {
    let mut normalized = JsString::default();
    let mut line_break_run = false;
    for unit in name?.units() {
        if *unit == u16::from(b'\r') || *unit == u16::from(b'\n') {
            line_break_run = true;
            continue;
        }
        if line_break_run {
            normalized.push_unit(u16::from(b' '));
            line_break_run = false;
        }
        normalized.push_unit(*unit);
    }
    let trimmed = normalized.trim();
    (!trimmed.is_empty()).then_some(trimmed)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMetadata {
    pub session_path: JsString,
    pub id: JsString,
    pub cwd: JsString,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<JsString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_session_path: Option<JsString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_archived: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal_json: Option<JsString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_subagent: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subagent_name: Option<JsString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<String>,
    pub created_at: String,
    pub modified_at: String,
    pub message_count: i64,
    pub first_message: JsString,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WritableSessionMetadata {
    pub session_path: JsString,
    pub id: JsString,
    pub cwd: JsString,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<JsString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_session_path: Option<JsString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_archived: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_subagent: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subagent_name: Option<JsString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<String>,
    pub created_at: JsString,
    pub modified_at: JsString,
    pub message_count: i64,
    pub first_message: JsString,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResumeSessionMetadataOptions {
    pub archived: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<JsString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_path_prefix: Option<JsString>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmptyMainSessionCandidate {
    pub id: JsString,
    pub session_path: JsString,
}

/// `writeSessionName`.
pub fn write_session_name(
    control_db_path: impl Into<JsString>,
    session_path: impl Into<JsString>,
    name: Option<&JsString>,
) -> Result<(), ControlDbError> {
    let session_path = session_path.into();
    let stored_name = normalize_session_name(name).unwrap_or_default();
    with_control_db(&control_db_path.into(), |db| {
        let changed = db.execute(
            "UPDATE session_metadata SET name = ?, updated_at = ? WHERE session_path = ?",
            params![
                encode_node_utf8(&stored_name),
                crate::utils::iso_time::now_iso_string(),
                encode_node_utf8(&session_path)
            ],
        )?;
        if changed != 1 {
            return Err(ControlDbError::new(
                JsString::from("Cannot update session name without metadata for ") + &session_path,
            ));
        }
        Ok(())
    })
}

/// `readSessionNameState`.
pub fn read_session_name_state(
    control_db_path: impl Into<JsString>,
    session_path: impl Into<JsString>,
) -> Result<SessionNameState, ControlDbError> {
    let session_path = session_path.into();
    with_control_db(&control_db_path.into(), |db| {
        let row: Option<Option<String>> = db
            .query_row(
                "SELECT name FROM session_metadata WHERE session_path = ?",
                params![encode_node_utf8(&session_path)],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?;
        match row {
            None | Some(None) => Ok(SessionNameState {
                name: None,
                has_stored_value: false,
            }),
            Some(Some(stored)) => Ok(SessionNameState {
                name: normalize_session_name(Some(&stored.into())),
                has_stored_value: true,
            }),
        }
    })
}

/// The plain and `.zst` aliases of a session path, as bind values.
fn session_path_aliases(session_path: &JsString) -> (String, String) {
    let plain = session_path
        .strip_suffix(".zst")
        .unwrap_or_else(|| session_path.clone());
    let archived = plain.clone() + ".zst";
    (encode_node_utf8(&plain), encode_node_utf8(&archived))
}

/// `listSubagentSessionPaths`: both archive aliases address the same parent.
pub fn list_subagent_session_paths(
    control_db_path: impl Into<JsString>,
    parent_session_path: impl Into<JsString>,
) -> Result<Vec<JsString>, ControlDbError> {
    let (plain, archived) = session_path_aliases(&parent_session_path.into());
    with_control_db(&control_db_path.into(), |db| {
        let mut statement = db.prepare(
            "SELECT session_path FROM session_metadata WHERE is_subagent = 1 AND parent_session_path IN (?, ?)",
        )?;
        Ok(statement
            .query_map(params![plain, archived], |row| {
                row.get::<_, String>(0).map(JsString::from)
            })?
            .collect::<Result<Vec<_>, _>>()?)
    })
}

/// `listDescendantSessionPaths`: recursive UNION also terminates parent cycles.
pub fn list_descendant_session_paths(
    control_db_path: impl Into<JsString>,
    parent_path: impl Into<JsString>,
) -> Result<Vec<JsString>, ControlDbError> {
    let (plain, archived) = session_path_aliases(&parent_path.into());
    with_control_db(&control_db_path.into(), |db| {
        let mut statement=db.prepare("WITH RECURSIVE descendants(session_path) AS (
            SELECT session_path FROM session_metadata WHERE is_subagent = 1 AND parent_session_path IN (?, ?)
            UNION
            SELECT child.session_path FROM descendants AS parent JOIN session_metadata AS child
            ON child.parent_session_path IN (
                CASE WHEN substr(parent.session_path,-4) = '.zst' THEN substr(parent.session_path,1,length(parent.session_path)-4) ELSE parent.session_path END,
                CASE WHEN substr(parent.session_path,-4) = '.zst' THEN parent.session_path ELSE parent.session_path || '.zst' END)
            WHERE child.is_subagent = 1)
            SELECT session_path FROM descendants WHERE session_path NOT IN (?, ?)")?;
        Ok(statement
            .query_map(params![plain, archived, plain, archived], |row| {
                row.get::<_, String>(0).map(JsString::from)
            })?
            .collect::<Result<Vec<_>, _>>()?)
    })
}

/// `removeSessionMetadata`.
pub fn remove_session_metadata(
    control_db_path: impl Into<JsString>,
    session_path: impl Into<JsString>,
) -> Result<(), ControlDbError> {
    remove_sessions_metadata(control_db_path, &[session_path.into()])
}

/// `removeSessionsMetadata`.
pub fn remove_sessions_metadata(
    control_db_path: impl Into<JsString>,
    paths: &[JsString],
) -> Result<(), ControlDbError> {
    if paths.is_empty() {
        return Ok(());
    }
    with_control_db(&control_db_path.into(), |db| {
        with_immediate_transaction(db, |db| {
            for path in paths.iter().map(encode_node_utf8) {
                db.execute(
                    "DELETE FROM session_sandbox_profiles WHERE session_path = ?",
                    params![path],
                )?;
                db.execute("DELETE FROM session_metadata WHERE session_path = ?", params![path])?;
            }
            Ok(())
        })
    })
}

/// `relocateSessionControlData`.
pub fn relocate_session_control_data(
    control_db_path: impl Into<JsString>,
    old_session_path: impl Into<JsString>,
    new_session_path: impl Into<JsString>,
) -> Result<(), ControlDbError> {
    let old_session_path = old_session_path.into();
    let new_session_path = new_session_path.into();
    if old_session_path == new_session_path {
        return Ok(());
    }
    with_control_db(&control_db_path.into(), |db| {
        let now = crate::utils::iso_time::now_iso_string();
        with_immediate_transaction(db, |db| {
            relocate_session_control_data_in_transaction(db, &old_session_path, &new_session_path, &now)
        })
    })
}

/// Rewrites every session-keyed row; raw paths are compared before binding.
fn relocate_session_control_data_in_transaction(
    db: &Connection,
    old_session_path: &JsString,
    new_session_path: &JsString,
    now: &str,
) -> Result<(), ControlDbError> {
    if old_session_path == new_session_path {
        return Ok(());
    }
    let old_session_path = &encode_node_utf8(old_session_path);
    let new_session_path = &encode_node_utf8(new_session_path);
    relocate_session_path_primary_key(db, "session_metadata", old_session_path, new_session_path, now)?;
    relocate_session_path_primary_key(db, "session_sandbox_profiles", old_session_path, new_session_path, now)?;
    relocate_multi_agent_session_rows(db, "multi_agent_agents", old_session_path, new_session_path, now)?;
    relocate_multi_agent_runtime_owners(db, old_session_path, new_session_path)?;
    relocate_multi_agent_session_rows(
        db,
        "multi_agent_terminal_outbox",
        old_session_path,
        new_session_path,
        now,
    )?;
    relocate_multi_agent_session_rows(
        db,
        "multi_agent_mailbox_messages",
        old_session_path,
        new_session_path,
        now,
    )?;
    relocate_multi_agent_counters(db, old_session_path, new_session_path, now)?;
    db.execute(
        "UPDATE runtime_mailbox_listeners SET session_path = ? WHERE session_path = ?",
        params![new_session_path, old_session_path],
    )?;
    Ok(())
}

/// `relocateAndArchiveSession`.
pub fn relocate_and_archive_session(
    control_db_path: impl Into<JsString>,
    old_session_path: impl Into<JsString>,
    new_session_path: impl Into<JsString>,
) -> Result<(), ControlDbError> {
    relocate_and_set_archive_state(
        &control_db_path.into(),
        &old_session_path.into(),
        &new_session_path.into(),
        true,
    )
}
/// `relocateAndUnarchiveSession`.
pub fn relocate_and_unarchive_session(
    control_db_path: impl Into<JsString>,
    old_session_path: impl Into<JsString>,
    new_session_path: impl Into<JsString>,
) -> Result<(), ControlDbError> {
    relocate_and_set_archive_state(
        &control_db_path.into(),
        &old_session_path.into(),
        &new_session_path.into(),
        false,
    )
}
fn relocate_and_set_archive_state(
    control_db_path: &JsString,
    old_session_path: &JsString,
    new_session_path: &JsString,
    archived: bool,
) -> Result<(), ControlDbError> {
    with_control_db(control_db_path, |db| {
        with_immediate_transaction(db, |db| {
            let now = crate::utils::iso_time::now_iso_string();
            relocate_session_control_data_in_transaction(db, old_session_path, new_session_path, &now)?;
            db.execute(
                "UPDATE session_metadata SET archived_at = ?, updated_at = ? WHERE session_path = ?",
                params![archived.then_some(&now), now, encode_node_utf8(new_session_path)],
            )?;
            Ok(())
        })
    })
}

fn relocate_session_path_primary_key(
    db: &Connection,
    table: &str,
    old_session_path: &str,
    new_session_path: &str,
    now: &str,
) -> Result<(), ControlDbError> {
    db.execute(
        &format!("DELETE FROM {table} WHERE session_path = ?"),
        params![new_session_path],
    )?;
    db.execute(
        &format!("UPDATE {table} SET session_path = ?, updated_at = ? WHERE session_path = ?"),
        params![new_session_path, now, old_session_path],
    )?;
    Ok(())
}

fn relocate_multi_agent_counters(
    db: &Connection,
    old_session_path: &str,
    new_session_path: &str,
    now: &str,
) -> Result<(), ControlDbError> {
    let old_counters: Option<(i64, i64)> = db
        .query_row(
            "SELECT next_agent_number, next_message_number FROM multi_agent_counters_v2 WHERE session_path = ?",
            params![old_session_path],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((old_agent, old_message)) = old_counters else {
        return Ok(());
    };
    let new_counters: Option<(i64, i64)> = db
        .query_row(
            "SELECT next_agent_number, next_message_number FROM multi_agent_counters_v2 WHERE session_path = ?",
            params![new_session_path],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let (new_agent, new_message) = new_counters.unwrap_or((1, 1));
    db.execute(
        "DELETE FROM multi_agent_counters_v2 WHERE session_path = ?",
        params![new_session_path],
    )?;
    db.execute(
        "UPDATE multi_agent_counters_v2
		 SET session_path = ?, next_agent_number = ?, next_message_number = ?, updated_at = ?
		 WHERE session_path = ?",
        params![
            new_session_path,
            old_agent.max(new_agent),
            old_message.max(new_message),
            now,
            old_session_path
        ],
    )?;
    Ok(())
}

fn relocate_multi_agent_runtime_owners(
    db: &Connection,
    old_session_path: &str,
    new_session_path: &str,
) -> Result<(), ControlDbError> {
    db.execute(
        "DELETE FROM multi_agent_runtime_owners WHERE session_path = ?",
        params![new_session_path],
    )?;
    db.execute(
        "UPDATE multi_agent_runtime_owners SET session_path = ? WHERE session_path = ?",
        params![new_session_path, old_session_path],
    )?;
    Ok(())
}

fn relocate_multi_agent_session_rows(
    db: &Connection,
    table: &str,
    old_session_path: &str,
    new_session_path: &str,
    now: &str,
) -> Result<(), ControlDbError> {
    db.execute(
        &format!("DELETE FROM {table} WHERE session_path = ?"),
        params![new_session_path],
    )?;
    db.execute(
        &format!("UPDATE {table} SET session_path = ?, updated_at = ? WHERE session_path = ?"),
        params![new_session_path, now, old_session_path],
    )?;
    Ok(())
}

struct SessionMetadataWriteValues {
    name: Option<JsString>,
    archived_at: Option<String>,
    is_subagent: bool,
    subagent_name: Option<JsString>,
    model_provider: Option<String>,
    model_id: Option<String>,
    thinking_level: Option<String>,
}

const UPSERT_SESSION_METADATA_SQL: &str = "
	INSERT INTO session_metadata (
		session_path,
		id,
		cwd,
		name,
		parent_session_path,
		archived_at,
		goal_json,
		is_subagent,
		subagent_name,
		model_provider,
		model_id,
		thinking_level,
		created_at,
		modified_at,
		message_count,
		first_message,
		updated_at
	)
	VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
	ON CONFLICT(session_path) DO UPDATE SET
		id = excluded.id,
		cwd = excluded.cwd,
		name = excluded.name,
		parent_session_path = excluded.parent_session_path,
		archived_at = excluded.archived_at,
		is_subagent = excluded.is_subagent,
		subagent_name = excluded.subagent_name,
		model_provider = excluded.model_provider,
		model_id = excluded.model_id,
		thinking_level = excluded.thinking_level,
		created_at = excluded.created_at,
		modified_at = excluded.modified_at,
		message_count = excluded.message_count,
		first_message = excluded.first_message,
		updated_at = excluded.updated_at
";

/// `writeSessionMetadata`.
pub fn write_session_metadata(
    control_db_path: impl Into<JsString>,
    metadata: &WritableSessionMetadata,
) -> Result<(), ControlDbError> {
    with_control_db(&control_db_path.into(), |db| {
        let values = read_session_metadata_write_values(db, metadata)?;
        write_session_metadata_row(db, metadata, &values)
    })
}

fn read_session_metadata_write_values(
    db: &Connection,
    metadata: &WritableSessionMetadata,
) -> Result<SessionMetadataWriteValues, ControlDbError> {
    type PreservedSessionMetadataRow = (
        Option<String>,
        Option<String>,
        Option<String>,
        i64,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    );
    let preserved: Option<PreservedSessionMetadataRow> = db
        .query_row(
            "SELECT name, archived_at, goal_json, is_subagent, subagent_name, model_provider, model_id, thinking_level
			 FROM session_metadata WHERE session_path = ?",
            params![encode_node_utf8(&metadata.session_path)],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                ))
            },
        )
        .optional()?;
    let preserved_is_subagent = preserved.as_ref().is_some_and(|values| values.3 == 1);
    Ok(SessionMetadataWriteValues {
        name: match metadata.name.as_ref() {
            Some(name) => Some(normalize_session_name(Some(name)).unwrap_or_default()),
            None => preserved
                .as_ref()
                .and_then(|values| values.0.clone())
                .map(JsString::from),
        },
        archived_at: metadata
            .archived_at
            .clone()
            .or_else(|| preserved.as_ref().and_then(|values| values.1.clone())),
        is_subagent: metadata.is_subagent.unwrap_or(preserved_is_subagent),
        subagent_name: metadata.subagent_name.clone().or_else(|| {
            preserved
                .as_ref()
                .and_then(|values| values.4.clone())
                .map(JsString::from)
        }),
        model_provider: metadata
            .model_provider
            .clone()
            .or_else(|| preserved.as_ref().and_then(|values| values.5.clone())),
        model_id: metadata
            .model_id
            .clone()
            .or_else(|| preserved.as_ref().and_then(|values| values.6.clone())),
        thinking_level: metadata
            .thinking_level
            .clone()
            .or_else(|| preserved.as_ref().and_then(|values| values.7.clone())),
    })
}

fn write_session_metadata_row(
    db: &Connection,
    metadata: &WritableSessionMetadata,
    values: &SessionMetadataWriteValues,
) -> Result<(), ControlDbError> {
    db.execute(
        UPSERT_SESSION_METADATA_SQL,
        params![
            encode_node_utf8(&metadata.session_path),
            encode_node_utf8(&metadata.id),
            encode_node_utf8(&metadata.cwd),
            values.name.as_ref().map(encode_node_utf8),
            metadata.parent_session_path.as_ref().map(encode_node_utf8),
            values.archived_at,
            Option::<String>::None,
            if values.is_subagent { 1 } else { 0 },
            values.subagent_name.as_ref().map(encode_node_utf8),
            values.model_provider,
            values.model_id,
            values.thinking_level,
            encode_node_utf8(&metadata.created_at),
            encode_node_utf8(&metadata.modified_at),
            metadata.message_count,
            encode_node_utf8(&metadata.first_message),
            crate::utils::iso_time::now_iso_string()
        ],
    )?;
    Ok(())
}

/// `writeSessionModel`.
pub fn write_session_model(
    control_db_path: impl Into<JsString>,
    session_path: impl Into<JsString>,
    provider: &str,
    model_id: &str,
) -> Result<(), ControlDbError> {
    let session_path = encode_node_utf8(&session_path.into());
    with_control_db(&control_db_path.into(), |db| {
        db.execute(
            "
			UPDATE session_metadata
			SET model_provider = ?, model_id = ?, updated_at = ?
			WHERE session_path = ?
			",
            params![
                provider,
                model_id,
                crate::utils::iso_time::now_iso_string(),
                session_path
            ],
        )?;
        Ok(())
    })
}

/// `writeSessionThinkingLevel`.
pub fn write_session_thinking_level(
    control_db_path: impl Into<JsString>,
    session_path: impl Into<JsString>,
    thinking_level: &str,
) -> Result<(), ControlDbError> {
    let session_path = encode_node_utf8(&session_path.into());
    with_control_db(&control_db_path.into(), |db| {
        db.execute(
            "
			UPDATE session_metadata
			SET thinking_level = ?, updated_at = ?
			WHERE session_path = ?
			",
            params![thinking_level, crate::utils::iso_time::now_iso_string(), session_path],
        )?;
        Ok(())
    })
}

fn assert_session_metadata_identity(
    db: &Connection,
    session_path: &JsString,
    session_id: &JsString,
) -> Result<(), ControlDbError> {
    let row: Option<String> = db
        .query_row(
            "SELECT id FROM session_metadata WHERE session_path = ?",
            params![encode_node_utf8(session_path)],
            |row| row.get(0),
        )
        .optional()?;
    let Some(row) = row else {
        return Err(ControlDbError::new(js_format!(
            "Session metadata is missing for {}",
            session_path
        )));
    };
    if *session_id != row {
        return Err(ControlDbError::new(js_format!(
            "Session identity mismatch for {}: expected {}, received {}",
            session_path,
            row,
            session_id
        )));
    }
    Ok(())
}

/// `writeSessionSandboxProfile`.
pub fn write_session_sandbox_profile(
    control_db_path: impl Into<JsString>,
    session_path: impl Into<JsString>,
    session_id: impl Into<JsString>,
    profile: SandboxProfileName,
) -> Result<(), ControlDbError> {
    let session_path = session_path.into();
    let session_id = session_id.into();
    with_control_db(&control_db_path.into(), |db| {
        assert_session_metadata_identity(db, &session_path, &session_id)?;
        db.execute(
            "INSERT INTO session_sandbox_profiles (session_path, session_id, profile, updated_at)
			 VALUES (?, ?, ?, ?)
			 ON CONFLICT(session_path) DO UPDATE SET
				session_id = excluded.session_id,
				profile = excluded.profile,
				updated_at = excluded.updated_at",
            params![
                encode_node_utf8(&session_path),
                encode_node_utf8(&session_id),
                profile.as_str(),
                crate::utils::iso_time::now_iso_string()
            ],
        )?;
        Ok(())
    })
}

/// `clearSessionSandboxProfile`.
pub fn clear_session_sandbox_profile(
    control_db_path: impl Into<JsString>,
    session_path: impl Into<JsString>,
    session_id: impl Into<JsString>,
) -> Result<(), ControlDbError> {
    let session_path = session_path.into();
    let session_id = session_id.into();
    with_control_db(&control_db_path.into(), |db| {
        assert_session_metadata_identity(db, &session_path, &session_id)?;
        db.execute(
            "DELETE FROM session_sandbox_profiles WHERE session_path = ? AND session_id = ?",
            params![encode_node_utf8(&session_path), encode_node_utf8(&session_id)],
        )?;
        Ok(())
    })
}

/// `readSessionSandboxProfile`.
pub fn read_session_sandbox_profile(
    control_db_path: impl Into<JsString>,
    session_path: impl Into<JsString>,
    session_id: impl Into<JsString>,
) -> Result<Option<SandboxProfileName>, ControlDbError> {
    let session_path = session_path.into();
    let session_id = session_id.into();
    with_control_db(&control_db_path.into(), |db| {
        let row: Option<(String, String)> = db
            .query_row(
                "SELECT session_id, profile FROM session_sandbox_profiles WHERE session_path = ?",
                params![encode_node_utf8(&session_path)],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((stored_session_id, profile)) = row else {
            return Ok(None);
        };
        assert_session_metadata_identity(db, &session_path, &session_id)?;
        if session_id != stored_session_id {
            return Err(ControlDbError::new(js_format!(
                "Session identity mismatch for {}: stored {}, received {}",
                &session_path,
                stored_session_id,
                &session_id
            )));
        }
        let Some(profile) = SandboxProfileName::parse(&profile) else {
            return Err(ControlDbError::new(js_format!(
                "Invalid sandbox profile '{}' for session {}",
                profile,
                &session_id
            )));
        };
        Ok(Some(profile))
    })
}

/// `writeSessionGoal`.
pub fn write_session_goal(
    control_db_path: impl Into<JsString>,
    session_path: impl Into<JsString>,
    goal_json: Option<&JsString>,
) -> Result<(), ControlDbError> {
    let session_path = encode_node_utf8(&session_path.into());
    let goal_json = goal_json.map(encode_node_utf8);
    with_control_db(&control_db_path.into(), |db| {
        db.execute(
            "
			UPDATE session_metadata
			SET goal_json = ?, updated_at = ?
			WHERE session_path = ?
			",
            params![goal_json, crate::utils::iso_time::now_iso_string(), session_path],
        )?;
        Ok(())
    })
}

/// `readSessionGoal`.
pub fn read_session_goal(
    control_db_path: impl Into<JsString>,
    session_path: impl Into<JsString>,
) -> Result<Option<JsString>, ControlDbError> {
    let session_path = encode_node_utf8(&session_path.into());
    with_control_db(&control_db_path.into(), |db| {
        Ok(db
            .query_row(
                "
				SELECT goal_json
				FROM session_metadata
				WHERE session_path = ?
				",
                params![session_path],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten()
            .map(JsString::from))
    })
}

type SessionMetadataRow = (
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    i64,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
    String,
    i64,
    String,
    String,
);

const SESSION_METADATA_SELECT: &str = "
	SELECT
		session_path,
		id,
		cwd,
		name,
		parent_session_path,
		archived_at,
		goal_json,
		is_subagent,
		subagent_name,
		model_provider,
		model_id,
		thinking_level,
		created_at,
		modified_at,
		message_count,
		first_message,
		updated_at
	FROM session_metadata
";

fn session_metadata_from_row(row: SessionMetadataRow) -> SessionMetadata {
    SessionMetadata {
        session_path: row.0.into(),
        id: row.1.into(),
        cwd: row.2.into(),
        name: normalize_session_name(row.3.map(JsString::from).as_ref()),
        parent_session_path: row.4.map(JsString::from),
        is_archived: Some(row.5.is_some()),
        archived_at: row.5,
        goal_json: row.6.map(JsString::from),
        is_subagent: Some(row.7 == 1),
        subagent_name: row.8.map(JsString::from),
        model_provider: row.9,
        model_id: row.10,
        thinking_level: row.11,
        created_at: row.12,
        modified_at: row.13,
        message_count: row.14,
        first_message: row.15.into(),
        updated_at: row.16,
    }
}

fn query_session_metadata_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionMetadataRow> {
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
        row.get(12)?,
        row.get(13)?,
        row.get(14)?,
        row.get(15)?,
        row.get(16)?,
    ))
}

/// `readSessionMetadata`.
pub fn read_session_metadata(
    control_db_path: impl Into<JsString>,
    session_path: impl Into<JsString>,
) -> Result<Option<SessionMetadata>, ControlDbError> {
    let session_path = encode_node_utf8(&session_path.into());
    with_control_db(&control_db_path.into(), |db| {
        let row = db
            .query_row(
                &format!("{SESSION_METADATA_SELECT} WHERE session_path = ?"),
                params![session_path],
                query_session_metadata_row,
            )
            .optional()?;
        Ok(row.map(session_metadata_from_row))
    })
}

/// `listSessionMetadata`.
pub fn list_session_metadata(control_db_path: impl Into<JsString>) -> Result<Vec<SessionMetadata>, ControlDbError> {
    list_session_metadata_by_archive_state(&control_db_path.into(), None)
}

/// `listEmptyMainSessionCandidates`.
pub fn list_empty_main_session_candidates(
    control_db_path: impl Into<JsString>,
) -> Result<Vec<EmptyMainSessionCandidate>, ControlDbError> {
    with_control_db(&control_db_path.into(), |db| {
        let mut statement=db.prepare("SELECT id,session_path FROM session_metadata WHERE archived_at IS NULL AND is_subagent = 0 AND message_count = 0")?;
        Ok(statement
            .query_map([], |row| {
                Ok(EmptyMainSessionCandidate {
                    id: row.get::<_, String>(0)?.into(),
                    session_path: row.get::<_, String>(1)?.into(),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?)
    })
}
/// `listResumeSessionMetadata`: absent metadata differs from a populated index with no matching rows.
pub fn list_resume_session_metadata(
    control_db_path: impl Into<JsString>,
    options: &ResumeSessionMetadataOptions,
) -> Result<Option<Vec<SessionMetadata>>, ControlDbError> {
    with_control_db(&control_db_path.into(), |db| {
        if db
            .query_row("SELECT 1 FROM session_metadata LIMIT 1", [], |row| row.get::<_, i64>(0))
            .optional()?
            .is_none()
        {
            return Ok(None);
        }
        let mut predicates = vec![
            if options.archived {
                "archived_at IS NOT NULL"
            } else {
                "archived_at IS NULL"
            },
            "is_subagent = 0",
        ];
        let mut values = Vec::new();
        if let Some(cwd) = options.cwd.as_ref().filter(|value| !value.is_empty()) {
            predicates.push("cwd = ?");
            values.push(encode_node_utf8(cwd));
        }
        if let Some(prefix) = options.session_path_prefix.as_ref().filter(|value| !value.is_empty()) {
            predicates.push("substr(session_path,1,length(?)) = ?");
            let prefix = encode_node_utf8(prefix);
            values.extend([prefix.clone(), prefix]);
        }
        let mut statement = db.prepare(&format!(
            "{SESSION_METADATA_SELECT} WHERE {} ORDER BY modified_at DESC,updated_at DESC,session_path DESC",
            predicates.join(" AND ")
        ))?;
        let rows = statement
            .query_map(rusqlite::params_from_iter(values), query_session_metadata_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Some(rows.into_iter().map(session_metadata_from_row).collect()))
    })
}

/// `listActiveSessionMetadata`.
pub fn list_active_session_metadata(
    control_db_path: impl Into<JsString>,
) -> Result<Vec<SessionMetadata>, ControlDbError> {
    list_session_metadata_by_archive_state(&control_db_path.into(), Some(false))
}

/// `listArchivedSessionMetadata`.
pub fn list_archived_session_metadata(
    control_db_path: impl Into<JsString>,
) -> Result<Vec<SessionMetadata>, ControlDbError> {
    list_session_metadata_by_archive_state(&control_db_path.into(), Some(true))
}

fn list_session_metadata_by_archive_state(
    control_db_path: &JsString,
    archived: Option<bool>,
) -> Result<Vec<SessionMetadata>, ControlDbError> {
    with_control_db(control_db_path, |db| {
        let archive_clause = match archived {
            None => "",
            Some(true) => "WHERE archived_at IS NOT NULL",
            Some(false) => "WHERE archived_at IS NULL",
        };
        let mut statement = db.prepare(&format!(
            "{SESSION_METADATA_SELECT}
				{archive_clause}
				ORDER BY modified_at DESC, updated_at DESC, session_path DESC"
        ))?;
        let rows = statement
            .query_map([], query_session_metadata_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows.into_iter().map(session_metadata_from_row).collect())
    })
}

/// `findActiveSessionMetadataById`.
pub fn find_active_session_metadata_by_id(
    control_db_path: impl Into<JsString>,
    id: impl Into<JsString>,
) -> Result<Vec<SessionMetadata>, ControlDbError> {
    find_active_session_metadata(&control_db_path.into(), "id", id)
}

/// `findActiveSessionMetadataByName`.
pub fn find_active_session_metadata_by_name(
    control_db_path: impl Into<JsString>,
    name: impl Into<JsString>,
) -> Result<Vec<SessionMetadata>, ControlDbError> {
    find_active_session_metadata(&control_db_path.into(), "name", name)
}

fn find_active_session_metadata(
    control_db_path: &JsString,
    field: &str,
    value: impl Into<JsString>,
) -> Result<Vec<SessionMetadata>, ControlDbError> {
    let value = value.into();
    with_control_db(control_db_path, |db| {
        let (match_clause, values): (&str, Vec<JsString>) = if field == "id" {
            ("id >= ? AND id < ?", vec![value.clone(), value.clone() + "\u{ffff}"])
        } else {
            ("name = ?", vec![value.clone()])
        };
        let mut statement = db.prepare(&format!(
            "{SESSION_METADATA_SELECT}
				WHERE archived_at IS NULL AND is_subagent = 0 AND {match_clause}
				ORDER BY modified_at DESC, updated_at DESC, session_path DESC"
        ))?;
        let rows = statement
            .query_map(
                rusqlite::params_from_iter(values.iter().map(encode_node_utf8)),
                query_session_metadata_row,
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows.into_iter().map(session_metadata_from_row).collect())
    })
}

/// `archiveSession`.
pub fn archive_session(
    control_db_path: impl Into<JsString>,
    session_path: impl Into<JsString>,
) -> Result<(), ControlDbError> {
    let session_path = encode_node_utf8(&session_path.into());
    with_control_db(&control_db_path.into(), |db| {
        let now = crate::utils::iso_time::now_iso_string();
        db.execute(
            "UPDATE session_metadata SET archived_at = ?, updated_at = ? WHERE session_path = ?",
            params![now, now, session_path],
        )?;
        Ok(())
    })
}

/// `unarchiveSession`.
pub fn unarchive_session(
    control_db_path: impl Into<JsString>,
    session_path: impl Into<JsString>,
) -> Result<(), ControlDbError> {
    let session_path = encode_node_utf8(&session_path.into());
    with_control_db(&control_db_path.into(), |db| {
        db.execute(
            "UPDATE session_metadata SET archived_at = NULL, updated_at = ? WHERE session_path = ?",
            params![crate::utils::iso_time::now_iso_string(), session_path],
        )?;
        Ok(())
    })
}

/// `archiveSessionsOlderThan`; `cutoff_iso` is the already-formatted cutoff timestamp.
pub fn archive_sessions_older_than(
    control_db_path: impl Into<JsString>,
    cutoff_iso: &str,
) -> Result<Vec<JsString>, ControlDbError> {
    with_control_db(&control_db_path.into(), |db| {
        let mut statement = db.prepare(
            "SELECT session_path, modified_at FROM session_metadata
             WHERE archived_at IS NULL AND is_subagent = 0 AND modified_at < ?",
        )?;
        let mut rows = statement
            .query_map(params![cutoff_iso], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.sort_by(|left, right| left.1.cmp(&right.1).then_with(|| left.0.cmp(&right.0)));
        Ok(rows.into_iter().map(|(session_path, _)| session_path.into()).collect())
    })
}

/// `writeLastMessage`.
pub fn write_last_message(
    control_db_path: impl Into<JsString>,
    role: &str,
    content: impl Into<JsString>,
) -> Result<(), ControlDbError> {
    let content = encode_node_utf8(&content.into());
    with_control_db(&control_db_path.into(), |db| {
        db.execute(
            "
			INSERT INTO last_message (id, role, content, updated_at)
			VALUES (1, ?, ?, ?)
			ON CONFLICT(id) DO UPDATE SET
				role = excluded.role,
				content = excluded.content,
				updated_at = excluded.updated_at
			",
            params![role, content, crate::utils::iso_time::now_iso_string()],
        )?;
        Ok(())
    })
}

/// `readLastMessage`.
pub fn read_last_message(control_db_path: impl Into<JsString>) -> Result<Option<LastControlMessage>, ControlDbError> {
    with_control_db(&control_db_path.into(), |db| {
        let row: Option<(String, String, String)> = db
            .query_row(
                "
				SELECT role, content, updated_at
				FROM last_message
				WHERE id = 1
				",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let Some((role, content, updated_at)) = row else {
            return Ok(None);
        };
        if role != "assistant" {
            return Ok(None);
        }
        Ok(Some(LastControlMessage {
            role,
            content,
            updated_at,
        }))
    })
}

/// `recordPromptHistoryEntry`.
pub fn record_prompt_history_entry(
    control_db_path: impl Into<JsString>,
    content: impl Into<JsString>,
) -> Result<(), ControlDbError> {
    let content = content.into();
    with_control_db(&control_db_path.into(), |db| insert_prompt_history_entry(db, &content))
}

/// `readPromptHistory`.
pub fn read_prompt_history(control_db_path: impl Into<JsString>, limit: i64) -> Result<Vec<JsString>, ControlDbError> {
    with_control_db(&control_db_path.into(), |db| read_prompt_history_rows(db, limit))
}

/// `readOrMigratePromptHistory`.
pub fn read_or_migrate_prompt_history(
    control_db_path: impl Into<JsString>,
    legacy_entries: &[JsString],
    limit: i64,
) -> Result<Vec<JsString>, ControlDbError> {
    with_control_db(&control_db_path.into(), |db| {
        let existing_entries = read_prompt_history_rows(db, limit)?;
        if !existing_entries.is_empty() {
            return Ok(existing_entries);
        }
        if legacy_entries.is_empty() {
            return Ok(Vec::new());
        }
        let prepared_rows = prepare_prompt_history_migration_rows(legacy_entries);
        let legacy_end = pi_js_compat::number::js_slice_index(limit as f64, legacy_entries.len());
        if prepared_rows.is_empty() {
            return Ok(legacy_entries[..legacy_end].to_vec());
        }
        let inserted = insert_prompt_history_migration_rows(db, &prepared_rows)?;
        if inserted == prepared_rows.len() as i64 {
            return Ok(legacy_entries[..legacy_end].to_vec());
        }
        if inserted == 0 {
            return read_prompt_history_rows(db, limit);
        }
        Err(ControlDbError::new(format!(
            "Prompt history migration inserted {inserted} of {} prepared rows",
            prepared_rows.len()
        )))
    })
}

fn prepare_prompt_history_migration_rows(legacy_entries: &[JsString]) -> Vec<(JsString, String)> {
    let created_at = crate::utils::iso_time::now_iso_string();
    let mut rows = Vec::new();
    let mut previous_content: Option<JsString> = None;
    for content in legacy_entries.iter().rev() {
        let content = content.trim();
        if content.is_empty() || previous_content.as_ref() == Some(&content) {
            continue;
        }
        rows.push((content.clone(), created_at.clone()));
        previous_content = Some(content);
    }
    rows
}

fn insert_prompt_history_migration_rows(db: &Connection, rows: &[(JsString, String)]) -> Result<i64, ControlDbError> {
    let value = JsValue::Array(
        rows.iter()
            .map(|(content, created_at)| {
                pi_js_compat::js_json!({ "content": content.clone(), "createdAt": created_at.as_str() })
            })
            .collect(),
    );
    let inserted = db.execute(
        "INSERT INTO prompt_history (content, created_at)
			 SELECT json_extract(value, '$.content'), json_extract(value, '$.createdAt')
			 FROM json_each(?)
			 WHERE NOT EXISTS (SELECT 1 FROM prompt_history)
			 ORDER BY CAST(key AS INTEGER)",
        params![js_json::stringify_lossless(&value)],
    )?;
    Ok(inserted as i64)
}

fn read_prompt_history_rows(db: &Connection, limit: i64) -> Result<Vec<JsString>, ControlDbError> {
    let mut statement = db.prepare(
        "
			SELECT content
			FROM prompt_history
			ORDER BY id DESC
			LIMIT ?
			",
    )?;
    let rows = statement
        .query_map(params![limit], |row| row.get::<_, JsString>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn insert_prompt_history_entry(db: &Connection, content: &JsString) -> Result<(), ControlDbError> {
    let trimmed_content = content.trim();
    if trimmed_content.is_empty() {
        return Ok(());
    }
    if read_latest_prompt_history_entry(db)?.is_some_and(|latest| trimmed_content == latest) {
        return Ok(());
    }
    db.execute(
        "
		INSERT INTO prompt_history (content, created_at)
		VALUES (?, ?)
		",
        params![
            encode_node_utf8(&trimmed_content),
            crate::utils::iso_time::now_iso_string()
        ],
    )?;
    Ok(())
}

fn read_latest_prompt_history_entry(db: &Connection) -> Result<Option<JsString>, ControlDbError> {
    Ok(db
        .query_row(
            "
			SELECT content
			FROM prompt_history
			ORDER BY id DESC
			LIMIT 1
			",
            [],
            |row| row.get::<_, JsString>(0),
        )
        .optional()?)
}
