//! Behavioral tests for the lifecycle coordinator (`core/lifecycle-coordinator.ts`).

use pi_js_compat::js_json::{self, JsString};
use std::sync::Arc;

use pi_coding_agent::core::lifecycle_coordinator::{
    DetachedCancellationCommandInput, FinalizeChildCommandInput, LifecycleCommandResult, LifecycleCoordinator,
    LifecycleCoordinatorOptions, OwnedLifecycleCommandInput, PrepareChildCommandInput, RecoverDeadChildCommandInput,
    RecoverDeadChildCommandResult, SteeringCommandResult,
};
use pi_coding_agent::core::multi_agent_store::{AgentError, AgentLifecycle, AgentPermission, AgentResult, SpawnAgent};
use pi_coding_agent::core::runtime_process::{ProcessIdentity, read_process_identity};
use pi_coding_agent::core::session_control_db::{
    CreateMultiAgentChildWithRuntimeOwnershipResult, MultiAgentRuntimeOwnerIdentity, MultiAgentRuntimeOwnership,
    list_runtime_mailbox_messages, read_multi_agent_agent, read_multi_agent_runtime_ownership, read_multi_agent_state,
    register_runtime_mailbox_listener,
};
use tempfile::TempDir;
#[path = "support/oracle.rs"]
mod oracle;

const NOW: &str = "2026-07-11T20:00:00.000Z";
const SESSION_PATH: &str = "/tmp/supervisor.jsonl";

fn current_identity() -> ProcessIdentity {
    read_process_identity(std::process::id()).expect("process identity")
}

fn test_process_identity(name: &str) -> ProcessIdentity {
    let mut value: u32 = 0;
    for character in name.chars() {
        value = value.wrapping_mul(31).wrapping_add(u32::from(character));
    }
    ProcessIdentity {
        pid: 1_000_000_000 + (value % 1_000_000_000),
        start_time_ticks: u64::from(value) + 1,
        incarnation: None,
    }
}

fn temp_db() -> (TempDir, String) {
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join("control.sqlite").display().to_string();
    (directory, path)
}

fn create_coordinator(control_db_path: &str, session_path: &str) -> LifecycleCoordinator {
    create_coordinator_with(control_db_path, session_path, || "agent-child".into())
}

fn create_coordinator_with(
    control_db_path: &str,
    session_path: &str,
    create_agent_id: impl Fn() -> JsString + Send + Sync + 'static,
) -> LifecycleCoordinator {
    let identity = current_identity();
    register_runtime_mailbox_listener(
        control_db_path,
        &pi_coding_agent::core::session_control_db::RuntimeMailboxAddress {
            session_id: "supervisor-session".into(),
            agent_id: None,
        },
        identity.pid,
        Some(&JsString::from(session_path)),
        pi_coding_agent::core::session_control_db::RuntimeMailboxRegistrationOptions {
            runtime_instance_id: Some(pi_coding_agent::core::session_control_db::serialize_process_identity(
                &identity,
            )),
            ..Default::default()
        },
    )
    .expect("register listener");
    LifecycleCoordinator::new(LifecycleCoordinatorOptions {
        control_db_path: control_db_path.into(),
        create_agent_id: Arc::new(move || Ok(create_agent_id())),
        // Frozen session-control-db.ts:2331 filters transport against Date.now().
        now: Arc::new(pi_coding_agent::utils::iso_time::now_iso_string),
        process_identity: identity,
        session_path: session_path.into(),
    })
}

fn child_input(parent_id: Option<&JsString>) -> SpawnAgent {
    SpawnAgent {
        parent_id: parent_id.map(JsString::from),
        display_name: "Explorer".into(),
        agent_type: "explore".into(),
        origin: None,
        cwd: "/tmp/worktree".into(),
        permission: AgentPermission {
            policy: "on-request".into(),
            inherited_from: None,
            narrowed: true,
        },
        worktree: None,
        model: None,
        account: None,
        slot: None,
        transcript: None,
        event_stream: None,
        worker: None,
    }
}

fn create_running_child(
    coordinator: &LifecycleCoordinator,
    input: SpawnAgent,
) -> CreateMultiAgentChildWithRuntimeOwnershipResult {
    let prepared = coordinator
        .prepare_child(&PrepareChildCommandInput {
            spawn: input,
            agent_id: None,
            detached: None,
            result: None,
        })
        .unwrap();
    coordinator
        .commit_running_child(
            &prepared,
            &JsString::from("supervisor-session"),
            &current_identity(),
            None,
        )
        .expect("commit child")
}

fn expect_created(
    result: CreateMultiAgentChildWithRuntimeOwnershipResult,
) -> (
    pi_coding_agent::core::multi_agent_store::AgentSnapshot,
    MultiAgentRuntimeOwnership,
) {
    match result {
        CreateMultiAgentChildWithRuntimeOwnershipResult::Ok { agent, ownership } => (*agent, *ownership),
        CreateMultiAgentChildWithRuntimeOwnershipResult::Err { error } => {
            panic!("expected child, got {}", error.as_str())
        }
    }
}

fn owned(
    agent: &pi_coding_agent::core::multi_agent_store::AgentSnapshot,
    ownership: &MultiAgentRuntimeOwnership,
) -> OwnedLifecycleCommandInput {
    OwnedLifecycleCommandInput {
        agent: agent.clone(),
        ownership: ownership.clone(),
    }
}

