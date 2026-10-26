//! Behavioral tests for the shared control database (`core/session-control-db.ts`).

use pi_js_compat::js_json::{self, JsObject, JsString};
use rusqlite::OptionalExtension;
use std::path::PathBuf;

use pi_coding_agent::core::multi_agent_store::{
    AgentError, AgentFileReference, AgentLifecycle, AgentNode, AgentPermission, AgentResult,
};
use pi_coding_agent::core::runtime_process::ProcessIdentity;
use pi_coding_agent::core::session_control_db::{
    CommitMultiAgentLifecycleMutationInput, CommitMultiAgentSteeringMutationInput,
    CommitMultiAgentTerminalMutationInput, CreateMultiAgentChildWithRuntimeOwnershipInput, DetachedJobOutcome,
    DetachedJobTerminalInput, DetachedJobTerminalOutput, EnqueueRuntimeMailboxMessageInput,
    EnqueueStoredRuntimeMailboxMessageInput, FinalizeDetachedJobInput, FinalizeDetachedJobResult,
    MultiAgentCounterName, MultiAgentCounters, MultiAgentRuntimeOwnerIdentity, PostArchitectRequestInput,
    PostSharedChannelMessageInput, PostSupervisorRequestInput, RuntimeMailboxAddress, RuntimeMailboxMessageKind,
    RuntimeMailboxMessageStatus, RuntimeMailboxStoreRef, SupervisorRequestKind, SupervisorResponse,
    WritableSessionMetadata, advance_shared_channel_cursor, allocate_multi_agent_counter, archive_session,
    archive_sessions_older_than, bootstrap_multi_agent_agent, claim_latest_incoming_message,
    claim_multi_agent_terminal_outbox, claim_next_supervisor_request, claim_pending_architect_requests,
    claim_runtime_mailbox_messages, cleanup_multi_agent_terminal_outbox, commit_multi_agent_detach_mark,
    commit_multi_agent_lifecycle_mutation, commit_multi_agent_steering_delivery, commit_multi_agent_steering_mutation,
    commit_multi_agent_terminal_mutation, complete_architect_request, complete_incoming_message,
    complete_supervisor_request, create_multi_agent_child_with_runtime_ownership, deliver_multi_agent_terminal_outbox,
    deliver_runtime_mailbox_message, enqueue_incoming_message, enqueue_runtime_mailbox_message,
    enqueue_stored_runtime_mailbox_message, fail_incoming_message, fail_runtime_mailbox_message, finalize_detached_job,
    get_multi_agent_mailbox_message_status, has_pending_supervisor_approval_request,
    initialize_shared_channel_cursor_at_tail, list_active_session_metadata, list_archived_session_metadata,
    list_pending_architect_requests, list_runtime_mailbox_listeners, list_runtime_mailbox_messages,
    list_session_health, list_session_metadata, list_shared_channel_messages_after,
    mark_runtime_mailbox_message_delivered, normalize_session_name, post_architect_request,
    post_shared_channel_message, post_supervisor_request, read_incoming_message_status, read_last_message,
    read_multi_agent_agent, read_multi_agent_state, read_or_migrate_prompt_history, read_prompt_history,
    read_runtime_mailbox_listener, read_runtime_mailbox_message, read_session_goal, read_session_health,
    read_session_metadata, read_session_name_state, read_session_sandbox_profile, read_shared_channel_cursor,
    read_shared_channel_tail, read_supervisor_request, record_prompt_history_entry, register_runtime_mailbox_listener,
    relocate_session_control_data, remove_session_metadata, renew_architect_request_claims, requeue_supervisor_request,
    retire_runtime_mailbox_listener, unarchive_session, upsert_multi_agent_mailbox_message, write_last_message,
    write_multi_agent_counters, write_session_goal, write_session_health, write_session_metadata, write_session_model,
    write_session_name, write_session_sandbox_profile, write_session_thinking_level,
};
use pi_coding_agent::core::session_health::{SessionCheckStatus, empty_session_health};
use tempfile::TempDir;

fn temp_db() -> (TempDir, String) {
    let directory = tempfile::tempdir().expect("temp dir");
    let path = directory.path().join("control.sqlite");
    (directory, path.display().to_string())
}

fn identity(pid: u32, start: u64) -> ProcessIdentity {
    ProcessIdentity {
        pid,
        start_time_ticks: start,
        incarnation: None,
    }
}

fn owner(session: &str) -> MultiAgentRuntimeOwnerIdentity {
    MultiAgentRuntimeOwnerIdentity {
        session_id: session.into(),
        agent_id: None,
    }
}

fn metadata(session_path: &str, id: &str, name: Option<&str>) -> WritableSessionMetadata {
    WritableSessionMetadata {
        session_path: session_path.into(),
        id: id.into(),
        cwd: "/repo".into(),
        name: name.map(JsString::from),
        parent_session_path: None,
        archived_at: None,
        is_archived: None,
        is_subagent: None,
        subagent_name: None,
        model_provider: None,
        model_id: None,
        thinking_level: None,
        created_at: "2026-06-21T00:00:00.000Z".into(),
        modified_at: "2026-06-21T00:00:00.000Z".into(),
        message_count: 1,
        first_message: "hello".into(),
    }
}

fn agent_node(id: &str, parent: Option<&str>, lifecycle: AgentLifecycle) -> AgentNode {
    AgentNode {
        key_order: Vec::new(),
        id: id.into(),
        parent_id: parent.map(JsString::from),
        display_name: "Scout".into(),
        agent_type: "scout".into(),
        origin: None,
        lifecycle,
        revision: 1,
        created_at: "2026-06-21T00:00:00.000Z".into(),
        updated_at: "2026-06-21T00:00:00.000Z".into(),
        cwd: "/repo".into(),
        worktree: None,
        model: None,
        account: None,
        permission: AgentPermission {
            policy: "on-request".into(),
            inherited_from: None,
            narrowed: true,
        },
        slot: None,
        transcript: None,
        event_stream: None,
        worker: None,
        detached: None,
        current_activity: None,
        last_activity: None,
        result: None,
        error: None,
        extra: pi_js_compat::js_json::JsObject::new(),
    }
}

fn register_listener(db: &str, recipient: &RuntimeMailboxAddress) {
    register_runtime_mailbox_listener(db, recipient, std::process::id(), None, Default::default())
        .expect("register listener");
}

fn stored_message(
    db: &str,
    session_path: &str,
    message_id: &str,
    recipient: &RuntimeMailboxAddress,
    sender: &RuntimeMailboxAddress,
    body: &str,
) -> i64 {
    register_listener(db, recipient);
    let message = pi_js_compat::js_json!({
        "body": body,
        // Frozen session-control-db.ts:2331 compares birth against Date.now().
        "createdAt": pi_coding_agent::utils::iso_time::now_iso_string(),
        "fromAgentId": sender.agent_id.clone().unwrap_or_else(|| "main".into()),
        "id": message_id,
        "kind": "message",
        "status": "pending",
        "toAgentId": recipient.agent_id.clone().unwrap_or_else(|| "main".into()),
        "updatedAt": "2026-06-21T00:00:00.000Z",
    });
    enqueue_stored_runtime_mailbox_message(
        db,
        &EnqueueStoredRuntimeMailboxMessageInput {
            recipient: recipient.clone(),
            recipient_host: None,
            sender: sender.clone(),
            kind: RuntimeMailboxMessageKind::Message,
            store_ref: RuntimeMailboxStoreRef {
                session_path: session_path.into(),
                message_id: message_id.into(),
            },
            message,
            updated_at: Some("2026-06-21T00:00:00.000Z".into()),
        },
    )
    .expect("enqueue stored message")
}

#[test]
fn schema_and_pragmas_match_the_shared_contract() {
    let (_dir, db) = temp_db();
    let health = empty_session_health("session", "2026-06-21T00:00:00.000Z");
    write_session_health(&db, &health).expect("write health");

    let connection = pi_coding_agent::core::sqlite::create_sqlite_database(&db).expect("open db");
    pi_coding_agent::core::sqlite::configure_shared_sqlite_database(&connection, 5000).expect("configure");
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("user_version");
    assert_eq!(version, 15);
    let journal: String = connection
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .expect("journal mode");
    assert_eq!(journal.to_lowercase(), "wal");
    let synchronous: i64 = connection
        .query_row("PRAGMA synchronous", [], |row| row.get(0))
        .expect("synchronous");
    assert_eq!(synchronous, 1);
    let tables: Vec<String> = connection
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
        .expect("prepare")
        .query_map([], |row| row.get(0))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("rows");
    for table in [
        "architect_requests",
        "incoming_messages",
        "last_message",
        "multi_agent_agents",
        "multi_agent_counters_v2",
        "multi_agent_mailbox_messages",
        "multi_agent_runtime_owners",
        "multi_agent_terminal_outbox",
        "prompt_history",
        "runtime_mailbox_listeners",
        "session_health",
        "session_metadata",
        "session_sandbox_profiles",
        "shared_channel_cursors",
        "shared_channel_messages",
        "supervisor_requests",
    ] {
        assert!(tables.iter().any(|name| name == table), "missing {table}");
    }
    assert!(
        !tables.iter().any(|name| name == "named_sessions"),
        "v15 stores names only in session_metadata"
    );
    let metadata_columns: Vec<String> = connection
        .prepare("PRAGMA table_info(session_metadata)")
        .expect("prepare columns")
        .query_map([], |row| row.get(1))
        .expect("query columns")
        .collect::<Result<_, _>>()
        .expect("columns");
    assert!(metadata_columns.iter().any(|column| column == "name"));
}

