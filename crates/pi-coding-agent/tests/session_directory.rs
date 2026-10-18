//! Behavioral port of `test/session-directory.test.ts`.

use std::sync::Arc;

use pi_coding_agent::core::session_control_db::{
    RuntimeMailboxAddress, RuntimeMailboxMessageKind, RuntimeMailboxRegistrationOptions, WritableSessionMetadata,
    get_control_db_path, list_runtime_mailbox_listeners, list_runtime_mailbox_messages, read_session_health,
    register_runtime_mailbox_listener, write_session_goal, write_session_health, write_session_metadata,
};
use pi_coding_agent::core::session_directory::{
    BroadcastSessionFilters, BroadcastSessionsInput, SessionDirectoryOptions, broadcast_to_sessions, list_sessions,
};
use pi_coding_agent::core::session_health::{
    SessionBroadcastOutcome, SessionCheckStatus, SessionLiveStatus, empty_session_health,
};
use serde_json::json;
use tempfile::TempDir;

struct Fixture {
    _temp: TempDir,
    control_db_path: String,
}

impl Fixture {
    fn new() -> Self {
        let temp = TempDir::new().unwrap();
        let control_db_path = get_control_db_path(temp.path());
        Self {
            _temp: temp,
            control_db_path,
        }
    }

    fn write_session(&self, id: &str, name: Option<&str>, cwd: &str, goal: Option<&str>) {
        self.write_session_at(
            id,
            name,
            cwd,
            goal,
            &format!("/sessions/{id}.jsonl"),
            "2026-01-01T00:10:00.000Z",
        );
    }

    fn write_session_at(
        &self,
        id: &str,
        name: Option<&str>,
        cwd: &str,
        goal: Option<&str>,
        session_path: &str,
        modified_at: &str,
    ) {
        write_session_metadata(
            &self.control_db_path,
            &WritableSessionMetadata {
                session_path: session_path.to_owned(),
                id: id.to_owned(),
                cwd: cwd.to_owned(),
                name: name.map(str::to_owned),
                parent_session_path: None,
                archived_at: None,
                is_archived: None,
                is_subagent: None,
                subagent_name: None,
                model_provider: None,
                model_id: None,
                thinking_level: None,
                created_at: "2026-01-01T00:00:00.000Z".to_owned(),
                modified_at: modified_at.to_owned(),
                message_count: 1,
                first_message: "hello".to_owned(),
                all_messages_text: "hello".to_owned(),
                index_message_text: None,
            },
        )
        .unwrap();
        if let Some(goal) = goal {
            write_session_goal(
                &self.control_db_path,
                session_path,
                Some(&json!({ "objective": goal }).to_string()),
            )
            .unwrap();
        }
    }

    fn register(&self, session_id: &str, pid: u32, session_path: Option<&str>) {
        register_runtime_mailbox_listener(
            &self.control_db_path,
            &RuntimeMailboxAddress {
                agent_id: None,
                session_id: session_id.to_owned(),
            },
            pid,
            session_path,
            RuntimeMailboxRegistrationOptions::default(),
        )
        .unwrap();
    }
}

fn fixed_now(iso: &str) -> SessionDirectoryOptions {
    let millis = pi_js_compat::date::date_parse_millis(iso).unwrap();
    SessionDirectoryOptions {
        now: Some(Arc::new(move || millis)),
        ..Default::default()
    }
}

fn alive(value: bool) -> Arc<dyn Fn(u32) -> bool + Send + Sync> {
    Arc::new(move |_| value)
}

#[test]
fn lists_sessions_with_purpose_metadata_and_live_pid_health() {
    let fixture = Fixture::new();
    fixture.write_session("session-a", Some("Alpha"), "/repo/a", Some("ship feature"));
    fixture.register("session-a", std::process::id(), None);

    let sessions = list_sessions(&fixture.control_db_path, &SessionDirectoryOptions::default()).unwrap();

    assert_eq!(sessions.len(), 1);
    let entry = &sessions[0];
    assert_eq!(entry.session_id, "session-a");
    assert_eq!(entry.name.as_deref(), Some("Alpha"));
    assert_eq!(entry.goal.as_deref(), Some("ship feature"));
    assert_eq!(entry.cwd.as_deref(), Some("/repo/a"));
    assert_eq!(entry.pid, Some(std::process::id()));
    assert_eq!(entry.check_status, SessionCheckStatus::Ok);
    assert!(entry.eligible_to_receive);
    assert_eq!(entry.agent_generation, 1);
}