fn force_runtime_ownership(
    control_db_path: &str,
    session_path: &str,
    agent_id: &JsString,
    owner: &MultiAgentRuntimeOwnerIdentity,
    process_identity: &ProcessIdentity,
) -> bool {
    let current = read_multi_agent_runtime_ownership(control_db_path, session_path, agent_id).expect("ownership");
    if current
        .as_ref()
        .and_then(|ownership| ownership.process_identity.as_ref())
        .is_some_and(pi_coding_agent::core::runtime_process::is_process_identity_alive)
    {
        return false;
    }
    let db = pi_coding_agent::core::sqlite::create_sqlite_database(control_db_path).expect("open");
    db.execute(
        "INSERT INTO multi_agent_runtime_owners (
            session_path, agent_id, process_identity, owner_session_id, owner_agent_id
        ) VALUES (?, ?, ?, ?, ?)
        ON CONFLICT(session_path, agent_id) DO UPDATE SET
            process_identity = excluded.process_identity,
            owner_session_id = excluded.owner_session_id,
            owner_agent_id = excluded.owner_agent_id",
        rusqlite::params![
            session_path,
            agent_id,
            pi_coding_agent::core::session_control_db::serialize_process_identity(process_identity),
            owner.session_id,
            owner.agent_id
        ],
    )
    .expect("force ownership");
    true
}

#[test]
fn atomically_creates_a_running_child_with_runtime_ownership() {
    let (_directory, db) = temp_db();
    let coordinator = create_coordinator(&db, SESSION_PATH);
    let (agent, ownership) = expect_created(create_running_child(&coordinator, child_input(None)));
    assert_eq!(agent.id, "agent-child");
    assert_eq!(agent.lifecycle, AgentLifecycle::Running);
    assert_eq!(agent.parent_id.as_ref(), Some(&JsString::from("main")));
    assert_eq!(agent.revision, 1);
    assert_eq!(ownership.agent_id, "agent-child");
    assert_eq!(
        ownership.owner,
        pi_coding_agent::core::session_control_db::MultiAgentRuntimeOwner {
            session_id: Some("supervisor-session".into()),
            agent_id: None,
        }
    );
    let state = read_multi_agent_state(&db, SESSION_PATH)
        .expect("state")
        .expect("persisted");
    assert_eq!(state.agents.len(), 1);
    let read = read_multi_agent_runtime_ownership(&db, SESSION_PATH, &JsString::from("agent-child"))
        .expect("ownership")
        .expect("row");
    assert_eq!(read.agent_id, ownership.agent_id);
}

#[test]
fn acquires_attached_runtime_ownership_and_advances_revision() {
    let (_directory, db) = temp_db();
    let agent = pi_coding_agent::core::multi_agent_store::AgentSnapshot {
        id: "attached-1".into(),
        parent_id: None,
        display_name: "Attached".into(),
        agent_type: "resumed-session".into(),
        origin: Some(pi_coding_agent::core::multi_agent_store::AgentOrigin::Attached),
        lifecycle: AgentLifecycle::Running,
        revision: 4,
        created_at: "2026-07-11T19:00:00.000Z".into(),
        updated_at: "2026-07-11T19:00:00.000Z".into(),
        cwd: "/tmp/worktree".into(),
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
    };
    pi_coding_agent::core::session_control_db::bootstrap_multi_agent_agent(
        &db,
        SESSION_PATH,
        &JsString::from("attached-1"),
        &js_json::to_value(&agent).expect("agent"),
    )
    .expect("bootstrap");
    let coordinator = create_coordinator(&db, SESSION_PATH);
    let result = coordinator
        .acquire_attached_runtime(&agent, &JsString::from("supervisor-session"))
        .expect("acquire");
    let pi_coding_agent::core::session_control_db::AcquireAttachedRuntimeOwnershipResult::Ok { agent, ownership } =
        result
    else {
        panic!("expected acquire ok");
    };
    assert_eq!(agent.revision, 5);
    assert_eq!(agent.lifecycle, AgentLifecycle::Running);
    assert_eq!(ownership.process_identity, Some(current_identity()));

    let mismatch = coordinator
        .acquire_attached_runtime(&agent, &JsString::from("other-session"))
        .expect("acquire");
    assert!(matches!(
        mismatch,
        pi_coding_agent::core::session_control_db::AcquireAttachedRuntimeOwnershipResult::Err {
            error: pi_coding_agent::core::session_control_db::AcquireAttachedError::MutationMismatch
        }
    ));
}

#[test]
fn rejects_ownership_takeover_while_the_exact_owner_process_is_alive() {
    let (_directory, db) = temp_db();
    let coordinator = create_coordinator(&db, SESSION_PATH);
    let (agent, _ownership) = expect_created(create_running_child(&coordinator, child_input(None)));
    let forced = force_runtime_ownership(
        &db,
        SESSION_PATH,
        &agent.id,
        &MultiAgentRuntimeOwnerIdentity {
            session_id: "supervisor-2".into(),
            agent_id: None,
        },
        &current_identity(),
    );
    assert!(!forced, "expected ownership_held");
}

#[test]
fn rejects_another_agents_ownership_token_under_the_same_supervisor() {
    let (_directory, db) = temp_db();
    let next = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = next.clone();
    let coordinator = create_coordinator_with(&db, SESSION_PATH, move || {
        let value = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        pi_js_compat::js_format!("agent-{}", value)
    });
    let (first_agent, first_ownership) = expect_created(create_running_child(&coordinator, child_input(None)));
    let (second_agent, _second_ownership) = expect_created(create_running_child(&coordinator, child_input(None)));
    let result = coordinator
        .request_cancellation(&owned(&second_agent, &first_ownership))
        .expect("cancel");
    assert!(matches!(
        result,
        LifecycleCommandResult::Err {
            error: pi_coding_agent::core::session_control_db::LifecycleCommandError::MutationMismatch
        }
    ));
    let read = read_multi_agent_agent(&db, SESSION_PATH, &second_agent.id)
        .expect("read")
        .expect("agent");
    assert_eq!(read.lifecycle, AgentLifecycle::Running);
    assert_eq!(read.revision, 1);
    assert_eq!(first_agent.revision, 1);
}