#[test]
fn incoming_messages_claim_supersedes_pending_rows() {
    let (_dir, db) = temp_db();
    let first = enqueue_incoming_message(&db, "first").expect("enqueue first");
    let second = enqueue_incoming_message(&db, "second").expect("enqueue second");
    assert_eq!(first + 1, second);

    let claimed = claim_latest_incoming_message(&db).expect("claim").expect("message");
    assert_eq!(claimed.id, second);
    assert_eq!(claimed.content, "second");
    assert_eq!(
        read_incoming_message_status(&db, first).expect("status"),
        Some("superseded".into())
    );
    assert_eq!(
        read_incoming_message_status(&db, second).expect("status"),
        Some("claimed".into())
    );
    complete_incoming_message(&db, second).expect("complete");
    assert_eq!(
        read_incoming_message_status(&db, second).expect("status"),
        Some("completed".into())
    );

    let third = enqueue_incoming_message(&db, "third").expect("enqueue third");
    fail_incoming_message(&db, third, &JsString::from("boom")).expect("fail");
    assert_eq!(
        read_incoming_message_status(&db, third).expect("status"),
        Some("failed".into())
    );
}

#[test]
fn session_metadata_round_trips_and_archives() {
    let (_dir, db) = temp_db();
    write_session_metadata(&db, &metadata("/sessions/a.jsonl", "id-a", None)).expect("write a");
    write_session_metadata(&db, &metadata("/sessions/b.jsonl", "id-b", None)).expect("write b");

    let read = read_session_metadata(&db, "/sessions/a.jsonl")
        .expect("read")
        .expect("metadata");
    assert_eq!(read.id, "id-a");
    assert_eq!(read.cwd, "/repo");
    assert_eq!(read.message_count, 1);
    assert_eq!(read.first_message, "hello");
    assert_eq!(read.is_subagent, Some(false));

    write_session_model(&db, "/sessions/a.jsonl", "anthropic", "claude").expect("model");
    write_session_thinking_level(&db, "/sessions/a.jsonl", "high").expect("thinking");
    let read = read_session_metadata(&db, "/sessions/a.jsonl")
        .expect("read")
        .expect("metadata");
    assert_eq!(read.model_provider.as_deref(), Some("anthropic"));
    assert_eq!(read.model_id.as_deref(), Some("claude"));
    assert_eq!(read.thinking_level.as_deref(), Some("high"));

    write_session_goal(
        &db,
        "/sessions/a.jsonl",
        Some(&JsString::from("{\"objective\":\"ship\"}")),
    )
    .expect("goal");
    assert_eq!(
        read_session_goal(&db, "/sessions/a.jsonl").expect("read goal").clone(),
        Some("{\"objective\":\"ship\"}".into())
    );
    write_session_goal(&db, "/sessions/a.jsonl", None).expect("clear goal");
    assert_eq!(read_session_goal(&db, "/sessions/a.jsonl").expect("read goal"), None);

    assert_eq!(list_session_metadata(&db).expect("list").len(), 2);
    archive_session(&db, "/sessions/a.jsonl").expect("archive");
    assert_eq!(list_active_session_metadata(&db).expect("active").len(), 1);
    assert_eq!(list_archived_session_metadata(&db).expect("archived").len(), 1);
    unarchive_session(&db, "/sessions/a.jsonl").expect("unarchive");
    assert_eq!(list_active_session_metadata(&db).expect("active").len(), 2);

    let archived = archive_sessions_older_than(&db, "2026-06-22T00:00:00.000Z").expect("archive older");
    assert_eq!(archived.len(), 2);
}

#[test]
fn session_names_live_only_in_session_metadata() {
    let (_dir, db) = temp_db();
    for (id, name) in [("session-a", "Alpha"), ("session-b", "Beta")] {
        write_session_metadata(&db, &metadata(&format!("/tmp/{id}.jsonl"), id, None)).expect("metadata");
        write_session_name(&db, format!("/tmp/{id}.jsonl"), Some(&JsString::from(name))).expect("name");
    }
    write_session_name(&db, "/tmp/session-a.jsonl", Some(&JsString::from("Alpha renamed"))).expect("rename");

    let mut names: Vec<(JsString, Option<JsString>)> = list_session_metadata(&db)
        .expect("list")
        .into_iter()
        .map(|session| (session.session_path, session.name))
        .collect();
    names.sort();
    assert_eq!(
        names,
        vec![
            ("/tmp/session-a.jsonl".into(), Some("Alpha renamed".into())),
            ("/tmp/session-b.jsonl".into(), Some("Beta".into())),
        ]
    );

    write_session_name(&db, "/tmp/session-a.jsonl", None).expect("clear");
    assert_eq!(
        read_session_metadata(&db, "/tmp/session-a.jsonl")
            .expect("read")
            .expect("metadata")
            .name,
        None
    );
    let state = read_session_name_state(&db, "/tmp/session-a.jsonl").expect("state");
    assert_eq!(state.name, None);
    assert!(state.has_stored_value, "an explicit clear is still a stored value");
    assert_eq!(
        read_session_metadata(&db, "/tmp/session-b.jsonl")
            .expect("read")
            .expect("metadata")
            .name
            .clone(),
        Some("Beta".into())
    );
}

#[test]
fn session_name_normalization_matches_the_typescript_helper() {
    assert_eq!(
        normalize_session_name(Some(&JsString::from("  Legacy\r\nPreferred\nName "))),
        Some("Legacy Preferred Name".into())
    );
    assert_eq!(normalize_session_name(Some(&JsString::from("\n\r\n"))), None);
    assert_eq!(normalize_session_name(Some(&JsString::from("   "))), None);
    assert_eq!(normalize_session_name(None), None);
    assert_eq!(
        normalize_session_name(Some(&JsString::from("Kept"))),
        Some("Kept".into())
    );
}

#[test]
fn write_session_name_requires_a_metadata_row() {
    let (_dir, db) = temp_db();
    let error = write_session_name(&db, "/sessions/missing.jsonl", Some(&JsString::from("Name")))
        .expect_err("missing metadata");
    assert_eq!(
        error.message(),
        "Cannot update session name without metadata for /sessions/missing.jsonl"
    );
    let state = read_session_name_state(&db, "/sessions/missing.jsonl").expect("state");
    assert_eq!(state.name, None);
    assert!(!state.has_stored_value);
}

#[test]
fn generic_metadata_snapshots_preserve_dedicated_session_names() {
    let (_dir, db) = temp_db();
    write_session_metadata(&db, &metadata("/tmp/session-a.jsonl", "session-a", None)).expect("metadata");
    write_session_name(&db, "/tmp/session-a.jsonl", Some(&JsString::from("Alpha"))).expect("name");
    let mut updated = metadata("/tmp/session-a.jsonl", "session-a", None);
    updated.message_count = 2;
    write_session_metadata(&db, &updated).expect("snapshot");
    assert_eq!(
        read_session_metadata(&db, "/tmp/session-a.jsonl")
            .expect("read")
            .expect("metadata")
            .name
            .clone(),
        Some("Alpha".into())
    );

    write_session_name(&db, "/tmp/session-a.jsonl", None).expect("clear");
    updated.message_count = 3;
    write_session_metadata(&db, &updated).expect("snapshot after clear");
    assert_eq!(
        read_session_metadata(&db, "/tmp/session-a.jsonl")
            .expect("read")
            .expect("metadata")
            .name,
        None
    );
    assert!(
        read_session_name_state(&db, "/tmp/session-a.jsonl")
            .expect("state")
            .has_stored_value
    );
}

#[test]
fn updates_existing_session_metadata_without_changing_its_session_path_or_name() {
    let (_dir, db) = temp_db();
    write_session_metadata(&db, &metadata("/tmp/session-a.jsonl", "session-a", Some("Original"))).expect("metadata");
    let mut updated = metadata("/tmp/session-a.jsonl", "session-a-renamed", None);
    updated.cwd = "/repo/a2".into();
    updated.parent_session_path = Some("/tmp/parent.jsonl".into());
    updated.message_count = 3;
    updated.first_message = "updated first".into();
    write_session_metadata(&db, &updated).expect("update");
    let read = read_session_metadata(&db, "/tmp/session-a.jsonl")
        .expect("read")
        .expect("metadata");
    assert_eq!(read.id, "session-a-renamed");
    assert_eq!(read.cwd, "/repo/a2");
    assert_eq!(read.name.clone(), Some("Original".into()));
    assert_eq!(read.parent_session_path.clone(), Some("/tmp/parent.jsonl".into()));
}

#[test]
fn named_sessions_and_relocation_move_all_control_rows() {
    let (_dir, db) = temp_db();
    write_session_metadata(&db, &metadata("/sessions/old.jsonl", "id", None)).expect("metadata");
    write_session_name(&db, "/sessions/old.jsonl", Some(&JsString::from("Named"))).expect("name");
    assert_eq!(
        read_session_metadata(&db, "/sessions/old.jsonl")
            .expect("read")
            .expect("metadata")
            .name
            .clone(),
        Some("Named".into())
    );

    write_session_sandbox_profile(
        &db,
        "/sessions/old.jsonl",
        "id",
        pi_coding_agent::core::permissions::presets::SandboxProfileName::WorkspaceWrite,
    )
    .expect("sandbox");
    relocate_session_control_data(&db, "/sessions/old.jsonl", "/sessions/new.jsonl").expect("relocate");
    assert!(
        read_session_metadata(&db, "/sessions/old.jsonl")
            .expect("read")
            .is_none()
    );
    let moved = read_session_metadata(&db, "/sessions/new.jsonl")
        .expect("read")
        .expect("metadata");
    assert_eq!(moved.id, "id");
    assert_eq!(moved.name.clone(), Some("Named".into()), "the name moves with the row");
    assert_eq!(
        read_session_sandbox_profile(&db, "/sessions/new.jsonl", "id").expect("profile"),
        Some(pi_coding_agent::core::permissions::presets::SandboxProfileName::WorkspaceWrite)
    );

    remove_session_metadata(&db, "/sessions/new.jsonl").expect("remove metadata");
    assert!(
        read_session_metadata(&db, "/sessions/new.jsonl")
            .expect("read")
            .is_none()
    );
}

