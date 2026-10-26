//! Control database schema and migrations (`initializeSchema` in `session-control-db.ts`).

use pi_js_compat::js_format;
use std::collections::HashSet;

use pi_js_compat::js_json::JsValue as Value;
use rusqlite::{Connection, OptionalExtension, params};

use super::{
    ControlDbError, parse_stored_json_object_lossless, require_native_string_field, to_runtime_mailbox_message_kind,
    to_runtime_mailbox_message_status, validate_mailbox_payload, with_immediate_transaction,
    write_canonical_mailbox_payload,
};
use crate::core::runtime_process::{is_pi_runtime_process_alive, is_process_identity_alive};
use pi_js_compat::js_json::{self, JsString, encode_node_utf8};

/// Version that carries the multi-agent lifecycle payload migration.
pub const LIFECYCLE_PROTOCOL_SCHEMA_VERSION: i64 = 14;
/// Version that makes `session_metadata.name` the only session-name store.
pub const CONTROL_DB_SCHEMA_VERSION: i64 = 15;

const CREATE_SCHEMA_SQL: &str = "
		CREATE TABLE IF NOT EXISTS incoming_messages (
			id INTEGER PRIMARY KEY AUTOINCREMENT,
			content TEXT NOT NULL,
			status TEXT NOT NULL,
			created_at TEXT NOT NULL,
			claimed_at TEXT,
			completed_at TEXT,
			error TEXT
		);

		CREATE TABLE IF NOT EXISTS codex_restriction_recovery_claims (
			user_request_id TEXT PRIMARY KEY NOT NULL,
			claimed_at TEXT NOT NULL
		);

		CREATE TABLE IF NOT EXISTS last_message (
			id INTEGER PRIMARY KEY CHECK (id = 1),
			role TEXT NOT NULL,
			content TEXT NOT NULL,
			updated_at TEXT NOT NULL
		);

		CREATE TABLE IF NOT EXISTS runtime_mailbox_listeners (
			recipient_session_id TEXT NOT NULL,
			recipient_agent_id_key TEXT NOT NULL,
			pid INTEGER NOT NULL,
			runtime_instance_id TEXT,
			session_path TEXT,
			session_path_asserted_at TEXT,
			updated_at TEXT NOT NULL,
			PRIMARY KEY (recipient_session_id, recipient_agent_id_key)
		);

		CREATE TABLE IF NOT EXISTS reminders (
			id INTEGER PRIMARY KEY AUTOINCREMENT,
			title TEXT NOT NULL CHECK (length(trim(title)) > 0),
			notes TEXT NOT NULL,
			project_path TEXT,
			due_at TEXT NOT NULL,
			status TEXT NOT NULL CHECK (status IN ('open', 'done')),
			created_at TEXT NOT NULL,
			completed_at TEXT,
			delivered_at TEXT,
			CHECK (
				(status = 'open' AND completed_at IS NULL)
				OR (status = 'done' AND completed_at IS NOT NULL AND julianday(completed_at) IS NOT NULL)
			)
		);

		CREATE TABLE IF NOT EXISTS prompt_history (
			id INTEGER PRIMARY KEY AUTOINCREMENT,
			content TEXT NOT NULL,
			created_at TEXT NOT NULL
		);

		CREATE INDEX IF NOT EXISTS prompt_history_id_idx
		ON prompt_history(id DESC);

		CREATE TABLE IF NOT EXISTS session_metadata (
			session_path TEXT PRIMARY KEY,
			id TEXT NOT NULL,
			cwd TEXT NOT NULL,
			name TEXT,
			parent_session_path TEXT,
			archived_at TEXT,
			goal_json TEXT,
			is_subagent INTEGER NOT NULL DEFAULT 0,
			subagent_name TEXT,
			model_provider TEXT,
			model_id TEXT,
			thinking_level TEXT,
			created_at TEXT NOT NULL,
			modified_at TEXT NOT NULL,
			message_count INTEGER NOT NULL,
			first_message TEXT NOT NULL,
			updated_at TEXT NOT NULL
		);

		CREATE INDEX IF NOT EXISTS session_metadata_modified_at_idx
		ON session_metadata(modified_at DESC, updated_at DESC);

		CREATE INDEX IF NOT EXISTS session_metadata_id_idx
		ON session_metadata(id);

		CREATE INDEX IF NOT EXISTS session_metadata_name_idx
		ON session_metadata(name);

		CREATE TABLE IF NOT EXISTS session_sandbox_profiles (
			session_path TEXT PRIMARY KEY,
			session_id TEXT NOT NULL,
			profile TEXT NOT NULL,
			updated_at TEXT NOT NULL
		);

		CREATE TABLE IF NOT EXISTS multi_agent_agents (
			session_path TEXT NOT NULL,
			agent_id TEXT NOT NULL,
			data TEXT NOT NULL,
			updated_at TEXT NOT NULL,
			PRIMARY KEY (session_path, agent_id)
		);

		CREATE INDEX IF NOT EXISTS multi_agent_agents_agent_updated_at_idx
		ON multi_agent_agents(CASE WHEN json_valid(data) THEN json_extract(data, '$.updatedAt') END);

		CREATE INDEX IF NOT EXISTS multi_agent_agents_detached_lifecycle_idx
		ON multi_agent_agents(
			json_extract(data, '$.detached'),
			json_extract(data, '$.lifecycle')
		)
		WHERE json_valid(data);

		CREATE TABLE IF NOT EXISTS multi_agent_runtime_owners (
			session_path TEXT NOT NULL,
			agent_id TEXT NOT NULL,
			process_identity TEXT,
			owner_session_id TEXT,
			owner_agent_id TEXT,
			PRIMARY KEY (session_path, agent_id)
		);

		CREATE TABLE IF NOT EXISTS multi_agent_terminal_outbox (
			session_path TEXT NOT NULL,
			agent_id TEXT NOT NULL,
			terminal_revision INTEGER NOT NULL,
			event_kind TEXT NOT NULL,
			status TEXT NOT NULL,
			claim_id TEXT,
			claimed_at TEXT,
			-- Unused since delivery deletes rows; dropping it needs a quiescent schema migration.
			delivered_at TEXT,
			attempt_count INTEGER NOT NULL DEFAULT 0,
			last_error TEXT,
			updated_at TEXT NOT NULL,
			PRIMARY KEY (session_path, agent_id, terminal_revision, event_kind)
		);

		CREATE INDEX IF NOT EXISTS multi_agent_terminal_outbox_status_idx
		ON multi_agent_terminal_outbox(status, updated_at);

		CREATE TABLE IF NOT EXISTS multi_agent_mailbox_messages (
			session_path TEXT NOT NULL,
			message_id TEXT NOT NULL,
			data TEXT NOT NULL,
			updated_at TEXT NOT NULL,
			PRIMARY KEY (session_path, message_id)
		);

		CREATE INDEX IF NOT EXISTS multi_agent_mailbox_recipient_status_idx
		ON multi_agent_mailbox_messages(
			CASE WHEN json_valid(data) THEN json_extract(data, '$.recipientSessionId') END,
			CASE WHEN json_valid(data) THEN json_extract(data, '$.recipientAgentId') END,
			CASE WHEN json_valid(data) THEN json_extract(data, '$.status') END
		);

		CREATE TABLE IF NOT EXISTS multi_agent_counters_v2 (
			session_path TEXT PRIMARY KEY,
			next_agent_number INTEGER NOT NULL,
			next_message_number INTEGER NOT NULL,
			updated_at TEXT NOT NULL
		);

		CREATE TABLE IF NOT EXISTS session_health (
			session_id TEXT PRIMARY KEY,
			agent_generation INTEGER NOT NULL,
			pid INTEGER,
			last_active_at TEXT,
			last_checked_at TEXT,
			check_status TEXT NOT NULL,
			checked_generation INTEGER,
			check_latency_ms INTEGER,
			updated_at TEXT NOT NULL
		);

		CREATE TABLE IF NOT EXISTS architect_requests (
			id INTEGER PRIMARY KEY AUTOINCREMENT,
			sender_session_id TEXT NOT NULL,
			project_cwd TEXT,
			body TEXT NOT NULL,
			status TEXT NOT NULL,
			created_at TEXT NOT NULL,
			claimed_at TEXT,
			claim_token TEXT,
			completed_at TEXT
		);

		CREATE INDEX IF NOT EXISTS architect_requests_status_id_idx
		ON architect_requests(status, id);

		CREATE TABLE IF NOT EXISTS supervisor_requests (
			id INTEGER PRIMARY KEY AUTOINCREMENT,
			sender_session_id TEXT NOT NULL,
			project_id TEXT NOT NULL,
			kind TEXT NOT NULL,
			payload_json TEXT NOT NULL,
			deadline_at TEXT NOT NULL,
			status TEXT NOT NULL,
			created_at TEXT NOT NULL,
			claimed_at TEXT,
			claim_token TEXT,
			completed_at TEXT,
			response_json TEXT
		);

		CREATE INDEX IF NOT EXISTS supervisor_requests_priority_idx
		ON supervisor_requests(status, kind, id);

		CREATE TABLE IF NOT EXISTS shared_channel_messages (
			id INTEGER PRIMARY KEY AUTOINCREMENT,
			sender_session_id TEXT NOT NULL,
			sender_agent_id TEXT,
			body TEXT NOT NULL,
			created_at TEXT NOT NULL
		);

		CREATE INDEX IF NOT EXISTS shared_channel_messages_id_idx
		ON shared_channel_messages(id);

		CREATE TABLE IF NOT EXISTS shared_channel_cursors (
			session_id TEXT NOT NULL,
			agent_id_key TEXT NOT NULL,
			last_seen_id INTEGER NOT NULL,
			updated_at TEXT NOT NULL,
			PRIMARY KEY (session_id, agent_id_key)
		);
	";