#[test]
fn does_not_claim_listener_ownership_while_listing_another_live_process() {
    let fixture = Fixture::new();
    fixture.write_session("session-owned", None, "/repo/owned", None);
    fixture.register("session-owned", 111, Some("/sessions/session-owned.jsonl"));

    let options = SessionDirectoryOptions {
        is_runtime_process_alive: Some(alive(true)),
        ..Default::default()
    };
    list_sessions(&fixture.control_db_path, &options).unwrap();

    let listeners = list_runtime_mailbox_listeners(&fixture.control_db_path).unwrap();
    assert_eq!(listeners.len(), 1);
    assert_eq!(listeners[0].session_id, "session-owned");
    assert_eq!(listeners[0].pid, 111);
    assert_eq!(
        listeners[0].session_path.as_deref(),
        Some("/sessions/session-owned.jsonl")
    );
    let health = read_session_health(&fixture.control_db_path, "session-owned")
        .unwrap()
        .unwrap();
    assert_eq!(health.pid, Some(111));
    assert_eq!(health.check_status, SessionCheckStatus::Ok);
}

#[test]
fn expires_a_fresh_binding_when_its_pid_is_not_a_pi_runtime() {
    let fixture = Fixture::new();
    fixture.write_session("session-fresh-reused-pid", None, "/repo/fresh", None);
    fixture.register("session-fresh-reused-pid", std::process::id(), None);

    let options = SessionDirectoryOptions {
        is_runtime_process_alive: Some(alive(false)),
        ..Default::default()
    };
    let sessions = list_sessions(&fixture.control_db_path, &options).unwrap();

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].pid, None);
    assert_eq!(sessions[0].status, SessionLiveStatus::Ended);
    assert_eq!(sessions[0].check_status, SessionCheckStatus::Dead);
    assert!(
        list_runtime_mailbox_listeners(&fixture.control_db_path)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn keeps_sticky_dead_sessions_skipped_until_generation_advances() {
    let fixture = Fixture::new();
    fixture.write_session("session-dead", None, "/repo/dead", None);
    let health = empty_session_health("session-dead", "2026-01-01T00:00:00.000Z");
    write_session_health(
        &fixture.control_db_path,
        &pi_coding_agent::core::session_health::SessionHealthRecord {
            agent_generation: 1,
            pid: Some(999_999),
            check_status: SessionCheckStatus::Dead,
            checked_generation: Some(1),
            last_checked_at: Some("2026-01-01T00:01:00.000Z".to_owned()),
            ..health
        },
    )
    .unwrap();

    let first = list_sessions(&fixture.control_db_path, &SessionDirectoryOptions::default()).unwrap();
    assert_eq!(first[0].check_status, SessionCheckStatus::Dead);
    assert!(!first[0].eligible_to_receive);

    fixture.register("session-dead", std::process::id(), None);
    let after_restart = list_sessions(&fixture.control_db_path, &SessionDirectoryOptions::default()).unwrap();
    assert_eq!(after_restart[0].check_status, SessionCheckStatus::Ok);
    assert!(after_restart[0].eligible_to_receive);
    assert_eq!(after_restart[0].agent_generation, 2);
}

#[test]
fn does_not_revive_an_unbound_historical_session_from_a_reused_live_pid() {
    let fixture = Fixture::new();
    fixture.write_session("session-historical", None, "/repo/historical", None);
    let health = empty_session_health("session-historical", "2026-01-01T00:00:00.000Z");
    write_session_health(
        &fixture.control_db_path,
        &pi_coding_agent::core::session_health::SessionHealthRecord {
            agent_generation: 1,
            pid: Some(std::process::id()),
            last_active_at: Some("2026-01-01T00:00:00.000Z".to_owned()),
            last_checked_at: Some("2026-01-01T00:00:00.000Z".to_owned()),
            check_status: SessionCheckStatus::Ok,
            checked_generation: Some(1),
            ..health
        },
    )
    .unwrap();

    let sessions = list_sessions(&fixture.control_db_path, &fixed_now("2026-01-01T00:11:00.000Z")).unwrap();

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].pid, None);
    assert_eq!(sessions[0].status, SessionLiveStatus::Ended);
    assert_eq!(sessions[0].check_status, SessionCheckStatus::Dead);
    assert!(!sessions[0].eligible_to_receive);
}

