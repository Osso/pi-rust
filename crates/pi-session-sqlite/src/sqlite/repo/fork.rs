//! Fork planning and row copying for the SQLite session repository.

use pi_agent_core::harness::session::{
    EntryOrder, EntryType, ForkOptions, LanePointer, SessionError, SessionErrorCode,
};
use pi_ai::types::JsonObject;

use crate::error::SqliteError;
use crate::sqlite::connection::{Database, transaction};
use crate::sqlite::repo::decode::entry_row_from_cached;
use crate::sqlite::repo::{ResolvedWriterLeaseOptions, claim_writer_lease};
use crate::sqlite::storage::branch_entries::{CachedBranchQuery, query_cached_branch_rows, read_cached_branch};
use crate::sqlite::storage::branch_tips::read_branch_tip_ids;
use crate::sqlite::storage::entries::{
    EntryRow, EntryRowsQuery, NewEntryRow, insert_entry_row, read_entry_row, read_entry_rows,
};
use crate::sqlite::storage::facts::{append_fact, read_latest_fact, read_latest_label_facts};
use crate::sqlite::storage::lanes::{create_initial_lane, create_lane, read_lane, read_lanes};
use crate::sqlite::storage::session_sequences::{create_sequence, set_next_sequence};
use crate::sqlite::storage::session_stats::create_stats;
use crate::sqlite::storage::sessions::{NewSessionRow, insert_session_row};
use crate::sqlite::storage::writer_leases::WriterLease;