#[test]
fn prompt_history_deduplicates_and_migrates_legacy_entries() {
    let (_dir, db) = temp_db();
    record_prompt_history_entry(&db, "  first  ").expect("first");
    record_prompt_history_entry(&db, "first").expect("duplicate");
    record_prompt_history_entry(&db, "second").expect("second");
    assert_eq!(read_prompt_history(&db, 10).expect("history"), vec!["second", "first"]);

    let (_dir2, db2) = temp_db();
    let migrated =
        read_or_migrate_prompt_history(&db2, &["one".into(), "two".into(), "two".into(), "three".into()], 10)
            .expect("migrate");
    assert_eq!(migrated, vec!["one", "two", "two", "three"]);
    assert_eq!(
        read_prompt_history(&db2, 10).expect("history"),
        vec!["one", "two", "three"]
    );
}

#[test]
fn last_message_only_returns_assistant_roles() {
    let (_dir, db) = temp_db();
    assert!(read_last_message(&db).expect("read").is_none());
    write_last_message(&db, "assistant", "hello").expect("write");
    let message = read_last_message(&db).expect("read").expect("message");
    assert_eq!(message.role, "assistant");
    assert_eq!(message.content, "hello");
    write_last_message(&db, "user", "ignored").expect("write");
    assert!(read_last_message(&db).expect("read").is_none());
}

#[test]
fn session_health_and_listener_registration_track_generations() {
    let (_dir, db) = temp_db();
    let recipient = RuntimeMailboxAddress {
        session_id: "session-1".into(),
        agent_id: None,
    };
    register_listener(&db, &recipient);
    let health = read_session_health(&db, &JsString::from("session-1"))
        .expect("read")
        .expect("health");
    assert_eq!(health.pid, Some(std::process::id()));
    assert_eq!(health.agent_generation, 1);
    assert_eq!(health.check_status, SessionCheckStatus::Ok);
    assert_eq!(health.checked_generation, Some(1));

    register_listener(&db, &recipient);
    let health = read_session_health(&db, &JsString::from("session-1"))
        .expect("read")
        .expect("health");
    assert_eq!(health.agent_generation, 1);

    let listener = read_runtime_mailbox_listener(&db, &recipient)
        .expect("listener")
        .expect("row");
    assert_eq!(listener.pid, std::process::id());
    assert_eq!(list_runtime_mailbox_listeners(&db).expect("listeners").len(), 1);

    assert!(retire_runtime_mailbox_listener(&db, &recipient, std::process::id()).expect("retire"));
    let health = read_session_health(&db, &JsString::from("session-1"))
        .expect("read")
        .expect("health");
    assert_eq!(health.check_status, SessionCheckStatus::Dead);
    assert_eq!(list_session_health(&db).expect("list").len(), 1);
}

#[test]
fn runtime_mailbox_claim_deliver_and_consume() {
    let (_dir, db) = temp_db();
    let recipient = RuntimeMailboxAddress {
        session_id: "recipient".into(),
        agent_id: None,
    };
    let sender = RuntimeMailboxAddress {
        session_id: "sender".into(),
        agent_id: None,
    };
    let id = stored_message(&db, "/sessions/sender.jsonl", "message_1", &recipient, &sender, "hello");

    let claimed = claim_runtime_mailbox_messages(&db, &recipient, 20).expect("claim");
    assert_eq!(claimed.len(), 1);
    assert_eq!(claimed[0].body, "hello");
    assert_eq!(claimed[0].status, RuntimeMailboxMessageStatus::Claimed);
    assert_eq!(claimed[0].id, id);

    assert!(deliver_runtime_mailbox_message(&db, id, None).expect("deliver"));
    // Frozen session-control-db.ts:1092-1101 delivers by deleting the canonical row.
    assert!(read_runtime_mailbox_message(&db, id).expect("read").is_none());
    assert!(list_runtime_mailbox_messages(&db).expect("list").is_empty());
    assert!(!deliver_runtime_mailbox_message(&db, id, None).expect("already delivered"));

    let second = stored_message(&db, "/sessions/sender.jsonl", "message_2", &recipient, &sender, "again");
    let _ = claim_runtime_mailbox_messages(&db, &recipient, 20).expect("claim");
    fail_runtime_mailbox_message(&db, second, "boom").expect("fail");
    let message = read_runtime_mailbox_message(&db, second)
        .expect("read")
        .expect("message");
    assert_eq!(message.status, RuntimeMailboxMessageStatus::Failed);
    assert_eq!(message.error.clone(), Some("boom".into()));

    let third = stored_message(&db, "/sessions/sender.jsonl", "message_3", &recipient, &sender, "again");
    let _ = claim_runtime_mailbox_messages(&db, &recipient, 20).expect("claim");
    mark_runtime_mailbox_message_delivered(&db, third).expect("mark delivered");
    // Frozen session-control-db.ts:2281-2288 also deletes rows when marking delivered.
    assert!(read_runtime_mailbox_message(&db, third).expect("read").is_none());
}

#[test]
fn runtime_mailbox_routing_preserves_payload_order_and_identity() {
    use pi_coding_agent::core::session_control_db::{
        retain_control_db_connection, with_control_db, with_immediate_transaction,
    };
    let (_dir, db) = temp_db();
    let recipient = RuntimeMailboxAddress {
        session_id: "recipient".into(),
        agent_id: None,
    };
    let sender = RuntimeMailboxAddress {
        session_id: "sender".into(),
        agent_id: None,
    };
    let store_ref = RuntimeMailboxStoreRef {
        session_path: "/sessions/sender.jsonl".into(),
        message_id: "message_1".into(),
    };
    upsert_multi_agent_mailbox_message(
        &db,
        &store_ref.session_path,
        &store_ref.message_id,
        &pi_js_compat::js_json!({
            "body": "hello",
            "fromAgentId": "main",
            "id": "message_1",
            "kind": "message",
            "status": "pending",
            "toAgentId": "main",
        }),
    )
    .expect("upsert");
    register_listener(&db, &recipient);
    let input = EnqueueRuntimeMailboxMessageInput {
        recipient: recipient.clone(),
        recipient_host: None,
        sender: sender.clone(),
        kind: RuntimeMailboxMessageKind::Message,
        store_ref: store_ref.clone(),
    };
    let path = JsString::from(db.as_str());
    let _retained = retain_control_db_connection(&path).expect("retain");
    let nested = with_control_db(&path, |connection| {
        with_immediate_transaction(connection, |_| enqueue_runtime_mailbox_message(&db, &input))
    });
    let error = nested.expect_err("routing owns its immediate transaction, as frozen does");
    assert_eq!(
        error.message(),
        &JsString::from("cannot start a transaction within a transaction")
    );
    let id = enqueue_runtime_mailbox_message(&db, &input).expect("route");
    assert_eq!(id, 1);

    let connection = pi_coding_agent::core::sqlite::create_sqlite_database(&db).expect("open");
    let data: String = connection
        .query_row(
            "SELECT data FROM multi_agent_mailbox_messages WHERE session_path = ? AND message_id = ?",
            rusqlite::params![store_ref.session_path, store_ref.message_id],
            |row| row.get(0),
        )
        .expect("row");
    // Frozen session-control-db.ts defaults omitted creation time before routing.
    let payload = js_json::parse_lossless(&data).expect("payload");
    let birth = &payload["createdAt"];
    let birth_json = js_json::stringify_lossless(birth);
    assert!(pi_js_compat::date::date_parse_utf16(birth.as_string().expect("birth string").units()).is_finite());
    assert_eq!(
        data,
        format!(
            "{{\"body\":\"hello\",\"fromAgentId\":\"main\",\"id\":\"message_1\",\"kind\":\"message\",\"status\":\"pending\",\"toAgentId\":\"main\",\"createdAt\":{birth_json},\"recipientSessionId\":\"recipient\",\"recipientAgentId\":null,\"senderSessionId\":\"sender\",\"senderAgentId\":null}}"
        )
    );

    // Re-routing the same identity is a no-op and preserves the row.
    enqueue_runtime_mailbox_message(
        &db,
        &EnqueueRuntimeMailboxMessageInput {
            recipient,
            recipient_host: None,
            sender,
            kind: RuntimeMailboxMessageKind::Message,
            store_ref,
        },
    )
    .expect("reroute");
}

#[test]
fn shared_channel_posts_and_cursors() {
    let (_dir, db) = temp_db();
    assert_eq!(read_shared_channel_tail(&db).expect("tail"), 0);
    let first = post_shared_channel_message(
        &db,
        &PostSharedChannelMessageInput {
            sender: RuntimeMailboxAddress {
                session_id: "one".into(),
                agent_id: None,
            },
            body: "  hello  ".into(),
        },
    )
    .expect("post");
    let second = post_shared_channel_message(
        &db,
        &PostSharedChannelMessageInput {
            sender: RuntimeMailboxAddress {
                session_id: "two".into(),
                agent_id: Some("agent".into()),
            },
            body: "world".into(),
        },
    )
    .expect("post");
    assert_eq!(first + 1, second);
    assert_eq!(read_shared_channel_tail(&db).expect("tail"), second);

    let messages = list_shared_channel_messages_after(&db, 0, 20, i64::MAX).expect("messages");
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].body, "hello");
    assert_eq!(messages[1].sender.agent_id.clone(), Some("agent".into()));

    let recipient = RuntimeMailboxAddress {
        session_id: "reader".into(),
        agent_id: None,
    };
    assert_eq!(
        initialize_shared_channel_cursor_at_tail(&db, &recipient).expect("cursor"),
        second
    );
    advance_shared_channel_cursor(&db, &recipient, second).expect("advance");
    assert_eq!(
        read_shared_channel_cursor(&db, &recipient).expect("cursor"),
        Some(second)
    );

    let error = post_shared_channel_message(
        &db,
        &PostSharedChannelMessageInput {
            sender: recipient.clone(),
            body: "   ".into(),
        },
    )
    .expect_err("empty body");
    assert_eq!(error.message(), "shared channel message body must be non-empty");
}