#[test]
fn rejects_finalization_from_a_different_current_process() {
    let (_directory, db) = temp_db();
    let owner = create_coordinator(&db, SESSION_PATH);
    let (agent, ownership) = expect_created(create_running_child(&owner, child_input(None)));
    let foreign = LifecycleCoordinator::new(LifecycleCoordinatorOptions {
        control_db_path: db.clone().into(),
        create_agent_id: Arc::new(|| Ok("unused".into())),
        // Frozen session-control-db.ts:2331 filters transport against Date.now().
        now: Arc::new(pi_coding_agent::utils::iso_time::now_iso_string),
        process_identity: test_process_identity("foreign-runtime"),
        session_path: SESSION_PATH.into(),
    });
    let result = foreign
        .finalize_child(&FinalizeChildCommandInput {
            agent: agent.clone(),
            ownership: ownership.clone(),
            error: None,
            result: None,
            terminal_lifecycle: "completed".into(),
        })
        .expect("finalize");
    assert!(matches!(
        result,
        LifecycleCommandResult::Err {
            error: pi_coding_agent::core::session_control_db::LifecycleCommandError::MutationMismatch
        }
    ));
    let read = read_multi_agent_agent(&db, SESSION_PATH, &agent.id)
        .expect("read")
        .expect("agent");
    assert_eq!(read.lifecycle, AgentLifecycle::Running);
    assert_eq!(read.revision, 1);
}

#[test]
fn requires_cancelling_before_abort_acknowledgement() {
    let (_directory, db) = temp_db();
    let coordinator = create_coordinator(&db, SESSION_PATH);
    let (agent, ownership) = expect_created(create_running_child(&coordinator, child_input(None)));
    let cancelling = coordinator
        .request_cancellation(&owned(&agent, &ownership))
        .expect("cancel");
    let LifecycleCommandResult::Ok {
        agent: cancelling_agent,
    } = cancelling
    else {
        panic!("expected cancelling");
    };
    assert_eq!(cancelling_agent.lifecycle, AgentLifecycle::Cancelling);
    assert_eq!(cancelling_agent.revision, 2);

    let aborted = coordinator
        .acknowledge_cancellation(&owned(&cancelling_agent, &ownership))
        .expect("abort");
    let LifecycleCommandResult::Ok { agent: aborted_agent } = aborted else {
        panic!("expected aborted");
    };
    assert_eq!(aborted_agent.lifecycle, AgentLifecycle::Aborted);
    assert_eq!(aborted_agent.revision, 3);

    let mut wrong = ownership.clone();
    wrong.process_identity = Some(test_process_identity("wrong-owner"));
    let duplicate = coordinator
        .acknowledge_cancellation(&owned(&cancelling_agent, &wrong))
        .expect("abort");
    assert!(matches!(
        duplicate,
        LifecycleCommandResult::Err {
            error: pi_coding_agent::core::session_control_db::LifecycleCommandError::MutationMismatch
        }
    ));
}

#[test]
fn detached_cancellation_preserves_terminal_notification_suppression() {
    let (_source_directory, source_db) = temp_db();
    let source_coordinator = create_coordinator(&source_db, SESSION_PATH);
    let (source_agent, source_ownership) = expect_created(create_running_child(&source_coordinator, child_input(None)));
    let mut source = oracle::TsOracle::spawn("coding-agent-core-leaves-oracle.ts", "packages/coding-agent/src");
    let expected = source.request(&pi_js_compat::js_json!({
        "id": 28301, "module": "core/lifecycle-coordinator.ts", "operation": "lifecycleDetachedCancellation",
        "now": pi_coding_agent::utils::iso_time::now_iso_string(),
        "options": {"controlDbPath": source_db, "sessionPath": SESSION_PATH, "processIdentity": current_identity()},
        "input": {"agent":source_agent,"ownership":source_ownership,"outputLabel":"Bash output","suppressTerminalNotification":true},
    }));
    assert_eq!(expected["ok"], js_json::JsValue::Bool(true));
    assert_eq!(expected["output"]["ok"], js_json::JsValue::Bool(true));
    assert_eq!(
        expected["output"]["agent"]["suppressTerminalNotification"],
        js_json::JsValue::Bool(true)
    );
    let (_directory, db) = temp_db();
    let coordinator = create_coordinator(&db, SESSION_PATH);
    let (agent, ownership) = expect_created(create_running_child(&coordinator, child_input(None)));
    let result = coordinator
        .request_detached_cancellation(&DetachedCancellationCommandInput {
            agent: agent.clone(),
            ownership,
            output_label: "Bash output".into(),
            reason: None,
            suppress_terminal_notification: Some(true),
        })
        .unwrap();
    let LifecycleCommandResult::Ok { agent } = result else {
        panic!("cancellation failed")
    };
    assert_eq!(
        agent
            .extra
            .get("suppressTerminalNotification")
            .and_then(js_json::JsValue::as_bool),
        Some(true)
    );
    let persisted = read_multi_agent_agent(&db, SESSION_PATH, &agent.id).unwrap().unwrap();
    assert_eq!(
        persisted
            .extra
            .get("suppressTerminalNotification")
            .and_then(js_json::JsValue::as_bool),
        Some(true)
    );
}