/// Everything a fork copies from its source.
pub(crate) struct ForkPlan {
    pub(crate) entries: Vec<EntryRow>,
    pub(crate) lanes: Vec<LanePointer>,
    pub(crate) branch_tips: Vec<String>,
    pub(crate) branch_fork_target_id: Option<String>,
    pub(crate) name_value: Option<String>,
    pub(crate) labels: Vec<(Option<String>, String)>,
    pub(crate) is_tree: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ForkPosition {
    At,
    Before,
}

fn fork_selection(options: &ForkOptions) -> (Option<String>, Option<ForkPosition>) {
    match options {
        ForkOptions::Branch => (None, None),
        ForkOptions::BranchDefaultBefore => (None, Some(ForkPosition::Before)),
        ForkOptions::BranchAt { entry_id } => (Some(entry_id.clone()), Some(ForkPosition::At)),
        ForkOptions::BranchBefore { entry_id } => (Some(entry_id.clone()), Some(ForkPosition::Before)),
        ForkOptions::Tree => (None, None),
    }
}

/// Reads the entries, lanes, tips, and facts a fork must copy.
pub(crate) fn plan_fork(db: &Database, source_id: &str, options: &ForkOptions) -> Result<ForkPlan, SessionError> {
    let mut plan = ForkPlan {
        entries: Vec::new(),
        lanes: Vec::new(),
        branch_tips: Vec::new(),
        branch_fork_target_id: None,
        name_value: None,
        labels: Vec::new(),
        is_tree: matches!(options, ForkOptions::Tree),
    };
    db.with_connection(|connection| {
        if plan.is_tree {
            collect_tree_rows(connection, source_id, &mut plan)?;
        } else {
            collect_branch_rows(connection, source_id, options, &mut plan)?;
        }
        collect_fork_facts(connection, source_id, &mut plan)
    })
    .map_err(SqliteError::into_session)?;
    Ok(plan)
}

fn collect_tree_rows(
    connection: &rusqlite::Connection,
    source_id: &str,
    plan: &mut ForkPlan,
) -> Result<(), SqliteError> {
    plan.entries = read_entry_rows(
        connection,
        source_id,
        &EntryRowsQuery {
            order: Some(EntryOrder::OldestFirst),
            ..EntryRowsQuery::default()
        },
    )?;
    plan.lanes = read_lanes(connection, source_id)?
        .into_iter()
        .map(|row| LanePointer {
            lane: row.lane,
            leaf_id: row.leaf_id,
        })
        .collect();
    plan.branch_tips = read_branch_tip_ids(connection, source_id)?;
    Ok(())
}

fn collect_branch_rows(
    connection: &rusqlite::Connection,
    source_id: &str,
    options: &ForkOptions,
    plan: &mut ForkPlan,
) -> Result<(), SqliteError> {
    let Some(main) = read_lane(connection, source_id, "main")? else {
        return Err(SqliteError::Session(SessionError::new(
            SessionErrorCode::InvalidLane,
            "Lane not found: main",
        )));
    };
    let (explicit_entry_id, explicit_position) = fork_selection(options);
    let selected_entry_id = explicit_entry_id.clone().or(main.leaf_id.clone());
    if let Some(selected_entry_id) = selected_entry_id {
        plan.branch_fork_target_id = fork_target_id(
            connection,
            source_id,
            &selected_entry_id,
            explicit_entry_id.is_some(),
            explicit_position,
        )?;
    }
    plan.lanes.push(LanePointer {
        lane: "main".to_owned(),
        leaf_id: plan.branch_fork_target_id.clone(),
    });
    if let Some(target_id) = plan.branch_fork_target_id.clone() {
        append_cached_branch(connection, source_id, &target_id, plan)?;
    }
    Ok(())
}

fn append_cached_branch(
    connection: &rusqlite::Connection,
    source_id: &str,
    target_id: &str,
    plan: &mut ForkPlan,
) -> Result<(), SqliteError> {
    let Some(cached) = read_cached_branch(connection, source_id, target_id)? else {
        return Err(invalid_fork_target(format!(
            "Fork target is not on a cached branch: {target_id}"
        )));
    };
    let rows = query_cached_branch_rows(
        connection,
        source_id,
        cached,
        &CachedBranchQuery {
            order: Some(EntryOrder::OldestFirst),
            ..CachedBranchQuery::default()
        },
    )?;
    plan.entries.extend(rows.iter().map(entry_row_from_cached));
    plan.branch_tips.push(target_id.to_owned());
    Ok(())
}

fn fork_target_id(
    connection: &rusqlite::Connection,
    source_id: &str,
    selected_entry_id: &str,
    explicit_entry: bool,
    explicit_position: Option<ForkPosition>,
) -> Result<Option<String>, SqliteError> {
    let target = read_entry_row(connection, source_id, selected_entry_id)?;
    let is_message = target
        .as_ref()
        .is_some_and(|target| target.entry_type == EntryType::Message.as_str());
    let Some(target) = target.filter(|_| is_message) else {
        return Err(invalid_fork_target(format!(
            "Fork target is not a message entry: {selected_entry_id}"
        )));
    };
    let position = explicit_position.unwrap_or(if explicit_entry {
        ForkPosition::Before
    } else {
        ForkPosition::At
    });
    Ok(match position {
        ForkPosition::At => Some(target.id),
        ForkPosition::Before => target.parent_id,
    })
}

fn invalid_fork_target(message: String) -> SqliteError {
    SqliteError::Session(SessionError::new(SessionErrorCode::InvalidForkTarget, message))
}

fn collect_fork_facts(
    connection: &rusqlite::Connection,
    source_id: &str,
    plan: &mut ForkPlan,
) -> Result<(), SqliteError> {
    plan.name_value = read_latest_fact(connection, source_id, "name", None)?.and_then(|row| row.value);
    let copied_ids: Vec<&str> = plan.entries.iter().map(|entry| entry.id.as_str()).collect();
    plan.labels = read_latest_label_facts(connection, source_id)?
        .into_iter()
        .filter(|(key, _)| plan.is_tree || key.as_ref().is_some_and(|key| copied_ids.iter().any(|id| id == key)))
        .collect();
    Ok(())
}

/// The new session's identity and ownership fields.
pub(crate) struct ForkCopy<'a> {
    pub(crate) id: &'a str,
    pub(crate) created_at: i64,
    pub(crate) cwd: &'a str,
    pub(crate) parent_session_id: Option<String>,
    pub(crate) metadata: Option<JsonObject>,
}