#[test]
fn architect_requests_claim_renew_and_complete() {
    let (_dir, db) = temp_db();
    let error = post_architect_request(
        &db,
        &PostArchitectRequestInput {
            sender_session_id: "sender".into(),
            project_cwd: "/repo".into(),
            body: "  ".into(),
        },
    )
    .expect_err("empty");
    assert_eq!(error.message(), "Architect request body must be non-empty");

    let id = post_architect_request(
        &db,
        &PostArchitectRequestInput {
            sender_session_id: "sender".into(),
            project_cwd: "/repo".into(),
            body: "  question  ".into(),
        },
    )
    .expect("post");
    assert_eq!(list_pending_architect_requests(&db, 20).expect("pending").len(), 1);
    let claimed = claim_pending_architect_requests(&db, &JsString::from("token"), 20).expect("claim");
    assert_eq!(claimed.len(), 1);
    assert_eq!(claimed[0].id, id);
    assert_eq!(
        claimed[0].status,
        pi_coding_agent::core::session_control_db::ArchitectRequestStatus::Claimed
    );

    renew_architect_request_claims(&db, &[id], &JsString::from("token")).expect("renew");
    let lost = renew_architect_request_claims(&db, &[id], &JsString::from("other")).expect_err("lost");
    assert_eq!(*lost.message(), format!("Architect request claim lost: {id}"));

    complete_architect_request(&db, id, &JsString::from("token"), Some(&JsString::from("sender"))).expect("complete");
    assert!(list_pending_architect_requests(&db, 20).expect("pending").is_empty());
}

#[test]
fn supervisor_requests_prioritize_approvals_and_validate_responses() {
    let (_dir, db) = temp_db();
    let advisory = post_supervisor_request(
        &db,
        &PostSupervisorRequestInput {
            sender_session_id: "sender".into(),
            project_id: "project".into(),
            kind: SupervisorRequestKind::SupervisorAdvisory,
            payload: JsObject::new(),
            deadline_at: "2099-01-01T00:00:00.000Z".into(),
        },
    )
    .expect("post advisory");
    let approval = post_supervisor_request(
        &db,
        &PostSupervisorRequestInput {
            sender_session_id: "sender".into(),
            project_id: "project".into(),
            kind: SupervisorRequestKind::ApprovalReview,
            payload: JsObject::new(),
            deadline_at: "2099-01-01T00:00:00.000Z".into(),
        },
    )
    .expect("post approval");
    assert!(has_pending_supervisor_approval_request(&db).expect("pending"));

    let claimed = claim_next_supervisor_request(&db, &JsString::from("claim"))
        .expect("claim")
        .expect("request");
    assert_eq!(claimed.id, approval);
    assert_eq!(
        claimed.status,
        pi_coding_agent::core::session_control_db::SupervisorRequestStatus::Claimed
    );
    assert!(
        claim_next_supervisor_request(&db, &JsString::from("other"))
            .expect("claim")
            .is_none()
    );

    let invalid = complete_supervisor_request(
        &db,
        approval,
        &JsString::from("claim"),
        &pi_js_compat::js_json!({"kind":"complete","reason":"done"}),
    )
    .expect_err("invalid");
    assert_eq!(
        invalid.message(),
        "Invalid Supervisor response kind complete for approval_review"
    );
    complete_supervisor_request(
        &db,
        approval,
        &JsString::from("claim"),
        &pi_js_compat::js_json!({"kind":"approve","reason":"ok"}),
    )
    .expect("complete");
    let read = read_supervisor_request(&db, approval).expect("read").expect("request");
    assert_eq!(
        read.response
            .as_ref()
            .map(|value| js_json::from_value::<SupervisorResponse>(value).unwrap()),
        Some(SupervisorResponse::Approve { reason: "ok".into() })
    );

    let next = claim_next_supervisor_request(&db, &JsString::from("claim-2"))
        .expect("claim")
        .expect("request");
    assert_eq!(next.id, advisory);
    requeue_supervisor_request(&db, advisory, &JsString::from("claim-2")).expect("requeue");
    assert!(requeue_supervisor_request(&db, advisory, &JsString::from("claim-2")).is_err());
}

#[test]
fn supervisor_recovery_completes_expired_requests() {
    let (_dir, db) = temp_db();
    let expired = post_supervisor_request(
        &db,
        &PostSupervisorRequestInput {
            sender_session_id: "sender".into(),
            project_id: "project".into(),
            kind: SupervisorRequestKind::GoalCompletionReview,
            payload: JsObject::new(),
            deadline_at: "2000-01-01T00:00:00.000Z".into(),
        },
    )
    .expect("post");
    pi_coding_agent::core::session_control_db::recover_supervisor_requests(&db).expect("recover");
    let read = read_supervisor_request(&db, expired).expect("read").expect("request");
    assert_eq!(
        read.response
            .as_ref()
            .map(|value| js_json::from_value::<SupervisorResponse>(value).unwrap()),
        Some(SupervisorResponse::Error {
            reason: "Supervisor request deadline expired".into()
        })
    );
}

#[test]
fn multi_agent_counters_allocate_monotonic_sequences() {
    let (_dir, db) = temp_db();
    assert_eq!(
        allocate_multi_agent_counter(&db, "/sessions/a.jsonl", MultiAgentCounterName::Agent).expect("agent"),
        1
    );
    assert_eq!(
        allocate_multi_agent_counter(&db, "/sessions/a.jsonl", MultiAgentCounterName::Agent).expect("agent"),
        2
    );
    assert_eq!(
        allocate_multi_agent_counter(&db, "/sessions/a.jsonl", MultiAgentCounterName::Message).expect("message"),
        1
    );
    write_multi_agent_counters(
        &db,
        "/sessions/b.jsonl",
        &MultiAgentCounters {
            next_agent_number: 7,
            next_message_number: 9,
        },
    )
    .expect("write counters");
    assert_eq!(
        allocate_multi_agent_counter(&db, "/sessions/b.jsonl", MultiAgentCounterName::Agent).expect("agent"),
        7
    );
    assert_eq!(
        allocate_multi_agent_counter(&db, "/sessions/b.jsonl", MultiAgentCounterName::Message).expect("message"),
        9
    );
}

#[test]
fn child_creation_requires_active_parent_and_running_revision_one() {
    let (_dir, db) = temp_db();
    let process_identity = identity(std::process::id(), 1);
    let mut child = agent_node("child", Some("main"), AgentLifecycle::Running);
    child.revision = 1;
    let result = create_multi_agent_child_with_runtime_ownership(
        &db,
        &CreateMultiAgentChildWithRuntimeOwnershipInput {
            session_path: "/sessions/main.jsonl".into(),
            agent_id: "child".into(),
            process_identity: process_identity.clone(),
            owner: owner("owner"),
            now_iso: "2026-06-21T00:00:00.000Z".into(),
            agent: child.clone(),
        },
    )
    .expect("create");
    assert!(matches!(
        result,
        pi_coding_agent::core::session_control_db::CreateMultiAgentChildWithRuntimeOwnershipResult::Ok { .. }
    ));

    let duplicate = create_multi_agent_child_with_runtime_ownership(
        &db,
        &CreateMultiAgentChildWithRuntimeOwnershipInput {
            session_path: "/sessions/main.jsonl".into(),
            agent_id: "child".into(),
            process_identity,
            owner: owner("owner"),
            now_iso: "2026-06-21T00:00:00.000Z".into(),
            agent: child,
        },
    )
    .expect("create");
    assert_eq!(
        duplicate,
        pi_coding_agent::core::session_control_db::CreateMultiAgentChildWithRuntimeOwnershipResult::Err {
            error: pi_coding_agent::core::session_control_db::CreateChildError::AgentExists
        }
    );

    let mut invalid = agent_node("bad", Some("main"), AgentLifecycle::Running);
    invalid.revision = 2;
    let error = create_multi_agent_child_with_runtime_ownership(
        &db,
        &CreateMultiAgentChildWithRuntimeOwnershipInput {
            session_path: "/sessions/main.jsonl".into(),
            agent_id: "bad".into(),
            process_identity: identity(std::process::id(), 1),
            owner: owner("owner"),
            now_iso: "2026-06-21T00:00:00.000Z".into(),
            agent: invalid,
        },
    )
    .expect_err("invalid revision");
    assert_eq!(error.message(), "Child runtime ownership requires running revision 1");
}