#[test]
fn requests_detached_cancellation_and_enqueues_the_runner_command() {
    let (_directory, db) = temp_db();
    let coordinator = create_coordinator(&db, SESSION_PATH);
    let (agent, ownership) = expect_created(create_running_child(&coordinator, child_input(None)));
    let result = coordinator
        .request_detached_cancellation(&DetachedCancellationCommandInput {
            agent: agent.clone(),
            ownership: ownership.clone(),
            output_label: "Bash output".into(),
            reason: Some("user requested".into()),
            suppress_terminal_notification: None,
        })
        .expect("cancel");
    let LifecycleCommandResult::Ok { agent: cancelling } = result else {
        panic!("expected cancelling");
    };
    assert_eq!(cancelling.lifecycle, AgentLifecycle::Cancelling);
    assert_eq!(cancelling.revision, 2);

    let messages = list_runtime_mailbox_messages(&db).expect("messages");
    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0].kind,
        pi_coding_agent::core::session_control_db::RuntimeMailboxMessageKind::System
    );
    assert_eq!(messages[0].recipient.agent_id.as_ref(), Some(&agent.id));
    assert_eq!(messages[0].recipient.session_id, "supervisor-session");
    assert_eq!(
        messages[0].status,
        pi_coding_agent::core::session_control_db::RuntimeMailboxMessageStatus::Pending
    );
    let body = js_json::parse_lossless(&messages[0].body).expect("body");
    assert_eq!(body["command"], pi_js_compat::js_json!("cancel"));
    assert_eq!(body["identity"]["jobId"], pi_js_compat::js_json!(agent.id));
    assert_eq!(body["identity"]["outputLabel"], pi_js_compat::js_json!("Bash output"));
    assert_eq!(
        body["identity"]["owner"]["sessionId"],
        pi_js_compat::js_json!("supervisor-session")
    );
}

#[test]
fn rolls_back_detached_cancellation_when_mailbox_insertion_fails() {
    let (_directory, db) = temp_db();
    let coordinator = create_coordinator(&db, SESSION_PATH);
    let (agent, ownership) = expect_created(create_running_child(&coordinator, child_input(None)));
    let connection = pi_coding_agent::core::sqlite::create_sqlite_database(&db).expect("open");
    connection
        .execute_batch(
            "CREATE TRIGGER reject_detached_control BEFORE INSERT ON multi_agent_mailbox_messages
             WHEN NEW.message_id LIKE 'detached-cancel:%'
             BEGIN SELECT RAISE(ABORT, 'blocked detached control'); END",
        )
        .expect("trigger");
    drop(connection);

    let error = coordinator
        .request_detached_cancellation(&DetachedCancellationCommandInput {
            agent: agent.clone(),
            ownership: ownership.clone(),
            output_label: "Bash output".into(),
            reason: None,
            suppress_terminal_notification: None,
        })
        .expect_err("blocked");
    assert!(error.message().contains("blocked detached control"));
    let state = read_multi_agent_state(&db, SESSION_PATH)
        .expect("state")
        .expect("persisted");
    assert_eq!(
        state.agents[0]["lifecycle"].as_string(),
        Some(&JsString::from("running")),
        "agent must roll back to running"
    );
    assert!(list_runtime_mailbox_messages(&db).expect("messages").is_empty());
}

#[test]
fn blocks_parent_recovery_until_active_descendants_are_terminal() {
    let (_directory, db) = temp_db();
    let next = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = next.clone();
    let coordinator = create_coordinator_with(&db, SESSION_PATH, move || {
        let value = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        pi_js_compat::js_format!("agent-{}", value)
    });
    let dead = test_process_identity("dead-parent-runtime");
    let parent = coordinator
        .prepare_child(&PrepareChildCommandInput {
            spawn: child_input(None),
            agent_id: None,
            detached: None,
            result: None,
        })
        .unwrap();
    let parent = coordinator
        .commit_running_child(&parent, &JsString::from("supervisor-session"), &dead, None)
        .expect("parent");
    let CreateMultiAgentChildWithRuntimeOwnershipResult::Ok {
        agent: parent_agent,
        ownership: parent_ownership,
    } = parent
    else {
        panic!("parent");
    };
    let child = coordinator
        .prepare_child(&PrepareChildCommandInput {
            spawn: child_input(Some(&parent_agent.id)),
            agent_id: None,
            detached: None,
            result: None,
        })
        .unwrap();
    coordinator
        .commit_running_child(&child, &JsString::from("supervisor-session"), &dead, None)
        .expect("child");

    let result = coordinator
        .recover_dead_child(&RecoverDeadChildCommandInput {
            agent: (*parent_agent).clone(),
            ownership: (*parent_ownership).clone(),
            owner_session_id: "supervisor-session".into(),
            supervisor_agent_id: None,
        })
        .expect("recover");
    assert!(matches!(
        result,
        RecoverDeadChildCommandResult::Err {
            error: pi_coding_agent::core::session_control_db::RecoverDeadRuntimeError::InvalidTransition
        }
    ));
}

#[test]
fn recovers_a_dead_child_owned_by_a_prior_session_incarnation() {
    let (_directory, db) = temp_db();
    let coordinator = create_coordinator(&db, SESSION_PATH);
    let prepared = coordinator
        .prepare_child(&PrepareChildCommandInput {
            spawn: child_input(None),
            agent_id: None,
            detached: None,
            result: None,
        })
        .unwrap();
    let created = coordinator
        .commit_running_child(
            &prepared,
            &JsString::from("stale-incarnation-session"),
            &test_process_identity("stale-incarnation-runtime"),
            None,
        )
        .expect("create");
    let CreateMultiAgentChildWithRuntimeOwnershipResult::Ok { agent, ownership } = created else {
        panic!("child");
    };
    let result = coordinator
        .recover_dead_child(&RecoverDeadChildCommandInput {
            agent: (*agent).clone(),
            ownership: (*ownership).clone(),
            owner_session_id: "supervisor-session".into(),
            supervisor_agent_id: None,
        })
        .expect("recover");
    let RecoverDeadChildCommandResult::Ok { agent: recovered } = result else {
        panic!("expected recovery");
    };
    assert_eq!(recovered.lifecycle, AgentLifecycle::Failed);
    assert_eq!(recovered.revision, 2);
    assert_eq!(
        recovered.error.as_ref().map(|error| error.code.as_deref()),
        Some(Some("lost_runtime"))
    );
}