/// `initializeSchema`.
pub fn initialize_schema(db: &Connection, self_restart_process_id: Option<u32>) -> Result<(), ControlDbError> {
    assert_supported_control_db_schema_version(db)?;
    db.execute_batch(CREATE_SCHEMA_SQL)?;
    let schema_version = read_user_version(db)?;
    if schema_version >= LIFECYCLE_PROTOCOL_SCHEMA_VERSION {
        migrate_legacy_multi_agent_counters(db)?;
    }
    migrate_legacy_multi_agent_payloads(db, self_restart_process_id)?;
    add_missing_session_metadata_columns(db)?;
    db.execute_batch(
        "CREATE INDEX IF NOT EXISTS session_metadata_parent_session_path_idx
		ON session_metadata(parent_session_path);
        CREATE INDEX IF NOT EXISTS session_metadata_empty_main_idx
		ON session_metadata(id, session_path)
		WHERE archived_at IS NULL AND is_subagent = 0 AND message_count = 0;",
    )?;
    migrate_legacy_session_names(db, self_restart_process_id)?;
    add_missing_runtime_mailbox_listener_columns(db)?;
    add_missing_architect_request_columns(db)?;
    add_missing_shared_channel_origin_columns(db)?;
    db.execute_batch(
        "CREATE INDEX IF NOT EXISTS shared_channel_messages_created_at_idx
		ON shared_channel_messages(created_at);
        CREATE TRIGGER IF NOT EXISTS multi_agent_mailbox_default_birth
		AFTER INSERT ON multi_agent_mailbox_messages
		WHEN CASE WHEN json_valid(NEW.data) THEN
			COALESCE(json_type(NEW.data, '$.createdAt'), 'null') = 'null' ELSE 0 END
		BEGIN
			UPDATE multi_agent_mailbox_messages
			SET data = json_set(NEW.data, '$.createdAt', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
			WHERE rowid = NEW.rowid;
		END;",
    )?;
    db.execute_batch(&format!(
        "CREATE INDEX IF NOT EXISTS multi_agent_mailbox_created_at_ms_idx
		ON multi_agent_mailbox_messages({})",
        super::retention::MAILBOX_CREATED_AT_MS_SQL
    ))?;
    super::retention::cleanup_expired_messages(db)?;
    Ok(())
}

pub(crate) fn read_user_version(db: &Connection) -> Result<i64, ControlDbError> {
    Ok(db.query_row("PRAGMA user_version", [], |row| row.get(0))?)
}

fn assert_supported_control_db_schema_version(db: &Connection) -> Result<(), ControlDbError> {
    let schema_version = read_user_version(db)?;
    if schema_version > CONTROL_DB_SCHEMA_VERSION {
        return Err(ControlDbError::new(format!(
            "Unsupported control database schema version {schema_version}; this Pi runtime supports up to version {CONTROL_DB_SCHEMA_VERSION}"
        )));
    }
    Ok(())
}

fn table_exists(db: &Connection, name: &str) -> Result<bool, ControlDbError> {
    Ok(db
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?",
            params![name],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

fn table_columns(db: &Connection, name: &str) -> Result<HashSet<String>, ControlDbError> {
    let mut statement = db.prepare(&format!("PRAGMA table_info({name})"))?;
    let rows = statement.query_map([], |row| row.get::<_, String>(1))?;
    let mut columns = HashSet::new();
    for column in rows {
        columns.insert(column?);
    }
    Ok(columns)
}

fn migrate_legacy_multi_agent_counters(db: &Connection) -> Result<(), ControlDbError> {
    if table_exists(db, "multi_agent_counters")? {
        let mut statement = db.prepare(
            "SELECT session_path, next_agent_number, next_message_number, updated_at FROM multi_agent_counters",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (session_path, next_agent_number, next_message_number, updated_at) in rows {
            let current: Option<(i64, i64, String)> = db
                .query_row(
                    "SELECT next_agent_number, next_message_number, updated_at FROM multi_agent_counters_v2 WHERE session_path = ?",
                    params![session_path],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?;
            let (current_agent, current_message, current_updated_at) = current
                .map(|(agent, message, updated_at)| (Some(agent), Some(message), Some(updated_at)))
                .unwrap_or((None, None, None));
            db.execute(
                "INSERT INTO multi_agent_counters_v2 (session_path, next_agent_number, next_message_number, updated_at)
			 VALUES (?, ?, ?, ?)
			 ON CONFLICT(session_path) DO UPDATE SET
			 next_agent_number = excluded.next_agent_number,
			 next_message_number = excluded.next_message_number,
			 updated_at = excluded.updated_at",
                params![
                    session_path,
                    current_agent.unwrap_or(1).max(next_agent_number),
                    current_message.unwrap_or(1).max(next_message_number),
                    current_updated_at
                        .filter(|current_updated_at| current_updated_at > &updated_at)
                        .unwrap_or(updated_at)
                ],
            )?;
        }
        db.execute_batch("DROP TABLE multi_agent_counters")?;
    }
    db.execute_batch("DROP TABLE IF EXISTS multi_agent_artifacts")?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum LifecycleMigrationOwnerTableKind {
    Missing,
    WithoutProcessIdentity,
    WithProcessIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LifecycleMigrationOwnerTableState {
    kind: LifecycleMigrationOwnerTableKind,
    process_identities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LifecycleProtocolMigrationState {
    dispatch_leases: LifecycleMigrationOwnerTableState,
    runtime_owners: LifecycleMigrationOwnerTableState,
    runtime_pids: Vec<u32>,
}

fn migrate_legacy_multi_agent_payloads(
    db: &Connection,
    self_restart_process_id: Option<u32>,
) -> Result<(), ControlDbError> {
    let target_version = LIFECYCLE_PROTOCOL_SCHEMA_VERSION;
    if read_user_version(db)? >= target_version {
        return Ok(());
    }
    let migration_state = read_lifecycle_protocol_migration_state(db)?;
    assert_control_db_migration_quiescent(
        &migration_state,
        self_restart_process_id,
        "lifecycle protocol",
        target_version,
    )?;
    let migration_timestamp = crate::utils::iso_time::now_iso_string();

    with_immediate_transaction(db, |db| {
        if read_user_version(db)? >= target_version {
            return Ok(());
        }
        let current_state = read_lifecycle_protocol_migration_state(db)?;
        if migration_state != current_state {
            return Err(ControlDbError::new(
                "Lifecycle protocol migration state changed while acquiring the writer lock; retry",
            ));
        }

        migrate_legacy_multi_agent_counters(db)?;
        drop_lifecycle_access_control_triggers(db)?;
        db.execute_batch("DROP TABLE IF EXISTS multi_agent_recovery_leader")?;
        migrate_terminal_outbox_schema(db)?;
        migrate_runtime_owner_table(db)?;
        migrate_legacy_lifecycle_rows(db, &migration_timestamp)?;
        migrate_legacy_multi_agent_payload_table(db, "multi_agent_agents", "agent_id", &migration_timestamp)?;
        migrate_legacy_multi_agent_payload_table(
            db,
            "multi_agent_mailbox_messages",
            "message_id",
            &migration_timestamp,
        )?;
        migrate_legacy_runtime_mailbox_messages(db, &migration_timestamp)?;
        create_legacy_artifact_field_triggers(db)?;
        db.execute_batch(&format!("PRAGMA user_version = {target_version}"))?;
        Ok(())
    })
}

/// `migrateLegacySessionNames`.
fn migrate_legacy_session_names(db: &Connection, self_restart_process_id: Option<u32>) -> Result<(), ControlDbError> {
    let target_version = CONTROL_DB_SCHEMA_VERSION;
    if read_user_version(db)? >= target_version {
        return Ok(());
    }
    let migration_state = read_lifecycle_protocol_migration_state(db)?;
    assert_control_db_migration_quiescent(
        &migration_state,
        self_restart_process_id,
        "session name schema",
        target_version,
    )?;

    with_immediate_transaction(db, |db| {
        if read_user_version(db)? >= target_version {
            return Ok(());
        }
        let current_state = read_lifecycle_protocol_migration_state(db)?;
        if migration_state != current_state {
            return Err(ControlDbError::new(
                "Session name migration state changed while acquiring the writer lock; retry",
            ));
        }
        assert_control_db_migration_quiescent(
            &current_state,
            self_restart_process_id,
            "session name schema",
            target_version,
        )?;
        move_legacy_session_names_into_metadata(db)?;
        db.execute_batch(&format!("PRAGMA user_version = {target_version}"))?;
        Ok(())
    })
}

/// `moveLegacySessionNamesIntoMetadata`.
fn move_legacy_session_names_into_metadata(db: &Connection) -> Result<(), ControlDbError> {
    if !table_exists(db, "named_sessions")? {
        return Ok(());
    }
    let mut statement = db.prepare("SELECT session_path, name FROM named_sessions")?;
    let legacy_rows = statement
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    for (session_path, name) in legacy_rows {
        db.execute(
            "UPDATE session_metadata SET name = ? WHERE session_path = ?",
            params![
                encode_node_utf8(&super::sessions::normalize_session_name(Some(&name.into())).unwrap_or_default()),
                session_path
            ],
        )?;
    }
    db.execute_batch("DROP TABLE named_sessions")?;
    Ok(())
}

fn read_lifecycle_protocol_migration_state(db: &Connection) -> Result<LifecycleProtocolMigrationState, ControlDbError> {
    let mut statement = db.prepare(
        "SELECT pid FROM runtime_mailbox_listeners
				 UNION
				 SELECT pid FROM session_health WHERE pid IS NOT NULL
				 ORDER BY pid",
    )?;
    let runtime_pids = statement
        .query_map([], |row| row.get::<_, u32>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(LifecycleProtocolMigrationState {
        dispatch_leases: read_lifecycle_migration_owner_table_state(db, "multi_agent_dispatch_leases")?,
        runtime_owners: read_lifecycle_migration_owner_table_state(db, "multi_agent_runtime_owners")?,
        runtime_pids,
    })
}

fn read_lifecycle_migration_owner_table_state(
    db: &Connection,
    table_name: &str,
) -> Result<LifecycleMigrationOwnerTableState, ControlDbError> {
    if !table_exists(db, table_name)? {
        return Ok(LifecycleMigrationOwnerTableState {
            kind: LifecycleMigrationOwnerTableKind::Missing,
            process_identities: Vec::new(),
        });
    }
    if !table_columns(db, table_name)?.contains("process_identity") {
        return Ok(LifecycleMigrationOwnerTableState {
            kind: LifecycleMigrationOwnerTableKind::WithoutProcessIdentity,
            process_identities: Vec::new(),
        });
    }
    let mut statement = db.prepare(&format!(
        "SELECT process_identity FROM {table_name} WHERE process_identity IS NOT NULL ORDER BY process_identity"
    ))?;
    let process_identities = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(LifecycleMigrationOwnerTableState {
        kind: LifecycleMigrationOwnerTableKind::WithProcessIdentity,
        process_identities,
    })
}

fn assert_control_db_migration_quiescent(
    state: &LifecycleProtocolMigrationState,
    self_restart_process_id: Option<u32>,
    migration_name: &str,
    target_version: i64,
) -> Result<(), ControlDbError> {
    let mut live_runtime_pids: Vec<u32> = state
        .runtime_pids
        .iter()
        .copied()
        .filter(|pid| is_pi_runtime_process_alive(*pid))
        .collect();
    for table_state in [&state.dispatch_leases, &state.runtime_owners] {
        for serialized_identity in &table_state.process_identities {
            if let Ok(identity) = super::parse_process_identity(serialized_identity)
                && is_process_identity_alive(&identity)
            {
                live_runtime_pids.push(identity.pid);
            }
        }
    }
    let mut seen = HashSet::new();
    let unique_live_pids: Vec<u32> = live_runtime_pids
        .into_iter()
        .filter(|pid| seen.insert(*pid) && Some(*pid) != self_restart_process_id)
        .collect();
    if unique_live_pids.is_empty() {
        return Ok(());
    }
    let pids = unique_live_pids
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    Err(ControlDbError::new(format!(
        "Cannot activate {migration_name} version {target_version} while lifecycle owners are active (PIDs: {pids}). Stop all Pi and detached runner processes, then retry"
    )))
}

fn migrate_terminal_outbox_schema(db: &Connection) -> Result<(), ControlDbError> {
    if !table_exists(db, "multi_agent_terminal_events")? {
        return Ok(());
    }
    db.execute_batch(
        "
		DROP TABLE IF EXISTS multi_agent_terminal_cursors;
		ALTER TABLE multi_agent_terminal_outbox RENAME TO multi_agent_terminal_outbox_v12;
		CREATE TABLE multi_agent_terminal_outbox (
			session_path TEXT NOT NULL,
			agent_id TEXT NOT NULL,
			terminal_revision INTEGER NOT NULL,
			event_kind TEXT NOT NULL,
			status TEXT NOT NULL,
			claim_id TEXT,
			claimed_at TEXT,
			delivered_at TEXT,
			attempt_count INTEGER NOT NULL DEFAULT 0,
			last_error TEXT,
			updated_at TEXT NOT NULL,
			PRIMARY KEY (session_path, agent_id, terminal_revision, event_kind)
		);
		INSERT INTO multi_agent_terminal_outbox
		SELECT * FROM multi_agent_terminal_outbox_v12 WHERE status != 'delivered';
		DROP TABLE multi_agent_terminal_outbox_v12;
		DROP TABLE multi_agent_terminal_events;
		CREATE INDEX IF NOT EXISTS multi_agent_terminal_outbox_status_idx
		ON multi_agent_terminal_outbox(status, updated_at);
	",
    )?;
    Ok(())
}

fn migrate_runtime_owner_table(db: &Connection) -> Result<(), ControlDbError> {
    if table_exists(db, "multi_agent_dispatch_leases")? {
        let has_exact_process_identity = table_columns(db, "multi_agent_dispatch_leases")?.contains("process_identity");
        let copy_columns = if has_exact_process_identity {
            "session_path, agent_id, process_identity, owner_session_id, owner_agent_id"
        } else {
            "session_path, agent_id"
        };
        db.execute_batch(&format!(
            "
			INSERT OR REPLACE INTO multi_agent_runtime_owners ({copy_columns})
			SELECT {copy_columns} FROM multi_agent_dispatch_leases;
			DROP TABLE multi_agent_dispatch_leases;
			DROP INDEX IF EXISTS multi_agent_dispatch_leases_expiry_idx;
		"
        ))?;
    }

    if !table_columns(db, "multi_agent_runtime_owners")?.contains("lease_id") {
        return Ok(());
    }
    db.execute_batch(
        "
		ALTER TABLE multi_agent_runtime_owners RENAME TO multi_agent_runtime_owners_legacy;
		CREATE TABLE multi_agent_runtime_owners (
			session_path TEXT NOT NULL,
			agent_id TEXT NOT NULL,
			process_identity TEXT,
			owner_session_id TEXT,
			owner_agent_id TEXT,
			PRIMARY KEY (session_path, agent_id)
		);
		INSERT INTO multi_agent_runtime_owners (session_path, agent_id)
		SELECT session_path, agent_id FROM multi_agent_runtime_owners_legacy;
		DROP TABLE multi_agent_runtime_owners_legacy;
		DROP INDEX IF EXISTS multi_agent_runtime_owners_expiry_idx;
	",
    )?;
    Ok(())
}

fn migrate_legacy_lifecycle_rows(db: &Connection, now_iso: &str) -> Result<(), ControlDbError> {
    let mut statement = db.prepare(
        "SELECT agents.session_path, agents.agent_id, agents.data
		FROM multi_agent_agents AS agents LEFT JOIN multi_agent_runtime_owners AS owners
		ON owners.session_path = agents.session_path AND owners.agent_id = agents.agent_id
		WHERE owners.agent_id IS NULL OR owners.process_identity IS NULL",
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (session_path, agent_id, data) in rows {
        let context = js_format!("multi_agent_agents:{}#{}", session_path, agent_id);
        let agent = parse_stored_json_object_lossless(&data, &context)?;
        let lifecycle = agent.get("lifecycle").and_then(Value::as_string);
        if lifecycle.is_some_and(|value| value == "queued") {
            db.execute(
                "INSERT OR IGNORE INTO multi_agent_runtime_owners (session_path, agent_id) VALUES (?, ?)",
                params![session_path, agent_id],
            )?;
            continue;
        }
        if !lifecycle.is_some_and(|value| {
            [
                "starting",
                "running",
                "waiting_for_input",
                "steering_pending",
                "cancelling",
            ]
            .iter()
            .any(|candidate| value == *candidate)
        }) {
            continue;
        }
        let terminal_revision = agent
            .get("revision")
            .and_then(Value::as_f64)
            .map(|value| value as i64)
            .map_or(1, |revision| revision + 1);
        let mut updated = agent.clone();
        updated.insert(
            "error".into(),
            pi_js_compat::js_json!({
                "code": "lost_runtime",
                "message": "Legacy active agent had no fenced runtime ownership during protocol migration.",
            }),
        );
        updated.insert("lifecycle".into(), Value::String("failed".into()));
        updated.insert("revision".into(), Value::Number(terminal_revision as f64));
        updated.insert("updatedAt".into(), Value::String(now_iso.into()));
        updated.shift_remove("worker");
        db.execute(
            "UPDATE multi_agent_agents SET data = ?, updated_at = ? WHERE session_path = ? AND agent_id = ?",
            params![
                js_json::stringify_lossless(&Value::Object(updated)),
                now_iso,
                session_path,
                agent_id
            ],
        )?;
        db.execute(
            "INSERT INTO multi_agent_terminal_outbox (session_path, agent_id, terminal_revision, event_kind, status, attempt_count, updated_at) VALUES (?, ?, ?, 'lost_runtime', 'pending', 0, ?)",
            params![session_path, agent_id, terminal_revision, now_iso],
        )?;
    }
    Ok(())
}

fn drop_lifecycle_access_control_triggers(db: &Connection) -> Result<(), ControlDbError> {
    db.execute_batch(
        "
		DROP TRIGGER IF EXISTS multi_agent_agents_require_authorized_lifecycle_update;
		DROP TRIGGER IF EXISTS multi_agent_agents_require_lifecycle_protocol_insert;
		DROP TRIGGER IF EXISTS multi_agent_agents_require_lifecycle_protocol_update;
	",
    )?;
    Ok(())
}

fn migrate_legacy_multi_agent_payload_table(
    db: &Connection,
    table: &str,
    id_column: &str,
    now: &str,
) -> Result<(), ControlDbError> {
    let mut statement = db.prepare(&format!("SELECT session_path, {id_column}, data FROM {table}"))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut update = db.prepare(&format!(
        "UPDATE {table} SET data = ?, updated_at = ? WHERE session_path = ? AND {id_column} = ?"
    ))?;
    for (session_path, id, data) in rows {
        let Ok(parsed) = js_json::parse_lossless(&data) else {
            // Leave malformed payloads for the normal restore validator to report with row context.
            continue;
        };
        let (migrated, changed) = strip_legacy_artifact_fields(&parsed);
        if !changed {
            continue;
        }
        update.execute(params![js_json::stringify_lossless(&migrated), now, session_path, id])?;
    }
    Ok(())
}

fn create_legacy_artifact_field_triggers(db: &Connection) -> Result<(), ControlDbError> {
    for table in ["multi_agent_agents", "multi_agent_mailbox_messages"] {
        db.execute_batch(&format!(
            "
			CREATE TRIGGER IF NOT EXISTS {table}_reject_legacy_artifact_fields_insert
			BEFORE INSERT ON {table}
			FOR EACH ROW
			WHEN json_valid(NEW.data)
				AND EXISTS (
					SELECT 1 FROM json_tree(NEW.data)
					WHERE key IN ('artifactIds', 'artifactRefs')
				)
			BEGIN
				SELECT RAISE(ABORT, 'Legacy artifact fields are not supported');
			END;

			CREATE TRIGGER IF NOT EXISTS {table}_reject_legacy_artifact_fields_update
			BEFORE UPDATE OF data ON {table}
			FOR EACH ROW
			WHEN json_valid(NEW.data)
				AND EXISTS (
					SELECT 1 FROM json_tree(NEW.data)
					WHERE key IN ('artifactIds', 'artifactRefs')
				)
			BEGIN
				SELECT RAISE(ABORT, 'Legacy artifact fields are not supported');
			END;
		"
        ))?;
    }
    Ok(())
}

fn strip_legacy_artifact_fields(value: &js_json::JsValue) -> (js_json::JsValue, bool) {
    use js_json::{JsObject, JsString, JsValue};
    match value {
        JsValue::Array(items) => {
            let mut changed = false;
            let migrated = items
                .iter()
                .map(|item| {
                    let (value, item_changed) = strip_legacy_artifact_fields(item);
                    changed |= item_changed;
                    value
                })
                .collect();
            if changed {
                (JsValue::Array(migrated), true)
            } else {
                (value.clone(), false)
            }
        }
        JsValue::Object(object) => {
            let mut changed = false;
            let mut migrated = JsObject::new();
            for (key, nested) in object {
                if key == &JsString::from("artifactIds") || key == &JsString::from("artifactRefs") {
                    changed = true;
                    continue;
                }
                let (value, nested_changed) = strip_legacy_artifact_fields(nested);
                changed |= nested_changed;
                migrated.insert(key.clone(), value);
            }
            if changed {
                (JsValue::Object(migrated), true)
            } else {
                (value.clone(), false)
            }
        }
        _ => (value.clone(), false),
    }
}

fn migrate_legacy_runtime_mailbox_messages(db: &Connection, now_iso: &str) -> Result<(), ControlDbError> {
    if !table_exists(db, "runtime_mailbox_messages")? {
        return Ok(());
    }
    let columns = table_columns(db, "runtime_mailbox_messages")?;
    if !columns.contains("store_session_path") || !columns.contains("store_message_id") {
        db.execute_batch("DROP TABLE runtime_mailbox_messages")?;
        return Ok(());
    }
    let mut statement = db.prepare(
        "SELECT recipient_session_id, recipient_agent_id, sender_session_id, sender_agent_id,
			        kind, store_session_path, store_message_id, status, created_at, updated_at,
			        error
			 FROM runtime_mailbox_messages
			 WHERE store_session_path IS NOT NULL AND store_message_id IS NOT NULL
			 ORDER BY CASE status WHEN 'delivered' THEN 4 WHEN 'failed' THEN 3 WHEN 'claimed' THEN 2 ELSE 1 END DESC,
			          updated_at DESC, id DESC",
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok(LegacyRuntimeMailboxRow {
                recipient_session_id: row.get(0)?,
                recipient_agent_id: row.get(1)?,
                sender_session_id: row.get(2)?,
                sender_agent_id: row.get(3)?,
                kind: row.get(4)?,
                store_session_path: row.get(5)?,
                store_message_id: row.get(6)?,
                status: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
                error: row.get(10)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut migrated = HashSet::new();
    for row in rows {
        let key = js_format!("{}\u{0}{}", row.store_session_path, row.store_message_id);
        if !migrated.insert(key) {
            continue;
        }
        migrate_legacy_runtime_mailbox_row(db, &row, now_iso)?;
    }
    db.execute_batch(
        "
		DROP INDEX IF EXISTS runtime_mailbox_store_ref_unique_idx;
		DROP INDEX IF EXISTS runtime_mailbox_recipient_status_idx;
		DROP INDEX IF EXISTS runtime_mailbox_created_at_idx;
		DROP TABLE runtime_mailbox_messages;
	",
    )?;
    Ok(())
}

struct LegacyRuntimeMailboxRow {
    recipient_session_id: JsString,
    recipient_agent_id: Option<JsString>,
    sender_session_id: Option<JsString>,
    sender_agent_id: Option<JsString>,
    kind: String,
    store_session_path: JsString,
    store_message_id: JsString,
    status: String,
    created_at: String,
    updated_at: String,
    error: Option<String>,
}

fn migrate_legacy_runtime_mailbox_row(
    db: &Connection,
    row: &LegacyRuntimeMailboxRow,
    now_iso: &str,
) -> Result<(), ControlDbError> {
    let store_ref = super::RuntimeMailboxStoreRef {
        message_id: row.store_message_id.clone(),
        session_path: row.store_session_path.clone(),
    };
    // Legacy payload validation precedes retention cleanup, as in frozen TS.
    let canonical=db.query_row("SELECT rowid,session_path,message_id,data,updated_at FROM multi_agent_mailbox_messages WHERE session_path=? AND message_id=?",params![store_ref.session_path,store_ref.message_id],|r|Ok(super::RuntimeMailboxRow{id:r.get(0)?,session_path:r.get(1)?,message_id:r.get(2)?,data:r.get(3)?,updated_at:r.get(4)?})).optional()?;
    let Some(canonical) = canonical else {
        return Ok(());
    };
    let context = js_format!(
        "multi_agent_mailbox_messages:{}#{}",
        row.store_session_path,
        row.store_message_id
    );
    let message = parse_stored_json_object_lossless(&canonical.data, &context)?;
    validate_mailbox_payload(&Value::Object(message.clone()), &context)?;
    let input = super::EnqueueRuntimeMailboxMessageInput {
        recipient_host: None,
        kind: to_runtime_mailbox_message_kind(&row.kind),
        recipient: super::RuntimeMailboxAddress {
            agent_id: row.recipient_agent_id.clone(),
            session_id: row.recipient_session_id.clone(),
        },
        sender: super::RuntimeMailboxAddress {
            agent_id: row.sender_agent_id.clone(),
            session_id: row.sender_session_id.clone().unwrap_or_default(),
        },
        store_ref,
    };
    let mut updated = add_runtime_mailbox_routing(&message, &input)?;
    if message
        .get("status")
        .and_then(Value::as_string)
        .is_some_and(|status| status == "delivered")
        || row.status == "delivered"
    {
        db.execute(
            "DELETE FROM multi_agent_mailbox_messages WHERE session_path=? AND message_id=?",
            params![input.store_ref.session_path, input.store_ref.message_id],
        )?;
        return Ok(());
    }
    let canonical_status = to_runtime_mailbox_message_status(&require_native_string_field(
        &message,
        "status",
        &JsString::from(&context),
    )?);
    let status = migrated_mailbox_status(row, canonical_status);
    apply_migrated_mailbox_fields(&mut updated, row, status, now_iso);
    write_canonical_mailbox_payload(
        db,
        canonical.id,
        &updated,
        if row.updated_at.is_empty() {
            now_iso
        } else {
            &row.updated_at
        },
    )
}

fn migrated_mailbox_status(
    row: &LegacyRuntimeMailboxRow,
    canonical_status: super::RuntimeMailboxMessageStatus,
) -> super::RuntimeMailboxMessageStatus {
    let is_terminal = canonical_status == super::RuntimeMailboxMessageStatus::Failed;
    if is_terminal {
        return canonical_status;
    }
    if row.status == "claimed" {
        return super::RuntimeMailboxMessageStatus::Pending;
    }
    to_runtime_mailbox_message_status(&row.status)
}

fn apply_migrated_mailbox_fields(
    updated: &mut js_json::JsObject,
    row: &LegacyRuntimeMailboxRow,
    status: super::RuntimeMailboxMessageStatus,
    now_iso: &str,
) {
    use js_json::{JsString, JsValue};
    if !matches!(updated.get(&JsString::from("createdAt")), Some(JsValue::String(_))) {
        updated.insert(
            JsString::from("createdAt"),
            JsValue::String(JsString::from(row.created_at.as_str())),
        );
    }
    updated.insert(
        JsString::from("status"),
        JsValue::String(JsString::from(status_name(status))),
    );
    updated.insert(
        JsString::from("updatedAt"),
        JsValue::String(JsString::from(if row.updated_at.is_empty() {
            now_iso
        } else {
            &row.updated_at
        })),
    );
    updated.shift_remove(&JsString::from("claimedAt"));
    updated.shift_remove(&JsString::from("claimantProcessIdentity"));
    if status == super::RuntimeMailboxMessageStatus::Failed
        && !matches!(updated.get(&JsString::from("error")), Some(JsValue::String(_)))
    {
        updated.insert(
            JsString::from("error"),
            row.error
                .as_deref()
                .map(|text| JsValue::String(JsString::from(text)))
                .unwrap_or(JsValue::Null),
        );
    }
}

fn status_name(status: super::RuntimeMailboxMessageStatus) -> &'static str {
    match status {
        super::RuntimeMailboxMessageStatus::Pending => "pending",
        super::RuntimeMailboxMessageStatus::Claimed => "claimed",
        super::RuntimeMailboxMessageStatus::Failed => "failed",
    }
}

pub(crate) fn add_runtime_mailbox_routing(
    message: &js_json::JsObject,
    input: &super::EnqueueRuntimeMailboxMessageInput,
) -> Result<js_json::JsObject, ControlDbError> {
    use js_json::JsValue;
    assert_runtime_mailbox_routing(message, input)?;
    let mut routed = message.clone();
    if let Some(host) = input.recipient_host.as_ref().filter(|host| !host.is_empty()) {
        routed.insert("recipientHost".into(), JsValue::String(host.clone()));
    }
    routed.insert(
        "recipientSessionId".into(),
        JsValue::String(input.recipient.session_id.clone()),
    );
    routed.insert(
        "recipientAgentId".into(),
        input
            .recipient
            .agent_id
            .clone()
            .map(JsValue::String)
            .unwrap_or(JsValue::Null),
    );
    routed.insert(
        "senderSessionId".into(),
        JsValue::String(input.sender.session_id.clone()),
    );
    routed.insert(
        "senderAgentId".into(),
        input
            .sender
            .agent_id
            .clone()
            .map(JsValue::String)
            .unwrap_or(JsValue::Null),
    );
    Ok(routed)
}

pub(crate) fn assert_runtime_mailbox_routing(
    message: &js_json::JsObject,
    input: &super::EnqueueRuntimeMailboxMessageInput,
) -> Result<(), ControlDbError> {
    use js_json::JsValue;
    let nullable_text =
        |value: Option<&js_json::JsString>| value.cloned().map(JsValue::String).unwrap_or(JsValue::Null);
    let fields = [
        (
            "recipientSessionId",
            JsValue::String(input.recipient.session_id.clone()),
        ),
        ("recipientAgentId", nullable_text(input.recipient.agent_id.as_ref())),
        ("senderSessionId", JsValue::String(input.sender.session_id.clone())),
        ("senderAgentId", nullable_text(input.sender.agent_id.as_ref())),
        ("kind", JsValue::String(kind_name(input.kind).into())),
    ]
    .into_iter()
    .map(|(key, value)| (key.into(), value))
    .collect();
    assert_runtime_mailbox_routing_with_fields(message, input, &fields)
}

pub(crate) fn assert_runtime_mailbox_routing_with_fields(
    message: &js_json::JsObject,
    input: &super::EnqueueRuntimeMailboxMessageInput,
    expected: &js_json::JsObject,
) -> Result<(), ControlDbError> {
    use js_json::{JsString, JsValue};
    let field = |key: &str| message.get(&JsString::from(key));
    if !matches!(field("recipientSessionId"), Some(JsValue::String(_))) {
        return Ok(());
    }
    let same_field = |key: &str| field(key) == expected.get(&JsString::from(key));
    let same_recipient = same_field("recipientSessionId") && same_field("recipientAgentId");
    let same_sender = same_field("senderSessionId") && same_field("senderAgentId");
    let same_kind = same_field("kind");
    let host = super::read_local_hostname();
    let default_host = JsValue::String(host.clone());
    let previous_host = match field("recipientHost") {
        None | Some(JsValue::Null) => &default_host,
        Some(value) => value,
    };
    let input_host = input
        .recipient_host
        .as_ref()
        .filter(|host| !host.is_empty())
        .cloned()
        .unwrap_or(host);
    let same_host = previous_host == &JsValue::String(input_host);
    if same_recipient && same_sender && same_kind && same_host {
        return Ok(());
    }
    Err(ControlDbError::new(js_format!(
        "Runtime mailbox store reference conflicts with canonical mailbox row: {}#{}",
        input.store_ref.session_path,
        input.store_ref.message_id
    )))
}

fn kind_name(kind: super::RuntimeMailboxMessageKind) -> &'static str {
    match kind {
        super::RuntimeMailboxMessageKind::Message => "message",
        super::RuntimeMailboxMessageKind::Ask => "ask",
        super::RuntimeMailboxMessageKind::Reply => "reply",
        super::RuntimeMailboxMessageKind::Steer => "steer",
        super::RuntimeMailboxMessageKind::ParentRequest => "parent_request",
        super::RuntimeMailboxMessageKind::System => "system",
    }
}

fn add_missing_architect_request_columns(db: &Connection) -> Result<(), ControlDbError> {
    let columns = table_columns(db, "architect_requests")?;
    if !columns.contains("project_cwd") {
        db.execute_batch("ALTER TABLE architect_requests ADD COLUMN project_cwd TEXT")?;
    }
    if !columns.contains("claimed_at") {
        db.execute_batch("ALTER TABLE architect_requests ADD COLUMN claimed_at TEXT")?;
    }
    if !columns.contains("claim_token") {
        db.execute_batch("ALTER TABLE architect_requests ADD COLUMN claim_token TEXT")?;
    }
    Ok(())
}

fn add_missing_runtime_mailbox_listener_columns(db: &Connection) -> Result<(), ControlDbError> {
    let columns = table_columns(db, "runtime_mailbox_listeners")?;
    if !columns.contains("runtime_instance_id") {
        db.execute_batch("ALTER TABLE runtime_mailbox_listeners ADD COLUMN runtime_instance_id TEXT")?;
    }
    if !columns.contains("session_path") {
        db.execute_batch("ALTER TABLE runtime_mailbox_listeners ADD COLUMN session_path TEXT")?;
    }
    if !columns.contains("session_path_asserted_at") {
        db.execute_batch("ALTER TABLE runtime_mailbox_listeners ADD COLUMN session_path_asserted_at TEXT")?;
    }
    Ok(())
}

fn add_missing_session_metadata_columns(db: &Connection) -> Result<(), ControlDbError> {
    let columns = table_columns(db, "session_metadata")?;
    if !columns.contains("name") {
        db.execute_batch("ALTER TABLE session_metadata ADD COLUMN name TEXT")?;
    }
    if !columns.contains("parent_session_path") {
        db.execute_batch("ALTER TABLE session_metadata ADD COLUMN parent_session_path TEXT")?;
    }
    if columns.contains("all_messages_text") {
        db.execute_batch("ALTER TABLE session_metadata DROP COLUMN all_messages_text")?;
    }
    if !columns.contains("archived_at") {
        db.execute_batch("ALTER TABLE session_metadata ADD COLUMN archived_at TEXT")?;
    }
    if !columns.contains("goal_json") {
        db.execute_batch("ALTER TABLE session_metadata ADD COLUMN goal_json TEXT")?;
    }
    if !columns.contains("is_subagent") {
        db.execute_batch("ALTER TABLE session_metadata ADD COLUMN is_subagent INTEGER NOT NULL DEFAULT 0")?;
    }
    if !columns.contains("subagent_name") {
        db.execute_batch("ALTER TABLE session_metadata ADD COLUMN subagent_name TEXT")?;
    }
    if !columns.contains("model_provider") {
        db.execute_batch("ALTER TABLE session_metadata ADD COLUMN model_provider TEXT")?;
    }
    if !columns.contains("model_id") {
        db.execute_batch("ALTER TABLE session_metadata ADD COLUMN model_id TEXT")?;
    }
    if !columns.contains("thinking_level") {
        db.execute_batch("ALTER TABLE session_metadata ADD COLUMN thinking_level TEXT")?;
    }
    Ok(())
}

fn add_missing_shared_channel_origin_columns(db: &Connection) -> Result<(), ControlDbError> {
    let columns = table_columns(db, "shared_channel_messages")?;
    if !columns.contains("origin_host") {
        db.execute_batch("ALTER TABLE shared_channel_messages ADD COLUMN origin_host TEXT")?;
    }
    if !columns.contains("origin_id") {
        db.execute_batch("ALTER TABLE shared_channel_messages ADD COLUMN origin_id INTEGER")?;
    }
    db.execute_batch("CREATE UNIQUE INDEX IF NOT EXISTS shared_channel_origin_idx ON shared_channel_messages(origin_host, origin_id) WHERE origin_host IS NOT NULL")?;
    Ok(())
}