#[test]
fn lifecycle_and_terminal_mutations_write_the_outbox() {
    let (_dir, db) = temp_db();
    let process_identity = identity(std::process::id(), 1);
    let session_path = "/sessions/main.jsonl";
    let mut child = agent_node("child", Some("main"), AgentLifecycle::Running);
    create_multi_agent_child_with_runtime_ownership(
        &db,
        &CreateMultiAgentChildWithRuntimeOwnershipInput {
            session_path: session_path.into(),
            agent_id: "child".into(),
            process_identity: process_identity.clone(),
            owner: owner("owner"),
            now_iso: "2026-06-21T00:00:00.000Z".into(),
            agent: child.clone(),
        },
    )
    .expect("create");

    let waiting = commit_multi_agent_lifecycle_mutation(
        &db,
        &CommitMultiAgentLifecycleMutationInput {
            session_path: session_path.into(),
            agent_id: "child".into(),
            process_identity: process_identity.clone(),
            owner: owner("owner"),
            requested_lifecycle: "waiting_for_input".into(),
            updated_at: "2026-06-21T00:00:01.000Z".into(),
            detached_cancellation: None,
        },
    )
    .expect("waiting");
    assert!(matches!(
        waiting,
        pi_coding_agent::core::session_control_db::CommitMultiAgentLifecycleMutationResult::Ok { .. }
    ));
    child = read_multi_agent_agent(&db, session_path, "child")
        .expect("read")
        .expect("agent");
    assert_eq!(child.lifecycle, AgentLifecycle::WaitingForInput);
    assert_eq!(child.revision, 2);

    let terminal = commit_multi_agent_terminal_mutation(
        &db,
        &CommitMultiAgentTerminalMutationInput {
            session_path: session_path.into(),
            agent_id: "child".into(),
            process_identity: process_identity.clone(),
            owner: owner("owner"),
            terminal_lifecycle: "completed".into(),
            event_kind: "completed".into(),
            agent_details: pi_coding_agent::core::session_control_db::AgentDetails {
                error: None,
                result: Some(
                    js_json::to_value(&AgentResult {
                        summary: Some("done".into()),
                        file_refs: Some(vec![AgentFileReference {
                            path: "/tmp/out".into(),
                            label: Some("Output".into()),
                        }]),
                        duration_ms: Some(12),
                        tool_call_id: None,
                    })
                    .expect("result"),
                ),
            },
            updated_at: "2026-06-21T00:00:02.000Z".into(),
        },
    )
    .expect("terminal");
    assert_eq!(
        terminal,
        pi_coding_agent::core::session_control_db::CommitMultiAgentTerminalMutationResult::Ok { terminal_revision: 3 }
    );

    let record = claim_multi_agent_terminal_outbox(
        &db,
        &JsString::from("claim"),
        "2026-06-21T00:00:03.000Z",
        &Default::default(),
    )
    .expect("claim")
    .expect("record");
    assert_eq!(record.terminal_revision, 3);
    assert_eq!(record.attempt_count, 1);
    assert!(deliver_multi_agent_terminal_outbox(&db, &record).expect("deliver"));
    assert_eq!(
        // Frozen session-control-db.ts: delivered outbox rows are deleted immediately.
        cleanup_multi_agent_terminal_outbox(&db, "2026-06-21T00:00:05.000Z").expect("cleanup"),
        0
    );
}

#[test]
fn steering_mutations_require_registered_authority() {
    let (_dir, db) = temp_db();
    let process_identity =
        pi_coding_agent::core::runtime_process::read_process_identity(std::process::id()).expect("identity");
    let session_path = "/sessions/main.jsonl";
    let mut child = agent_node("child", Some("main"), AgentLifecycle::Running);
    child.transcript = Some(pi_coding_agent::core::multi_agent_store::AgentTranscriptMetadata {
        session_id: "child-session".into(),
        path: None,
    });
    create_multi_agent_child_with_runtime_ownership(
        &db,
        &CreateMultiAgentChildWithRuntimeOwnershipInput {
            session_path: session_path.into(),
            agent_id: "child".into(),
            process_identity: process_identity.clone(),
            owner: owner("supervisor"),
            now_iso: "2026-06-21T00:00:00.000Z".into(),
            agent: child,
        },
    )
    .expect("create");
    let recipient = RuntimeMailboxAddress {
        session_id: "child-session".into(),
        agent_id: Some("child".into()),
    };
    register_listener(&db, &recipient);
    register_runtime_mailbox_listener(
        &db,
        &RuntimeMailboxAddress {
            session_id: "supervisor".into(),
            agent_id: None,
        },
        std::process::id(),
        Some(&JsString::from(session_path)),
        Default::default(),
    )
    .expect("register supervisor listener");

    let result = commit_multi_agent_steering_mutation(
        &db,
        &CommitMultiAgentSteeringMutationInput {
            session_path: session_path.into(),
            agent_id: "child".into(),
            process_identity: process_identity.clone(),
            owner: owner("supervisor"),
            requested_lifecycle: "steering_pending".into(),
            updated_at: pi_coding_agent::utils::iso_time::now_iso_string(),
            body: "steer".into(),
            file_refs: None,
            from_agent_id: "supervisor".into(),
            recipient: recipient.clone(),
            target_checkpoint: None,
            thread_id: None,
        },
    )
    .expect("steer");
    let pi_coding_agent::core::session_control_db::CommitMultiAgentSteeringMutationResult::Ok { agent, message } =
        result
    else {
        panic!("expected steering ok");
    };
    assert_eq!(agent.lifecycle, AgentLifecycle::SteeringPending);
    assert_eq!(message.id, "message_1");
    assert_eq!(message.body.clone(), Some("steer".into()));

    let delivery = commit_multi_agent_steering_delivery(
        &db,
        &pi_coding_agent::core::session_control_db::CommitMultiAgentSteeringDeliveryInput {
            session_path: session_path.into(),
            agent_id: "child".into(),
            process_identity,
            owner: owner("supervisor"),
            requested_lifecycle: "running".into(),
            updated_at: "2026-06-21T00:00:02.000Z".into(),
            message_id: "message_1".into(),
        },
    )
    .expect("deliver");
    assert!(matches!(
        delivery,
        pi_coding_agent::core::session_control_db::CommitMultiAgentSteeringDeliveryResult::Ok { .. }
    ));
    let message = pi_coding_agent::core::session_control_db::get_multi_agent_mailbox_message_status(
        &db,
        session_path,
        &JsString::from("message_1"),
    )
    .expect("status");
    // Frozen session-control-db.ts steering delivery deletes the row.
    assert_eq!(message, None);
}

#[test]
fn detach_mark_is_idempotent_and_fenced_by_ownership() {
    let (_dir, db) = temp_db();
    let process_identity = identity(std::process::id(), 1);
    let session_path = "/sessions/main.jsonl";
    let child = agent_node("child", Some("main"), AgentLifecycle::Running);
    create_multi_agent_child_with_runtime_ownership(
        &db,
        &CreateMultiAgentChildWithRuntimeOwnershipInput {
            session_path: session_path.into(),
            agent_id: "child".into(),
            process_identity: process_identity.clone(),
            owner: owner("owner"),
            now_iso: "2026-06-21T00:00:00.000Z".into(),
            agent: child,
        },
    )
    .expect("create");

    let input = pi_coding_agent::core::session_control_db::CommitMultiAgentDetachMarkInput {
        agent_id: "child".into(),
        owner: owner("owner"),
        process_identity: process_identity.clone(),
        session_path: session_path.into(),
        updated_at: "2026-06-21T00:00:01.000Z".into(),
    };
    let first = commit_multi_agent_detach_mark(&db, &input).expect("detach");
    let pi_coding_agent::core::session_control_db::CommitMultiAgentDetachMarkResult::Ok { agent } = first else {
        panic!("expected detach ok");
    };
    assert_eq!(agent.detached, Some(true));
    assert_eq!(agent.revision, 2);

    let second = commit_multi_agent_detach_mark(&db, &input).expect("detach again");
    let pi_coding_agent::core::session_control_db::CommitMultiAgentDetachMarkResult::Ok { agent } = second else {
        panic!("expected detach ok");
    };
    assert_eq!(agent.revision, 2);

    let mismatch = commit_multi_agent_detach_mark(
        &db,
        &pi_coding_agent::core::session_control_db::CommitMultiAgentDetachMarkInput {
            owner: owner("other"),
            ..input
        },
    )
    .expect("detach mismatch");
    assert!(matches!(
        mismatch,
        pi_coding_agent::core::session_control_db::CommitMultiAgentDetachMarkResult::Err { .. }
    ));
}

#[test]
fn finalize_detached_job_records_terminal_transport_and_artifacts() {
    let (_dir, db) = temp_db();
    let process_identity = identity(std::process::id(), 1);
    let session_path = "/sessions/main.jsonl";
    let mut child = agent_node("bash_1", Some("main"), AgentLifecycle::Running);
    child.detached = Some(true);
    child.worker = Some(pi_coding_agent::core::multi_agent_store::AgentWorkerAdapter {
        adapter: pi_coding_agent::core::multi_agent_store::AgentWorkerAdapterKind::Runtime,
        handle_id: std::process::id().to_string().into(),
        cwd: None,
        tool_call_id: Some("tool-1".into()),
    });
    create_multi_agent_child_with_runtime_ownership(
        &db,
        &CreateMultiAgentChildWithRuntimeOwnershipInput {
            session_path: session_path.into(),
            agent_id: "bash_1".into(),
            process_identity: process_identity.clone(),
            owner: owner("owner"),
            now_iso: "2026-06-21T00:00:00.000Z".into(),
            agent: child,
        },
    )
    .expect("create");

    let result = finalize_detached_job(
        &db,
        &FinalizeDetachedJobInput {
            session_path: session_path.into(),
            terminal: DetachedJobTerminalInput {
                job_id: "bash_1".into(),
                owner: owner("owner"),
                process_identity,
                output_label: "Bash output".into(),
                terminal_at: pi_coding_agent::utils::iso_time::now_iso_string().into(),
                tool_call_id: Some("tool-1".into()),
                duration_ms: Some(42),
                outcome: DetachedJobOutcome::Completed {
                    exit_code: Some(0),
                    summary: Some("ok".into()),
                },
                output: DetachedJobTerminalOutput {
                    label: "Bash output".into(),
                    path: "/tmp/output.log".into(),
                    size: 3,
                    sha256: "abc".into(),
                },
            },
        },
    )
    .expect("finalize");
    let FinalizeDetachedJobResult::Ok {
        terminal_agent,
        terminal_revision,
    } = result
    else {
        panic!("expected finalize ok");
    };
    assert_eq!(terminal_revision, 2);
    assert_eq!(terminal_agent.lifecycle, AgentLifecycle::Completed);
    assert_eq!(terminal_agent.worker, None);
    let result = terminal_agent.result.expect("result");
    assert_eq!(result.summary.clone(), Some("ok".into()));
    assert_eq!(result.duration_ms, Some(42));
    assert_eq!(result.tool_call_id.clone(), Some("tool-1".into()));
    let file_refs = result.file_refs.expect("file refs");
    assert_eq!(file_refs[0].path, "/tmp/output.log");

    let messages = list_runtime_mailbox_messages(&db).expect("messages");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].kind, RuntimeMailboxMessageKind::System);
    assert!(messages[0].body.contains("\"type\":\"multi_agent_terminal\""));
    assert_eq!(
        messages[0].store_ref.message_id,
        "terminal:bash_1:2:detached_job_completed"
    );
}