/// Copies the plan into a new session inside one transaction and claims its writer lease.
pub(crate) fn copy_fork(
    db: &Database,
    copy: ForkCopy<'_>,
    plan: &ForkPlan,
    lease_options: ResolvedWriterLeaseOptions,
) -> Result<WriterLease, SessionError> {
    let id = copy.id;
    let created_at = copy.created_at;
    db.with_connection(|connection| {
        transaction(connection, |connection| {
            let mut next_seq = 1_i64;
            insert_fork_session(connection, &copy, plan)?;
            insert_fork_entries(connection, id, &plan.entries, &mut next_seq)?;
            insert_fork_lanes(connection, id, plan, &mut next_seq)?;
            insert_fork_facts(connection, id, plan, &mut next_seq)?;
            set_next_sequence(connection, id, next_seq)?;
            for tip in &plan.branch_tips {
                crate::sqlite::branch_cache::build_cached_branch(connection, id, tip)?;
            }
            claim_writer_lease(connection, id, lease_options, created_at)
        })
        .map_err(|error| match error {
            SqliteError::Session(error) => SqliteError::Session(error),
            other => SqliteError::Session(SessionError::new(
                SessionErrorCode::Storage,
                format!("Failed to fork SQLite session {id}: {other}"),
            )),
        })
    })
    .map_err(SqliteError::into_session)
}

fn insert_fork_session(
    connection: &rusqlite::Connection,
    copy: &ForkCopy<'_>,
    plan: &ForkPlan,
) -> Result<(), SqliteError> {
    insert_session_row(
        connection,
        &NewSessionRow {
            id: copy.id.to_owned(),
            created_at: copy.created_at,
            cwd: copy.cwd.to_owned(),
            parent_session_id: copy.parent_session_id.clone(),
            metadata: copy.metadata.clone(),
        },
    )?;
    create_sequence(connection, copy.id, 1)?;
    let message_count = plan
        .entries
        .iter()
        .filter(|entry| entry.entry_type == EntryType::Message.as_str())
        .count() as u64;
    create_stats(connection, copy.id, message_count)
}

fn insert_fork_entries(
    connection: &rusqlite::Connection,
    id: &str,
    entries: &[EntryRow],
    next_seq: &mut i64,
) -> Result<(), SqliteError> {
    for entry in entries {
        insert_entry_row(
            connection,
            id,
            &NewEntryRow {
                seq: *next_seq,
                id: entry.id.clone(),
                parent_id: entry.parent_id.clone(),
                entry_type: entry.entry_type.clone(),
                timestamp: entry.timestamp,
                payload: entry.payload.clone(),
            },
        )?;
        *next_seq += 1;
    }
    Ok(())
}

fn insert_fork_lanes(
    connection: &rusqlite::Connection,
    id: &str,
    plan: &ForkPlan,
    next_seq: &mut i64,
) -> Result<(), SqliteError> {
    if !plan.is_tree {
        return create_initial_lane(connection, id, "main", plan.branch_fork_target_id.as_deref());
    }
    for lane in &plan.lanes {
        create_lane(connection, id, *next_seq, &lane.lane, lane.leaf_id.as_deref())?;
        *next_seq += 1;
    }
    Ok(())
}

fn insert_fork_facts(
    connection: &rusqlite::Connection,
    id: &str,
    plan: &ForkPlan,
    next_seq: &mut i64,
) -> Result<(), SqliteError> {
    if let Some(name) = plan.name_value.as_deref() {
        append_fact(connection, id, *next_seq, "name", None, Some(name))?;
        *next_seq += 1;
    }
    for (key, value) in &plan.labels {
        append_fact(connection, id, *next_seq, "label", key.as_deref(), Some(value))?;
        *next_seq += 1;
    }
    Ok(())
}