#[test]
fn settles_a_cancelling_child_to_aborted_on_dead_owner_recovery() {
    let (_directory, db) = temp_db();
    let coordinator = create_coordinator(&db, SESSION_PATH);
    let prepared = coordinator
        .prepare_child(&PrepareChildCommandInput {
            spawn: child_input(None),
            agent_id: None,
            detached: None,
            result: None,
        })
        .unwrap();
    let created = coordinator
        .commit_running_child(
            &prepared,
            &JsString::from("supervisor-session"),
            &test_process_identity("cancelling-dead-runtime"),
            None,
        )
        .expect("create");
    let CreateMultiAgentChildWithRuntimeOwnershipResult::Ok { agent, ownership } = created else {
        panic!("child");
    };
    let cancelling = coordinator
        .request_detached_cancellation(&DetachedCancellationCommandInput {
            agent: (*agent).clone(),
            ownership: (*ownership).clone(),
            output_label: "Pyrun output".into(),
            reason: Some("test cancel".into()),
            suppress_terminal_notification: None,
        })
        .expect("cancel");
    let LifecycleCommandResult::Ok { agent: cancelling } = cancelling else {
        panic!("cancelling");
    };
    let result = coordinator
        .recover_dead_child(&RecoverDeadChildCommandInput {
            agent: (*cancelling).clone(),
            ownership: (*ownership).clone(),
            owner_session_id: "supervisor-session".into(),
            supervisor_agent_id: None,
        })
        .expect("recover");
    let RecoverDeadChildCommandResult::Ok { agent: recovered } = result else {
        panic!("expected recovery");
    };
    assert_eq!(recovered.lifecycle, AgentLifecycle::Aborted);
}

#[test]
fn blocks_parent_terminalization_until_descendants_are_terminal() {
    let (_directory, db) = temp_db();
    let next = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = next.clone();
    let coordinator = create_coordinator_with(&db, SESSION_PATH, move || {
        let value = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        pi_js_compat::js_format!("agent-{}", value)
    });
    let (parent, parent_ownership) = expect_created(create_running_child(&coordinator, child_input(None)));
    let (child, child_ownership) = expect_created(create_running_child(&coordinator, child_input(Some(&parent.id))));

    let blocked = coordinator
        .finalize_child(&FinalizeChildCommandInput {
            agent: parent.clone(),
            ownership: parent_ownership.clone(),
            error: None,
            result: None,
            terminal_lifecycle: "completed".into(),
        })
        .expect("finalize");
    assert!(matches!(
        blocked,
        LifecycleCommandResult::Err {
            error: pi_coding_agent::core::session_control_db::LifecycleCommandError::InvalidTransition
        }
    ));
    let child_result = coordinator
        .finalize_child(&FinalizeChildCommandInput {
            agent: child,
            ownership: child_ownership,
            error: None,
            result: None,
            terminal_lifecycle: "completed".into(),
        })
        .expect("finalize child");
    assert!(matches!(child_result, LifecycleCommandResult::Ok { .. }));
    let parent_result = coordinator
        .finalize_child(&FinalizeChildCommandInput {
            agent: parent,
            ownership: parent_ownership,
            error: None,
            result: None,
            terminal_lifecycle: "completed".into(),
        })
        .expect("finalize parent");
    assert!(matches!(parent_result, LifecycleCommandResult::Ok { .. }));
}

#[test]
fn orders_natural_completion_before_a_later_cancellation_request() {
    let (_directory, db) = temp_db();
    let coordinator = create_coordinator(&db, SESSION_PATH);
    let (agent, ownership) = expect_created(create_running_child(&coordinator, child_input(None)));
    let completed = coordinator
        .finalize_child(&FinalizeChildCommandInput {
            agent: agent.clone(),
            ownership: ownership.clone(),
            error: None,
            result: Some(AgentResult {
                summary: Some("done".into()),
                file_refs: None,
                duration_ms: None,
                tool_call_id: None,
            }),
            terminal_lifecycle: "completed".into(),
        })
        .expect("finalize");
    let LifecycleCommandResult::Ok { agent: completed } = completed else {
        panic!("completed");
    };
    assert_eq!(completed.lifecycle, AgentLifecycle::Completed);
    assert_eq!(completed.revision, 2);
    let cancelled = coordinator
        .request_cancellation(&owned(&agent, &ownership))
        .expect("cancel");
    assert!(matches!(
        cancelled,
        LifecycleCommandResult::Err {
            error: pi_coding_agent::core::session_control_db::LifecycleCommandError::InvalidTransition
        }
    ));
}

#[test]
fn orders_accepted_cancellation_before_completion_and_deduplicates_acknowledgement() {
    let (_directory, db) = temp_db();
    let coordinator = create_coordinator(&db, SESSION_PATH);
    let (agent, ownership) = expect_created(create_running_child(&coordinator, child_input(None)));
    let cancelling = coordinator
        .request_cancellation(&owned(&agent, &ownership))
        .expect("cancel");
    let LifecycleCommandResult::Ok { agent: cancelling } = cancelling else {
        panic!("cancelling");
    };
    let completed = coordinator
        .finalize_child(&FinalizeChildCommandInput {
            agent: (*cancelling).clone(),
            ownership: ownership.clone(),
            error: None,
            result: None,
            terminal_lifecycle: "completed".into(),
        })
        .expect("finalize");
    assert!(matches!(
        completed,
        LifecycleCommandResult::Err {
            error: pi_coding_agent::core::session_control_db::LifecycleCommandError::InvalidTransition
        }
    ));
    let first = coordinator
        .acknowledge_cancellation(&owned(&cancelling, &ownership))
        .expect("abort");
    let LifecycleCommandResult::Ok { agent: first_agent } = &first else {
        panic!("abort");
    };
    assert_eq!(first_agent.lifecycle, AgentLifecycle::Aborted);
    assert_eq!(first_agent.revision, 3);
    let second = coordinator
        .acknowledge_cancellation(&owned(&cancelling, &ownership))
        .expect("abort");
    assert_eq!(first, second);
}