#[test]
fn bootstrap_refuses_to_mutate_owned_rows() {
    let (_dir, db) = temp_db();
    let process_identity = identity(std::process::id(), 1);
    let session_path = "/sessions/main.jsonl";
    let child = agent_node("child", Some("main"), AgentLifecycle::Running);
    create_multi_agent_child_with_runtime_ownership(
        &db,
        &CreateMultiAgentChildWithRuntimeOwnershipInput {
            session_path: session_path.into(),
            agent_id: "child".into(),
            process_identity,
            owner: owner("owner"),
            now_iso: "2026-06-21T00:00:00.000Z".into(),
            agent: child.clone(),
        },
    )
    .expect("create");
    let value = js_json::to_value(&child).expect("value");
    let error = bootstrap_multi_agent_agent(&db, session_path, &JsString::from("child"), &value).expect_err("owned");
    assert_eq!(
        *error.message(),
        format!("Generic agent upsert cannot mutate process-owned lifecycle row {session_path}#child")
    );

    let unowned = agent_node("plain", Some("main"), AgentLifecycle::Running);
    bootstrap_multi_agent_agent(
        &db,
        session_path,
        &JsString::from("plain"),
        &js_json::to_value(&unowned).expect("value"),
    )
    .expect("bootstrap");
    let state = read_multi_agent_state(&db, session_path)
        .expect("state")
        .expect("persisted state");
    assert_eq!(state.agents.len(), 2);
    assert_eq!(state.counters.next_agent_number, 1);
}

#[test]
fn legacy_database_migrates_to_schema_fifteen() {
    let (directory, path) = temp_db();
    let connection = rusqlite::Connection::open(&path).expect("open legacy");
    connection
        .execute_batch(
            "
            CREATE TABLE multi_agent_counters (
                session_path TEXT PRIMARY KEY,
                next_agent_number INTEGER NOT NULL,
                next_message_number INTEGER NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE TABLE multi_agent_agents (
                session_path TEXT NOT NULL,
                agent_id TEXT NOT NULL,
                data TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY (session_path, agent_id)
            );
            CREATE TABLE multi_agent_mailbox_messages (
                session_path TEXT NOT NULL,
                message_id TEXT NOT NULL,
                data TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY (session_path, message_id)
            );
            CREATE TABLE multi_agent_dispatch_leases (
                session_path TEXT NOT NULL,
                agent_id TEXT NOT NULL,
                process_identity TEXT,
                owner_session_id TEXT,
                owner_agent_id TEXT,
                PRIMARY KEY (session_path, agent_id)
            );
            INSERT INTO multi_agent_counters VALUES ('/sessions/a.jsonl', 4, 7, '2026-01-01T00:00:00.000Z');
            INSERT INTO multi_agent_agents VALUES (
                '/sessions/a.jsonl', 'agent_1',
                '{\"id\":\"agent_1\",\"lifecycle\":\"running\",\"revision\":2,\"artifactIds\":[\"legacy\"]}',
                '2026-01-01T00:00:00.000Z'
            );
            ",
        )
        .expect("legacy schema");
    drop(connection);

    let _ = read_session_health(&path, &JsString::from("missing")).expect("initializes");
    let connection = rusqlite::Connection::open(&path).expect("open migrated");
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("version");
    assert_eq!(version, 15);
    let counters: (i64, i64) = connection
        .query_row(
            "SELECT next_agent_number, next_message_number FROM multi_agent_counters_v2 WHERE session_path = ?",
            rusqlite::params!["/sessions/a.jsonl"],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("counters");
    assert_eq!(counters, (4, 7));
    let migrated: String = connection
        .query_row(
            "SELECT data FROM multi_agent_agents WHERE agent_id = 'agent_1'",
            [],
            |row| row.get(0),
        )
        .expect("agent");
    assert!(!migrated.contains("artifactIds"));
    let outbox: String = connection
        .query_row(
            "SELECT event_kind FROM multi_agent_terminal_outbox WHERE agent_id = 'agent_1'",
            [],
            |row| row.get(0),
        )
        .expect("outbox");
    assert_eq!(outbox, "lost_runtime");
    let triggers: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'trigger' AND name LIKE '%reject_legacy_artifact_fields%'",
            [],
            |row| row.get(0),
        )
        .expect("triggers");
    assert_eq!(triggers, 4);

    let error = connection
        .execute(
            "INSERT INTO multi_agent_agents (session_path, agent_id, data, updated_at) VALUES (?, ?, ?, ?)",
            rusqlite::params![
                "/sessions/a.jsonl",
                "agent_2",
                "{\"artifactRefs\":[]}",
                "2026-01-01T00:00:00.000Z"
            ],
        )
        .expect_err("trigger rejects legacy fields");
    assert!(error.to_string().contains("Legacy artifact fields are not supported"));
    drop(directory);
}

const VERSION_FOURTEEN_SCHEMA: &str = "
    CREATE TABLE session_metadata (
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
        all_messages_text TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );
    CREATE TABLE named_sessions (
        session_path TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );
    INSERT INTO session_metadata (
        session_path, id, cwd, name, created_at, modified_at,
        message_count, first_message, all_messages_text, updated_at
    ) VALUES
        ('/sessions/legacy-name.jsonl', 'legacy-name', '/repo', 'Metadata Name',
         '2026-08-01T00:00:00.000Z', '2026-08-01T00:01:00.000Z', 1, 'first', 'first',
         '2026-08-01T00:01:00.000Z'),
        ('/sessions/metadata-only.jsonl', 'metadata-only', '/repo', 'Metadata Only',
         '2026-08-01T00:00:00.000Z', '2026-08-01T00:01:00.000Z', 1, 'first', 'first',
         '2026-08-01T00:01:00.000Z');
    INSERT INTO named_sessions (session_path, name, updated_at) VALUES
        ('/sessions/legacy-name.jsonl', ' Legacy' || char(13) || char(10) || 'Preferred' || char(10) || 'Name ', '2026-08-02T00:00:00.000Z'),
        ('/sessions/orphan.jsonl', 'Orphan Name', '2026-08-02T00:00:00.000Z');
    PRAGMA user_version = 14;
";

fn create_version_fourteen_database(path: &str) {
    let connection = rusqlite::Connection::open(path).expect("open v14");
    connection.execute_batch(VERSION_FOURTEEN_SCHEMA).expect("v14 schema");
}

#[test]
fn version_fourteen_session_names_migrate_into_metadata_and_drop_the_legacy_table() {
    let (_dir, db) = temp_db();
    create_version_fourteen_database(&db);

    let first_initialization = (
        read_session_metadata(&db, "/sessions/legacy-name.jsonl")
            .expect("first read")
            .expect("metadata")
            .name,
        read_session_metadata(&db, "/sessions/metadata-only.jsonl")
            .expect("first read")
            .expect("metadata")
            .name,
    );
    let second_initialization = (
        read_session_metadata(&db, "/sessions/legacy-name.jsonl")
            .expect("second read")
            .expect("metadata")
            .name,
        read_session_metadata(&db, "/sessions/metadata-only.jsonl")
            .expect("second read")
            .expect("metadata")
            .name,
    );
    assert_eq!(
        first_initialization,
        (Some("Legacy Preferred Name".into()), Some("Metadata Only".into())),
        "legacy rows win for matching metadata and normalization applies"
    );
    assert_eq!(second_initialization, first_initialization, "migration is idempotent");

    let connection = rusqlite::Connection::open(&db).expect("open migrated");
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("version");
    assert_eq!(version, 15);
    let legacy_table: Option<String> = connection
        .query_row(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'named_sessions'",
            [],
            |row| row.get(0),
        )
        .optional()
        .expect("legacy table query");
    assert_eq!(legacy_table, None, "the migration drops named_sessions");
    let stored: Vec<(String, Option<String>)> = connection
        .prepare("SELECT session_path, name FROM session_metadata ORDER BY session_path")
        .expect("prepare")
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("rows");
    assert_eq!(
        stored,
        vec![
            (
                "/sessions/legacy-name.jsonl".into(),
                Some("Legacy Preferred Name".into())
            ),
            ("/sessions/metadata-only.jsonl".into(), Some("Metadata Only".into())),
        ],
        "orphan legacy rows are discarded and metadata-only rows are untouched"
    );
}

#[test]
fn session_name_migration_requires_runtime_quiescence() {
    let (_dir, db) = temp_db();
    create_version_fourteen_database(&db);
    let connection = rusqlite::Connection::open(&db).expect("open");
    connection
        .execute_batch(
            "
            CREATE TABLE runtime_mailbox_listeners (
                recipient_session_id TEXT NOT NULL,
                recipient_agent_id_key TEXT NOT NULL,
                pid INTEGER NOT NULL,
                runtime_instance_id TEXT,
                session_path TEXT,
                session_path_asserted_at TEXT,
                updated_at TEXT NOT NULL,
                PRIMARY KEY (recipient_session_id, recipient_agent_id_key)
            );
            ",
        )
        .expect("listener table");
    connection
        .execute(
            "INSERT INTO runtime_mailbox_listeners (recipient_session_id, recipient_agent_id_key, pid, updated_at)
             VALUES ('live-runtime', '', ?, '2026-08-23T00:00:00.000Z')",
            rusqlite::params![std::process::id()],
        )
        .expect("live listener");
    drop(connection);

    let error = read_session_metadata(&db, "/sessions/legacy-name.jsonl").expect_err("quiescence blocks");
    let message = error.message();
    assert!(
        message.contains("Cannot activate session name schema version 15 while lifecycle owners are active"),
        "unexpected message: {message:?}"
    );
    assert!(
        message.contains(std::process::id().to_string()),
        "message lists the live pid"
    );

    let blocked = rusqlite::Connection::open(&db).expect("open blocked");
    let version: i64 = blocked
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("version");
    assert_eq!(version, 14);
    let legacy_table: Option<String> = blocked
        .query_row(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'named_sessions'",
            [],
            |row| row.get(0),
        )
        .optional()
        .expect("legacy table query");
    assert_eq!(legacy_table.as_deref(), Some("named_sessions"));
    blocked
        .execute(
            "DELETE FROM runtime_mailbox_listeners WHERE recipient_session_id = 'live-runtime'",
            [],
        )
        .expect("clear listener");
    drop(blocked);

    assert_eq!(
        read_session_metadata(&db, "/sessions/legacy-name.jsonl")
            .expect("read after quiescence")
            .expect("metadata")
            .name
            .clone(),
        Some("Legacy Preferred Name".into())
    );
}

#[test]
fn unsupported_future_schema_is_rejected() {
    let (_directory, path) = temp_db();
    let connection = rusqlite::Connection::open(&path).expect("open");
    connection.execute_batch("PRAGMA user_version = 99").expect("version");
    drop(connection);
    let error = read_session_health(&path, &JsString::from("missing")).expect_err("future schema");
    assert_eq!(
        error.message(),
        "Unsupported control database schema version 99; this Pi runtime supports up to version 15"
    );
}

#[test]
fn retained_connection_survives_and_releases() {
    let (_directory, path) = temp_db();
    let guard = pi_coding_agent::core::session_control_db::retain_control_db_connection(&path).expect("retain");
    enqueue_incoming_message(&path, "while retained").expect("enqueue");
    guard.release();
    let claimed = claim_latest_incoming_message(&path).expect("claim").expect("message");
    assert_eq!(claimed.content, "while retained");
}

#[test]
fn concurrent_writers_share_the_database_without_loss() {
    let (_directory, path) = temp_db();
    let threads = 4;
    let per_thread = 20;
    let mut handles = Vec::new();
    for thread in 0..threads {
        let path = path.clone();
        handles.push(std::thread::spawn(move || {
            for index in 0..per_thread {
                post_shared_channel_message(
                    &path,
                    &PostSharedChannelMessageInput {
                        sender: RuntimeMailboxAddress {
                            session_id: format!("session-{thread}").into(),
                            agent_id: None,
                        },
                        body: format!("message-{thread}-{index}").into(),
                    },
                )
                .expect("post");
            }
        }));
    }
    for handle in handles {
        handle.join().expect("writer");
    }
    let messages = list_shared_channel_messages_after(&path, 0, 1000, i64::MAX).expect("messages");
    assert_eq!(messages.len(), threads * per_thread);
    assert_eq!(
        read_shared_channel_tail(&path).expect("tail"),
        (threads * per_thread) as i64
    );
}

#[test]
fn detached_artifact_listing_filters_by_label_and_lifecycle() {
    let (_dir, db) = temp_db();
    let session_path = "/sessions/main.jsonl";
    let mut completed = agent_node("bash_1", Some("main"), AgentLifecycle::Completed);
    completed.updated_at = "2026-06-21T00:00:01.000Z".into();
    completed.result = Some(AgentResult {
        summary: None,
        file_refs: Some(vec![AgentFileReference {
            path: "/tmp/output.log".into(),
            label: Some("Bash output".into()),
        }]),
        duration_ms: None,
        tool_call_id: None,
    });
    bootstrap_multi_agent_agent(
        &db,
        session_path,
        &JsString::from("bash_1"),
        &js_json::to_value(&completed).expect("value"),
    )
    .expect("bootstrap");
    let mut other = agent_node("other", Some("main"), AgentLifecycle::Completed);
    other.result = Some(AgentResult {
        summary: None,
        file_refs: Some(vec![AgentFileReference {
            path: "/tmp/other.log".into(),
            label: Some("Other".into()),
        }]),
        duration_ms: None,
        tool_call_id: None,
    });
    bootstrap_multi_agent_agent(
        &db,
        session_path,
        &JsString::from("other"),
        &js_json::to_value(&other).expect("value"),
    )
    .expect("bootstrap");

    let records = pi_coding_agent::core::session_control_db::list_detached_artifact_agents_updated_at_or_before(
        &db,
        "2026-06-21T00:00:02.000Z",
    )
    .expect("records");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].agent.id, "bash_1");
}