#[test]
fn uses_session_path_as_the_deterministic_tie_break_for_duplicate_metadata() {
    let fixture = Fixture::new();
    fixture.write_session_at(
        "session-live",
        None,
        "/repo/historical",
        None,
        "/sessions/a-historical.jsonl",
        "2026-01-01T00:11:00.000Z",
    );
    fixture.write_session_at(
        "session-live",
        None,
        "/repo/current",
        None,
        "/sessions/z-current.jsonl",
        "2026-01-01T00:11:00.000Z",
    );
    fixture.register("session-live", std::process::id(), None);

    let options = SessionDirectoryOptions {
        include_ended: Some(false),
        ..Default::default()
    };
    let sessions = list_sessions(&fixture.control_db_path, &options).unwrap();

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].session_id, "session-live");
    assert_eq!(sessions[0].session_path, "/sessions/z-current.jsonl");
    assert_eq!(sessions[0].cwd.as_deref(), Some("/repo/current"));
}

#[test]
fn broadcasts_only_to_eligible_matching_current_sessions() {
    let fixture = Fixture::new();
    fixture.write_session("session-live", Some("Live"), "/repo/live", None);
    fixture.write_session("session-filtered", Some("Other"), "/repo/other", None);
    fixture.register("session-live", std::process::id(), None);
    fixture.register("session-filtered", std::process::id() + 1, None);

    let input = BroadcastSessionsInput {
        message: "please restart".to_owned(),
        filters: Some(BroadcastSessionFilters {
            session_ids: Some(vec!["session-live".to_owned()]),
            ..Default::default()
        }),
        sender_session_id: "sender-session".to_owned(),
        sender_session_path: "/sessions/sender.jsonl".to_owned(),
        sender_agent_id: None,
    };
    let options = SessionDirectoryOptions {
        is_runtime_process_alive: Some(alive(true)),
        ..Default::default()
    };
    let results = broadcast_to_sessions(&fixture.control_db_path, &input, &options).unwrap();

    assert_eq!(results.len(), 2);
    let live = results
        .iter()
        .find(|result| result.session_id == "session-live")
        .unwrap();
    assert_eq!(live.outcome, SessionBroadcastOutcome::Sent);
    let filtered = results
        .iter()
        .find(|result| result.session_id == "session-filtered")
        .unwrap();
    assert_eq!(filtered.outcome, SessionBroadcastOutcome::SkippedFilter);
    let messages = list_runtime_mailbox_messages(&fixture.control_db_path).unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].recipient.session_id, "session-live");
    assert_eq!(messages[0].recipient.agent_id, None);
    assert_eq!(messages[0].kind, RuntimeMailboxMessageKind::Message);
    let health = read_session_health(&fixture.control_db_path, "session-live")
        .unwrap()
        .unwrap();
    assert_eq!(health.check_status, SessionCheckStatus::Ok);
}

#[test]
fn broadcast_requires_a_non_empty_message() {
    let fixture = Fixture::new();
    let input = BroadcastSessionsInput {
        message: "   ".to_owned(),
        filters: None,
        sender_session_id: "sender-session".to_owned(),
        sender_session_path: "/sessions/sender.jsonl".to_owned(),
        sender_agent_id: None,
    };
    let error =
        broadcast_to_sessions(&fixture.control_db_path, &input, &fixed_now("2026-01-01T00:11:00.000Z")).unwrap_err();
    assert_eq!(error.to_string(), "broadcast requires a non-empty message");
}

#[test]
fn uses_the_newest_main_session_listener_for_a_shared_live_pid() {
    let fixture = Fixture::new();
    fixture.write_session_at(
        "session-historical",
        None,
        "/repo/historical",
        None,
        "/sessions/session-historical.jsonl",
        "2026-01-01T00:00:00.000Z",
    );
    fixture.write_session_at(
        "session-current",
        None,
        "/repo/current",
        None,
        "/sessions/session-current.jsonl",
        "2026-01-01T00:00:00.000Z",
    );
    fixture.register("session-historical", std::process::id(), None);
    std::thread::sleep(std::time::Duration::from_millis(5));
    fixture.register("session-current", std::process::id(), None);

    let input = BroadcastSessionsInput {
        message: "please restart".to_owned(),
        filters: None,
        sender_session_id: "sender-session".to_owned(),
        sender_session_path: "/sessions/sender.jsonl".to_owned(),
        sender_agent_id: None,
    };
    let results = broadcast_to_sessions(&fixture.control_db_path, &input, &SessionDirectoryOptions::default()).unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].session_id, "session-current");
    assert_eq!(results[0].outcome, SessionBroadcastOutcome::Sent);
    let messages = list_runtime_mailbox_messages(&fixture.control_db_path).unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].recipient.session_id, "session-current");
}