#[test]
fn does_not_persist_a_child_until_construction_succeeds() {
    let (_directory, db) = temp_db();
    let coordinator = create_coordinator(&db, SESSION_PATH);
    let prepared = coordinator
        .prepare_child(&PrepareChildCommandInput {
            spawn: child_input(None),
            agent_id: None,
            detached: None,
            result: None,
        })
        .unwrap();
    assert_eq!(prepared.id, "agent-child");
    assert_eq!(prepared.lifecycle, AgentLifecycle::Running);
    assert_eq!(prepared.revision, 1);
    let state = read_multi_agent_state(&db, SESSION_PATH).expect("state");
    assert!(state.is_none());
    let result = coordinator
        .commit_running_child(
            &prepared,
            &JsString::from("supervisor-session"),
            &current_identity(),
            None,
        )
        .expect("commit");
    assert!(matches!(
        result,
        CreateMultiAgentChildWithRuntimeOwnershipResult::Ok { .. }
    ));
}

#[test]
fn persists_interrupted_construction_as_failed_without_ownership() {
    let (_directory, db) = temp_db();
    let coordinator = create_coordinator(&db, SESSION_PATH);
    let prepared = coordinator
        .prepare_child(&PrepareChildCommandInput {
            spawn: child_input(None),
            agent_id: None,
            detached: None,
            result: None,
        })
        .unwrap();
    let failed = coordinator
        .commit_failed_child(
            &prepared,
            AgentError {
                code: Some("runtime_spawn_failed".into()),
                message: "factory interrupted".into(),
            },
        )
        .expect("failed");
    let pi_coding_agent::core::session_control_db::CreateFailedMultiAgentChildResult::Ok { agent } = failed else {
        panic!("failed");
    };
    assert_eq!(agent.lifecycle, AgentLifecycle::Failed);
    assert_eq!(agent.revision, 1);
    assert_eq!(
        agent.error.as_ref().and_then(|error| error.code.as_deref()),
        Some("runtime_spawn_failed")
    );
    assert!(
        read_multi_agent_runtime_ownership(&db, SESSION_PATH, &JsString::from("agent-child"))
            .expect("ownership")
            .is_none()
    );
}

#[test]
fn uses_a_preallocated_artifact_identity_without_consuming_another_id() {
    let (_directory, db) = temp_db();
    let generated = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let captured = generated.clone();
    let coordinator = create_coordinator_with(&db, SESSION_PATH, move || {
        captured.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        "generated".into()
    });
    let prepared = coordinator
        .prepare_child(&PrepareChildCommandInput {
            spawn: child_input(None),
            agent_id: Some("preallocated-job".into()),
            detached: None,
            result: None,
        })
        .unwrap();
    assert_eq!(prepared.id, "preallocated-job");
    coordinator
        .commit_running_child(
            &prepared,
            &JsString::from("supervisor-session"),
            &current_identity(),
            None,
        )
        .expect("commit");
    assert_eq!(generated.load(std::sync::atomic::Ordering::SeqCst), 0);
}

#[test]
fn rejects_child_and_attachment_creation_beneath_a_terminal_parent() {
    let (_directory, db) = temp_db();
    let next = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = next.clone();
    let coordinator = create_coordinator_with(&db, SESSION_PATH, move || {
        let value = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        pi_js_compat::js_format!("agent-{}", value)
    });
    let (parent, ownership) = expect_created(create_running_child(&coordinator, child_input(None)));
    coordinator
        .finalize_child(&FinalizeChildCommandInput {
            agent: parent.clone(),
            ownership,
            error: None,
            result: None,
            terminal_lifecycle: "completed".into(),
        })
        .expect("finalize");

    let result = create_running_child(&coordinator, child_input(Some(&parent.id)));
    assert!(matches!(
        result,
        CreateMultiAgentChildWithRuntimeOwnershipResult::Err {
            error: pi_coding_agent::core::session_control_db::CreateChildError::ParentNotFound
        }
    ));
    let attachment = coordinator
        .create_attachment(&child_input(Some(&parent.id)))
        .expect("attachment");
    assert!(matches!(
        attachment,
        pi_coding_agent::core::session_control_db::CreateMultiAgentAttachmentResult::Err {
            error: pi_coding_agent::core::session_control_db::CreateAttachmentError::ParentNotFound
        }
    ));
    let state = read_multi_agent_state(&db, SESSION_PATH)
        .expect("state")
        .expect("persisted");
    assert_eq!(state.agents.len(), 1);
}

#[test]
fn rejects_a_missing_parent_without_committing_child_or_ownership() {
    let (_directory, db) = temp_db();
    let coordinator = create_coordinator(&db, SESSION_PATH);
    let prepared = coordinator
        .prepare_child(&PrepareChildCommandInput {
            spawn: child_input(Some(&JsString::from("missing-parent"))),
            agent_id: None,
            detached: None,
            result: None,
        })
        .unwrap();
    let result = coordinator
        .commit_running_child(
            &prepared,
            &JsString::from("supervisor-session"),
            &current_identity(),
            None,
        )
        .expect("commit");
    assert!(matches!(
        result,
        CreateMultiAgentChildWithRuntimeOwnershipResult::Err {
            error: pi_coding_agent::core::session_control_db::CreateChildError::ParentNotFound
        }
    ));
    assert!(read_multi_agent_state(&db, SESSION_PATH).expect("state").is_none());
    assert!(
        read_multi_agent_runtime_ownership(&db, SESSION_PATH, &JsString::from("agent-child"))
            .expect("ownership")
            .is_none()
    );
}