#[test]
fn agent_error_type_is_persisted_through_terminal_mutation() {
    let (_dir, db) = temp_db();
    let process_identity = identity(std::process::id(), 1);
    let session_path = "/sessions/main.jsonl";
    let child = agent_node("child", Some("main"), AgentLifecycle::Running);
    create_multi_agent_child_with_runtime_ownership(
        &db,
        &CreateMultiAgentChildWithRuntimeOwnershipInput {
            session_path: session_path.into(),
            agent_id: "child".into(),
            process_identity: process_identity.clone(),
            owner: owner("owner"),
            now_iso: "2026-06-21T00:00:00.000Z".into(),
            agent: child,
        },
    )
    .expect("create");
    commit_multi_agent_terminal_mutation(
        &db,
        &CommitMultiAgentTerminalMutationInput {
            session_path: session_path.into(),
            agent_id: "child".into(),
            process_identity,
            owner: owner("owner"),
            terminal_lifecycle: "failed".into(),
            event_kind: "failed".into(),
            agent_details: pi_coding_agent::core::session_control_db::AgentDetails {
                error: Some(
                    js_json::to_value(&AgentError {
                        message: "boom".into(),
                        code: Some("E".into()),
                    })
                    .expect("error"),
                ),
                result: None,
            },
            updated_at: "2026-06-21T00:00:01.000Z".into(),
        },
    )
    .expect("terminal");
    let agent = read_multi_agent_agent(&db, session_path, "child")
        .expect("read")
        .expect("agent");
    assert_eq!(agent.lifecycle, AgentLifecycle::Failed);
    assert_eq!(agent.error.expect("error").message, "boom");
}

#[test]
fn read_multi_agent_state_orders_rows_by_rowid() {
    let (_dir, db) = temp_db();
    let session_path = "/sessions/main.jsonl";
    for id in ["first", "second", "third"] {
        bootstrap_multi_agent_agent(
            &db,
            session_path,
            &JsString::from(id),
            &js_json::to_value(&agent_node(id, Some("main"), AgentLifecycle::Running)).expect("value"),
        )
        .expect("bootstrap");
    }
    let state = read_multi_agent_state(&db, session_path)
        .expect("state")
        .expect("persisted");
    let ids: Vec<&JsString> = state
        .agents
        .iter()
        .map(|agent| agent.get("id").and_then(|value| value.as_string()).expect("id"))
        .collect();
    assert_eq!(ids, vec!["first", "second", "third"]);
}

#[test]
fn session_metadata_identity_is_enforced_for_sandbox_profiles() {
    let (_dir, db) = temp_db();
    write_session_metadata(&db, &metadata("/sessions/a.jsonl", "id", None)).expect("write");
    let error = write_session_sandbox_profile(
        &db,
        "/sessions/a.jsonl",
        "other-id",
        pi_coding_agent::core::permissions::presets::SandboxProfileName::ReadOnly,
    )
    .expect_err("mismatch");
    assert_eq!(
        error.message(),
        "Session identity mismatch for /sessions/a.jsonl: expected id, received other-id"
    );
    let error = write_session_sandbox_profile(
        &db,
        "/sessions/missing.jsonl",
        "id",
        pi_coding_agent::core::permissions::presets::SandboxProfileName::ReadOnly,
    )
    .expect_err("missing");
    assert_eq!(
        error.message(),
        "Session metadata is missing for /sessions/missing.jsonl"
    );
}

#[test]
fn health_record_round_trips_all_statuses() {
    let (_dir, db) = temp_db();
    let mut health = empty_session_health("session", "2026-06-21T00:00:00.000Z");
    health.pid = Some(42);
    health.agent_generation = 3;
    health.last_active_at = Some("2026-06-21T00:00:00.000Z".into());
    health.last_checked_at = Some("2026-06-21T00:00:01.000Z".into());
    health.check_status = SessionCheckStatus::Timeout;
    health.checked_generation = Some(2);
    health.check_latency_ms = Some(1234);
    write_session_health(&db, &health).expect("write");
    let read = read_session_health(&db, &JsString::from("session"))
        .expect("read")
        .expect("health");
    assert_eq!(read, health);
}

#[test]
fn control_db_path_joins_control_sqlite() {
    assert_eq!(
        pi_coding_agent::core::session_control_db::get_control_db_path(pi_js_compat::node_fs::decode_node_path(
            PathBuf::from("/state").as_os_str()
        ))
        .to_utf8()
        .expect("fixture control path is scalar"),
        "/state/control.sqlite"
    );
}

#[test]
fn incoming_claim_returns_none_when_queue_is_empty() {
    let (_dir, db) = temp_db();
    assert!(claim_latest_incoming_message(&db).expect("claim").is_none());
}

#[test]
fn supervisor_advisory_claim_requires_matching_response_kind() {
    let (_dir, db) = temp_db();
    let id = post_supervisor_request(
        &db,
        &PostSupervisorRequestInput {
            sender_session_id: "sender".into(),
            project_id: "project".into(),
            kind: SupervisorRequestKind::SupervisorAdvisory,
            payload: JsObject::new(),
            deadline_at: "2099-01-01T00:00:00.000Z".into(),
        },
    )
    .expect("post");
    let claimed = claim_next_supervisor_request(&db, &JsString::from("token"))
        .expect("claim")
        .expect("request");
    assert_eq!(claimed.id, id);
    complete_supervisor_request(
        &db,
        id,
        &JsString::from("token"),
        &pi_js_compat::js_json!({"answer":"answer","kind":"advisory"}),
    )
    .expect("complete");
}