#[test]
fn steering_commands_require_registered_sender_and_recipient_listeners() {
    let (_directory, db) = temp_db();
    let coordinator = create_coordinator(&db, SESSION_PATH);
    let mut input = child_input(None);
    input.transcript = Some(pi_coding_agent::core::multi_agent_store::AgentTranscriptMetadata {
        session_id: "child-session".into(),
        path: None,
    });
    let (agent, ownership) = expect_created(create_running_child(&coordinator, input));

    let result = coordinator
        .request_steering(&pi_coding_agent::core::lifecycle_coordinator::SteeringCommandInput {
            agent: agent.clone(),
            ownership,
            recipient: pi_coding_agent::core::session_control_db::RuntimeMailboxAddress {
                session_id: "child-session".into(),
                agent_id: Some(agent.id.clone()),
            },
            from_agent_id: "supervisor".into(),
            body: "steer".into(),
            target_checkpoint: None,
            thread_id: None,
            file_refs: None,
        })
        .expect("steer");
    assert!(matches!(
        result,
        SteeringCommandResult::Err {
            error: pi_coding_agent::core::session_control_db::SteeringCommandError::MutationMismatch
        }
    ));
}

#[test]
fn contention_retry_recovers_after_the_writer_releases_lock_like_frozen() {
    for frozen in [false, true] {
        let (_directory, db) = temp_db();
        let coordinator = create_coordinator(&db, SESSION_PATH);
        let identity = test_process_identity("automatic-retry");
        let mut dead = coordinator
            .prepare_child(&PrepareChildCommandInput {
                spawn: child_input(None),
                agent_id: Some("dead".into()),
                detached: Some(true),
                result: None,
            })
            .unwrap();
        dead.worker = Some(pi_coding_agent::core::multi_agent_store::AgentWorkerAdapter {
            adapter: pi_coding_agent::core::multi_agent_store::AgentWorkerAdapterKind::Runtime,
            handle_id: identity.pid.to_string().into(),
            cwd: None,
            tool_call_id: None,
        });
        expect_created(
            coordinator
                .commit_running_child(&dead, &"supervisor-session".into(), &identity, None)
                .unwrap(),
        );
        let writer = rusqlite::Connection::open(&db).unwrap();
        writer.execute_batch("BEGIN IMMEDIATE").unwrap();
        let mut source =
            frozen.then(|| oracle::TsOracle::spawn("coding-agent-core-leaves-oracle.ts", "packages/coding-agent/src"));
        let now = pi_coding_agent::utils::iso_time::now_iso_string();
        if let Some(source) = source.as_mut() {
            let response = source.request(&pi_js_compat::js_json!({
                "id":28304,"module":"core/lifecycle-coordinator.ts","operation":"lifecycleReconcile","db":db,"now":now,
            }));
            assert_eq!(response["ok"], js_json::JsValue::Bool(true));
            assert_eq!(response["output"], js_json::JsValue::Number(0.0));
        } else {
            assert_eq!(
                LifecycleCoordinator::reconcile_dead_detached_runtimes(&db.clone().into(), &now, None).unwrap(),
                0
            );
        }
        writer.execute_batch("ROLLBACK").unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let agent = read_multi_agent_agent(&db, SESSION_PATH, &dead.id).unwrap().unwrap();
            if agent.lifecycle == AgentLifecycle::Failed {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "retry did not terminalize dead runtime, frozen={frozen}"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(
            read_multi_agent_runtime_ownership(&db, SESSION_PATH, &dead.id)
                .unwrap()
                .unwrap()
                .process_identity
                .is_none()
        );
    }
}

#[test]
fn successful_reconciliation_cancels_the_pending_retry_like_frozen() {
    for frozen in [false, true] {
        let (_directory, db) = temp_db();
        let coordinator = create_coordinator(&db, SESSION_PATH);
        let identity = test_process_identity("cancel-retry");
        let mut dead = coordinator
            .prepare_child(&PrepareChildCommandInput {
                spawn: child_input(None),
                agent_id: Some("dead".into()),
                detached: Some(true),
                result: None,
            })
            .unwrap();
        dead.worker = Some(pi_coding_agent::core::multi_agent_store::AgentWorkerAdapter {
            adapter: pi_coding_agent::core::multi_agent_store::AgentWorkerAdapterKind::Runtime,
            handle_id: identity.pid.to_string().into(),
            cwd: None,
            tool_call_id: None,
        });
        expect_created(
            coordinator
                .commit_running_child(&dead, &"supervisor-session".into(), &identity, None)
                .unwrap(),
        );
        let writer = rusqlite::Connection::open(&db).unwrap();
        writer.execute_batch("BEGIN IMMEDIATE").unwrap();
        let mut oracle =
            frozen.then(|| oracle::TsOracle::spawn("coding-agent-core-leaves-oracle.ts", "packages/coding-agent/src"));
        let mut reconcile = || {
            let now = pi_coding_agent::utils::iso_time::now_iso_string();
            match oracle.as_mut() {
                Some(source) => {
                    let response = source.request(&pi_js_compat::js_json!({
                        "id": 28303,"module":"core/lifecycle-coordinator.ts","operation":"lifecycleReconcile",
                        "db":db,"now":now,
                    }));
                    assert_eq!(response["ok"], js_json::JsValue::Bool(true));
                    response["output"].as_f64().unwrap() as i64
                }
                None => {
                    LifecycleCoordinator::reconcile_dead_detached_runtimes(&JsString::from(&db), &now, None).unwrap()
                }
            }
        };
        assert_eq!(reconcile(), 0, "contention schedules a retry");
        writer.execute_batch("ROLLBACK").unwrap();
        assert_eq!(reconcile(), 1, "manual recovery clears the pending retry");
        drop(reconcile);
        drop(writer);
        std::fs::remove_file(&db).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(500));
        assert!(
            !std::path::Path::new(&db).exists(),
            "cancelled retry must not reopen a removed DB, frozen={frozen}"
        );
    }
}