#[test]
fn outbox_claim_recovers_stale_claims_and_poisons_after_max_attempts() {
    let (_dir, db) = temp_db();
    let process_identity = identity(std::process::id(), 1);
    let session_path = "/sessions/main.jsonl";
    let child = agent_node("child", Some("main"), AgentLifecycle::Running);
    create_multi_agent_child_with_runtime_ownership(
        &db,
        &CreateMultiAgentChildWithRuntimeOwnershipInput {
            session_path: session_path.into(),
            agent_id: "child".into(),
            process_identity: process_identity.clone(),
            owner: owner("owner"),
            now_iso: "2026-06-21T00:00:00.000Z".into(),
            agent: child,
        },
    )
    .expect("create");
    commit_multi_agent_terminal_mutation(
        &db,
        &CommitMultiAgentTerminalMutationInput {
            session_path: session_path.into(),
            agent_id: "child".into(),
            process_identity,
            owner: owner("owner"),
            terminal_lifecycle: "completed".into(),
            event_kind: "completed".into(),
            agent_details: Default::default(),
            updated_at: "2026-06-21T00:00:01.000Z".into(),
        },
    )
    .expect("terminal");

    let first = claim_multi_agent_terminal_outbox(
        &db,
        &JsString::from("claim-1"),
        "2026-06-21T00:00:02.000Z",
        &Default::default(),
    )
    .expect("claim")
    .expect("record");
    assert_eq!(first.attempt_count, 1);
    pi_coding_agent::core::session_control_db::fail_multi_agent_terminal_outbox(
        &db,
        &first,
        &JsString::from("transient"),
        "2026-06-21T00:00:03.000Z",
        2,
    )
    .expect("fail");

    let second = claim_multi_agent_terminal_outbox(
        &db,
        &JsString::from("claim-2"),
        "2026-06-21T00:00:04.000Z",
        &Default::default(),
    )
    .expect("claim")
    .expect("record");
    assert_eq!(second.attempt_count, 2);
    pi_coding_agent::core::session_control_db::fail_multi_agent_terminal_outbox(
        &db,
        &second,
        &JsString::from("permanent"),
        "2026-06-21T00:00:05.000Z",
        2,
    )
    .expect("fail");
    assert!(
        claim_multi_agent_terminal_outbox(
            &db,
            &JsString::from("claim-3"),
            "2026-06-21T00:00:06.000Z",
            &Default::default()
        )
        .expect("claim")
        .is_none()
    );
}

#[test]
fn runtime_mailbox_claim_respects_registered_listener_pid() {
    let (_dir, db) = temp_db();
    let recipient = RuntimeMailboxAddress {
        session_id: "recipient".into(),
        agent_id: None,
    };
    let sender = RuntimeMailboxAddress {
        session_id: "sender".into(),
        agent_id: None,
    };
    stored_message(&db, "/sessions/sender.jsonl", "message_1", &recipient, &sender, "hello");
    // A listener registered for a different process does not own the mailbox.
    register_runtime_mailbox_listener(&db, &recipient, 999_999, None, Default::default()).expect("register");
    let claimed = claim_runtime_mailbox_messages(&db, &recipient, 20).expect("claim");
    assert!(claimed.is_empty());
}

#[test]
fn mailbox_consumption_removes_even_a_legacy_delivered_row() {
    let (_dir, db) = temp_db();
    let session_path = "/sessions/main.jsonl";
    upsert_multi_agent_mailbox_message(
        &db,
        session_path,
        "message_1",
        &pi_js_compat::js_json!({
            "fromAgentId": "main",
            "id": "message_1",
            "kind": "message",
            "status": "delivered",
            "toAgentId": "main",
        }),
    )
    .expect("upsert");
    assert!(
        // Frozen session-control-db.ts deletes by store reference regardless of status.
        pi_coding_agent::core::session_control_db::mark_multi_agent_mailbox_message_delivered(
            &db,
            session_path,
            &JsString::from("message_1")
        )
        .expect("mark")
    );
    assert_eq!(
        get_multi_agent_mailbox_message_status(&db, session_path, &JsString::from("message_1")).expect("status"),
        None
    );
}

#[test]
fn terminal_outbox_replay_is_idempotent() {
    let (_dir, db) = temp_db();
    let process_identity = identity(std::process::id(), 1);
    let session_path = "/sessions/main.jsonl";
    let child = agent_node("child", Some("main"), AgentLifecycle::Running);
    create_multi_agent_child_with_runtime_ownership(
        &db,
        &CreateMultiAgentChildWithRuntimeOwnershipInput {
            session_path: session_path.into(),
            agent_id: "child".into(),
            process_identity: process_identity.clone(),
            owner: owner("owner"),
            now_iso: "2026-06-21T00:00:00.000Z".into(),
            agent: child,
        },
    )
    .expect("create");
    let input = CommitMultiAgentTerminalMutationInput {
        session_path: session_path.into(),
        agent_id: "child".into(),
        process_identity,
        owner: owner("owner"),
        terminal_lifecycle: "completed".into(),
        event_kind: "completed".into(),
        agent_details: Default::default(),
        updated_at: "2026-06-21T00:00:01.000Z".into(),
    };
    commit_multi_agent_terminal_mutation(&db, &input).expect("terminal");
    let replay = commit_multi_agent_terminal_mutation(&db, &input).expect("replay");
    assert_eq!(
        replay,
        pi_coding_agent::core::session_control_db::CommitMultiAgentTerminalMutationResult::Ok { terminal_revision: 2 }
    );
}

#[test]
fn session_metadata_write_preserves_archived_state() {
    let (_dir, db) = temp_db();
    write_session_metadata(&db, &metadata("/sessions/a.jsonl", "id", None)).expect("write");
    archive_session(&db, "/sessions/a.jsonl").expect("archive");
    // Frozen session-control-db.ts:2793-2857 has no text-index fields; preserves archivedAt.
    let update = metadata("/sessions/a.jsonl", "id", None);
    write_session_metadata(&db, &update).expect("update");
    let read = read_session_metadata(&db, "/sessions/a.jsonl")
        .expect("read")
        .expect("metadata");
    assert!(read.archived_at.is_some());
}

#[cfg(target_os = "linux")]
#[test]
fn name_migration_errors_preserve_owner_pid_encounter_order() {
    let (_dir, db) = temp_db();
    create_version_fourteen_database(&db);
    let stat = std::fs::read_to_string("/proc/self/stat").expect("process stat");
    let fields = stat
        .rsplit_once(')')
        .expect("stat command")
        .1
        .split_whitespace()
        .collect::<Vec<_>>();
    let parent: u32 = fields[1].parse().expect("parent pid");
    let identity = pi_coding_agent::core::runtime_process::read_process_identity(parent).expect("parent identity");
    let raw = rusqlite::Connection::open(&db).expect("database");
    raw.execute_batch(
        "CREATE TABLE multi_agent_runtime_owners (
        session_path TEXT NOT NULL, agent_id TEXT NOT NULL, process_identity TEXT,
        owner_session_id TEXT, owner_agent_id TEXT, PRIMARY KEY(session_path, agent_id));
        CREATE TABLE session_health (
        session_id TEXT PRIMARY KEY, agent_generation INTEGER NOT NULL, pid INTEGER,
        last_active_at TEXT, last_checked_at TEXT, check_status TEXT NOT NULL,
        checked_generation INTEGER, check_latency_ms INTEGER, updated_at TEXT NOT NULL);",
    )
    .expect("owners");
    raw.execute(
        "INSERT INTO session_health(session_id,agent_generation,pid,check_status,updated_at)
        VALUES ('current',1,?,'ok','2026-10-20T00:00:00.000Z')",
        [std::process::id()],
    )
    .expect("health");
    raw.execute(
        "INSERT INTO multi_agent_runtime_owners(session_path,agent_id,process_identity)
        VALUES ('/owner','parent',?)",
        [serde_json::to_string(&identity).expect("identity")],
    )
    .expect("owner");
    drop(raw);
    let failure = read_session_metadata(&db, "/sessions/legacy-name.jsonl").expect_err("live owners");
    assert_eq!(
        *failure.message(),
        format!(
            "Cannot activate session name schema version 15 while lifecycle owners are active (PIDs: {}, {parent}). Stop all Pi and detached runner processes, then retry",
            std::process::id()
        )
    );
}

#[test]
fn existing_v15_names_update_without_reentering_lifecycle_migrations() {
    let (_dir, db) = temp_db();
    write_session_metadata(&db, &metadata("/existing", "existing", None)).expect("initialize v15");
    let raw = rusqlite::Connection::open(&db).expect("database");
    raw.execute(
        "INSERT INTO session_health(session_id,agent_generation,pid,check_status,updated_at)
        VALUES ('live',1,?,'ok','2026-10-20T00:00:00.000Z')",
        [std::process::id()],
    )
    .expect("active owner");
    let payload = r#"{"id":"agent_1","lifecycle":"running","revision":1}"#;
    raw.execute(
        "INSERT INTO multi_agent_agents(session_path,agent_id,data,updated_at)
        VALUES ('/existing','agent_1',?,'2026-10-20T00:00:00.000Z')",
        [payload],
    )
    .expect("agent");
    write_session_name(&db, "/existing", Some(&JsString::from("Existing v15 Name"))).expect("no migration required");
    assert_eq!(
        read_session_name_state(&db, "/existing")
            .expect("name state")
            .name
            .clone(),
        Some("Existing v15 Name".into())
    );
    let retained: String = raw
        .query_row(
            "SELECT data FROM multi_agent_agents WHERE agent_id='agent_1'",
            [],
            |row| row.get(0),
        )
        .expect("payload");
    assert_eq!(retained, payload);
    let version: i64 = raw
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("version");
    assert_eq!(version, 15);
}