#[test]
fn dead_runtime_reconciliation_cleans_expired_artifacts() {
    for frozen in [false, true] {
        for explicit_root in [false, true] {
            let (directory, db) = temp_db();
            let root = if explicit_root {
                directory.path().to_str().unwrap().into()
            } else {
                pi_coding_agent::config::get_agent_dir().unwrap()
            };
            let session_path = directory.path().join("reconcile.jsonl").to_str().unwrap().to_owned();
            let coordinator = create_coordinator(&db, &session_path);
            let mut dead = coordinator
                .prepare_child(&PrepareChildCommandInput {
                    spawn: child_input(None),
                    agent_id: Some("dead".into()),
                    detached: Some(true),
                    result: None,
                })
                .unwrap();
            let dead_identity = test_process_identity("dead-reconcile");
            dead.worker = Some(pi_coding_agent::core::multi_agent_store::AgentWorkerAdapter {
                adapter: pi_coding_agent::core::multi_agent_store::AgentWorkerAdapterKind::Runtime,
                handle_id: dead_identity.pid.to_string().into(),
                cwd: None,
                tool_call_id: None,
            });
            let committed = coordinator
                .commit_running_child(&dead, &"supervisor-session".into(), &dead_identity, None)
                .unwrap();
            assert!(matches!(
                committed,
                CreateMultiAgentChildWithRuntimeOwnershipResult::Ok { .. }
            ));

            let artifact_id = directory.path().file_name().unwrap().to_str().unwrap();
            let artifact = std::path::PathBuf::from(pi_js_compat::js_json::encode_node_utf8(&root))
                .join("detached-jobs/reconcile")
                .join(artifact_id);
            std::fs::create_dir_all(&artifact).unwrap();
            std::fs::write(artifact.join("output.log"), "expired").unwrap();
            dead.id = artifact_id.into();
            dead.lifecycle = AgentLifecycle::Completed;
            dead.created_at = "2026-01-01T00:00:00.000Z".into();
            dead.updated_at = dead.created_at.clone();
            dead.result = Some(AgentResult {
                summary: None,
                duration_ms: None,
                tool_call_id: None,
                file_refs: Some(vec![pi_coding_agent::core::multi_agent_store::AgentFileReference {
                    path: artifact.join("output.log").to_str().unwrap().into(),
                    label: Some("Bash output".into()),
                }]),
            });
            pi_coding_agent::core::session_control_db::bootstrap_multi_agent_agent(
                &db,
                &session_path,
                &dead.id,
                &js_json::to_value(&dead).unwrap(),
            )
            .unwrap();
            let now = pi_coding_agent::utils::iso_time::now_iso_string();
            let count = if frozen {
                let mut source =
                    oracle::TsOracle::spawn("coding-agent-core-leaves-oracle.ts", "packages/coding-agent/src");
                let response = source.request(&pi_js_compat::js_json!({
                    "id": 28302, "module": "core/lifecycle-coordinator.ts", "operation": "lifecycleReconcile",
                    "db": db, "now": now, "artifactRoot": if explicit_root { Some(root.clone()) } else { None },
                }));
                assert_eq!(response["ok"], js_json::JsValue::Bool(true));
                response["output"].as_f64().unwrap() as i64
            } else {
                LifecycleCoordinator::reconcile_dead_detached_runtimes(
                    &db.clone().into(),
                    &now,
                    explicit_root.then_some(&root),
                )
                .unwrap()
            };
            assert_eq!(count, 1, "frozen={frozen} explicit_root={explicit_root}");
            assert!(
                !artifact.exists(),
                "successful dead runtime reconciliation must trigger artifact cleanup"
            );
        }
    }
}

#[test]
fn dead_runtime_reconciliation_is_a_noop_without_candidates() {
    let (_directory, db) = temp_db();
    let count =
        LifecycleCoordinator::reconcile_dead_detached_runtimes(&JsString::from(&db), NOW, None).expect("reconcile");
    assert_eq!(count, 0);
}

#[test]
fn ownership_identity_requires_the_current_process_and_session_path() {
    let (_directory, db) = temp_db();
    let coordinator = create_coordinator(&db, SESSION_PATH);
    let (agent, ownership) = expect_created(create_running_child(&coordinator, child_input(None)));
    let mut foreign = ownership.clone();
    foreign.process_identity = Some(test_process_identity("other"));
    let result = coordinator
        .mark_waiting_for_input(&owned(&agent, &foreign))
        .expect("waiting");
    assert!(matches!(
        result,
        LifecycleCommandResult::Err {
            error: pi_coding_agent::core::session_control_db::LifecycleCommandError::MutationMismatch
        }
    ));
    let mut wrong_path = ownership.clone();
    wrong_path.session_path = "/tmp/other.jsonl".into();
    let result = coordinator
        .mark_waiting_for_input(&owned(&agent, &wrong_path))
        .expect("waiting");
    assert!(matches!(
        result,
        LifecycleCommandResult::Err {
            error: pi_coding_agent::core::session_control_db::LifecycleCommandError::MutationMismatch
        }
    ));
}

#[test]
fn detach_mark_allows_a_foreign_owner_process() {
    let (_directory, db) = temp_db();
    let coordinator = create_coordinator(&db, SESSION_PATH);
    let prepared = coordinator
        .prepare_child(&PrepareChildCommandInput {
            spawn: child_input(None),
            agent_id: None,
            detached: None,
            result: None,
        })
        .unwrap();
    let created = coordinator
        .commit_running_child(
            &prepared,
            &JsString::from("supervisor-session"),
            &test_process_identity("runner"),
            None,
        )
        .expect("create");
    let (agent, ownership) = expect_created(created);
    let result = coordinator.mark_detached(&owned(&agent, &ownership)).expect("detach");
    let LifecycleCommandResult::Ok { agent: detached } = result else {
        panic!("detach");
    };
    assert_eq!(detached.detached, Some(true));
    assert_eq!(detached.revision, 2);
}
