//! Port of `packages/coding-agent/test/session-directory.test.ts`.

use std::sync::Arc;

use pi_coding_agent::core::session_control_db::WritableSessionMetadata;
use pi_coding_agent::core::session_control_db::{
    RuntimeMailboxAddress, get_control_db_path, list_runtime_mailbox_listeners, list_runtime_mailbox_messages,
    read_session_health, register_runtime_mailbox_listener, write_session_goal, write_session_health,
    write_session_metadata,
};
use pi_coding_agent::core::session_directory::{
    BroadcastSessionFilters, BroadcastSessionsInput, SessionDirectoryOptions, broadcast_to_sessions, list_sessions,
};
use pi_coding_agent::core::session_health::{SessionCheckStatus, SessionLiveStatus, empty_session_health};
use tempfile::TempDir;

struct Fixture {
    _temp_dir: TempDir,
    control_db_path: String,
}

impl Fixture {
    fn new() -> Self {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let control_db_path = get_control_db_path(temp_dir.path());
        Self {
            _temp_dir: temp_dir,
            control_db_path,
        }
    }

    fn write_session(&self, id: &str, overrides: SessionOverrides<'_>) {
        let session_path = overrides
            .session_path
            .map(str::to_owned)
            .unwrap_or_else(|| format!("/sessions/{id}.jsonl"));
        write_session_metadata(
            &self.control_db_path,
            &WritableSessionMetadata {
                session_path: session_path.clone(),
                id: id.to_owned(),
                cwd: overrides.cwd.unwrap_or("/repo/default").to_owned(),
                name: overrides.name.map(str::to_owned),
                parent_session_path: None,
                archived_at: None,
                is_archived: None,
                is_subagent: None,
                subagent_name: None,
                model_provider: None,
                model_id: None,
                thinking_level: None,
                created_at: "2026-01-01T00:00:00.000Z".to_owned(),
                modified_at: overrides.modified_at.unwrap_or("2026-01-01T00:10:00.000Z").to_owned(),
                message_count: 1,
                first_message: "hello".to_owned(),
                all_messages_text: "hello".to_owned(),
                index_message_text: None,
            },
        )
        .expect("write metadata");
        if let Some(goal) = overrides.goal {
            write_session_goal(
                &self.control_db_path,
                &session_path,
                Some(&format!("{{\"objective\":\"{goal}\"}}")),
            )
            .expect("write goal");
        }
    }

    fn register_listener(&self, session_id: &str, pid: u32, session_path: Option<&str>) {
        register_runtime_mailbox_listener(
            &self.control_db_path,
            &RuntimeMailboxAddress {
                agent_id: None,
                session_id: session_id.to_owned(),
            },
            pid,
            session_path,
            Default::default(),
        )
        .expect("register listener");
    }
}

#[derive(Default)]
struct SessionOverrides<'a> {
    name: Option<&'a str>,
    cwd: Option<&'a str>,
    goal: Option<&'a str>,
    session_path: Option<&'a str>,
    modified_at: Option<&'a str>,
}

fn fixed_now() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is after the Unix epoch");
    i64::try_from(elapsed.as_millis()).expect("timestamp fits in i64")
}

fn eleven_minutes_from_now() -> i64 {
    fixed_now() + 11 * 60 * 1000
}

fn always_alive() -> Arc<dyn Fn(u32) -> bool + Send + Sync> {
    Arc::new(|_| true)
}

fn never_alive() -> Arc<dyn Fn(u32) -> bool + Send + Sync> {
    Arc::new(|_| false)
}

fn options(now_ms: i64) -> SessionDirectoryOptions {
    SessionDirectoryOptions {
        now_ms: Some(now_ms),
        include_ended: None,
        is_runtime_process_alive: None,
    }
}

#[test]
fn lists_sessions_with_purpose_metadata_and_live_pid_health() {
    let fixture = Fixture::new();
    fixture.write_session(
        "session-a",
        SessionOverrides {
            name: Some("Alpha"),
            goal: Some("ship feature"),
            cwd: Some("/repo/a"),
            ..Default::default()
        },
    );
    fixture.register_listener("session-a", std::process::id(), None);

    let sessions = list_sessions(&fixture.control_db_path, &options(fixed_now())).expect("list sessions");

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
    fixture.write_session("session-owned", SessionOverrides::default());
    fixture.register_listener("session-owned", 111, Some("/sessions/session-owned.jsonl"));

    let mut list_options = options(fixed_now());
    list_options.is_runtime_process_alive = Some(always_alive());
    list_sessions(&fixture.control_db_path, &list_options).expect("list sessions");

    let listeners = list_runtime_mailbox_listeners(&fixture.control_db_path).expect("listeners");
    assert_eq!(listeners.len(), 1);
    assert_eq!(listeners[0].session_id, "session-owned");
    assert_eq!(listeners[0].pid, 111);
    assert_eq!(
        listeners[0].session_path.as_deref(),
        Some("/sessions/session-owned.jsonl")
    );
    let health = read_session_health(&fixture.control_db_path, "session-owned")
        .expect("health")
        .expect("row");
    assert_eq!(health.pid, Some(111));
    assert_eq!(health.check_status, SessionCheckStatus::Ok);
}

#[test]
fn keeps_sticky_dead_sessions_skipped_until_generation_advances() {
    let fixture = Fixture::new();
    fixture.write_session("session-dead", SessionOverrides::default());
    let mut health = empty_session_health("session-dead", "2026-01-01T00:00:00.000Z");
    health.agent_generation = 1;
    health.pid = Some(999_999);
    health.check_status = SessionCheckStatus::Dead;
    health.checked_generation = Some(1);
    health.last_checked_at = Some("2026-01-01T00:01:00.000Z".to_owned());
    write_session_health(&fixture.control_db_path, &health).expect("write health");

    let first = list_sessions(&fixture.control_db_path, &options(fixed_now())).expect("first list");
    assert_eq!(first[0].check_status, SessionCheckStatus::Dead);
    assert!(!first[0].eligible_to_receive);

    fixture.register_listener("session-dead", std::process::id(), None);
    let after_restart = list_sessions(&fixture.control_db_path, &options(fixed_now())).expect("second list");
    assert_eq!(after_restart[0].check_status, SessionCheckStatus::Ok);
    assert!(after_restart[0].eligible_to_receive);
    assert_eq!(after_restart[0].agent_generation, 2);
}

#[test]
fn does_not_revive_an_unbound_historical_session_from_a_reused_live_pid() {
    let fixture = Fixture::new();
    fixture.write_session("session-historical", SessionOverrides::default());
    let mut health = empty_session_health("session-historical", "2026-01-01T00:00:00.000Z");
    health.agent_generation = 1;
    health.pid = Some(std::process::id());
    health.last_active_at = Some("2026-01-01T00:00:00.000Z".to_owned());
    health.last_checked_at = Some("2026-01-01T00:00:00.000Z".to_owned());
    health.check_status = SessionCheckStatus::Ok;
    health.checked_generation = Some(1);
    write_session_health(&fixture.control_db_path, &health).expect("write health");

    let mut list_options = options(eleven_minutes_from_now());
    list_options.is_runtime_process_alive = Some(always_alive());
    let sessions = list_sessions(&fixture.control_db_path, &list_options).expect("list");

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].pid, None);
    assert_eq!(sessions[0].status, SessionLiveStatus::Ended);
    assert_eq!(sessions[0].check_status, SessionCheckStatus::Dead);
    assert!(!sessions[0].eligible_to_receive);
}

#[test]
fn expires_a_fresh_binding_when_its_pid_no_longer_belongs_to_a_pi_runtime() {
    let fixture = Fixture::new();
    fixture.write_session("session-fresh-reused-pid", SessionOverrides::default());
    fixture.register_listener("session-fresh-reused-pid", std::process::id(), None);

    let mut list_options = options(fixed_now());
    list_options.is_runtime_process_alive = Some(never_alive());
    let sessions = list_sessions(&fixture.control_db_path, &list_options).expect("list");

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].pid, None);
    assert_eq!(sessions[0].status, SessionLiveStatus::Ended);
    assert_eq!(sessions[0].check_status, SessionCheckStatus::Dead);
    assert!(
        list_runtime_mailbox_listeners(&fixture.control_db_path)
            .expect("listeners")
            .is_empty()
    );
}

#[test]
fn expires_a_stale_binding_when_its_pid_no_longer_belongs_to_a_pi_runtime() {
    let fixture = Fixture::new();
    fixture.write_session("session-stale", SessionOverrides::default());
    fixture.register_listener("session-stale", std::process::id(), None);

    let mut list_options = options(eleven_minutes_from_now());
    list_options.is_runtime_process_alive = Some(never_alive());
    let sessions = list_sessions(&fixture.control_db_path, &list_options).expect("list");

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].pid, None);
    assert_eq!(sessions[0].status, SessionLiveStatus::Ended);
    assert_eq!(sessions[0].check_status, SessionCheckStatus::Dead);
    assert!(
        list_runtime_mailbox_listeners(&fixture.control_db_path)
            .expect("listeners")
            .is_empty()
    );
}

#[test]
fn rejects_a_future_dated_heartbeat_when_its_pid_is_not_a_pi_runtime() {
    let fixture = Fixture::new();
    fixture.write_session("session-future", SessionOverrides::default());
    fixture.register_listener("session-future", std::process::id(), None);

    let mut list_options = options(fixed_now());
    list_options.is_runtime_process_alive = Some(never_alive());
    let sessions = list_sessions(&fixture.control_db_path, &list_options).expect("list");

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].pid, None);
    assert_eq!(sessions[0].status, SessionLiveStatus::Ended);
    assert_eq!(sessions[0].check_status, SessionCheckStatus::Dead);
    assert!(!sessions[0].eligible_to_receive);
    assert!(
        list_runtime_mailbox_listeners(&fixture.control_db_path)
            .expect("listeners")
            .is_empty()
    );
}

#[test]
fn does_not_abort_a_stale_supervisor_store_while_its_process_is_still_alive() {
    let fixture = Fixture::new();
    fixture.write_session("session-stale-live", SessionOverrides::default());
    fixture.register_listener("session-stale-live", std::process::id(), None);

    let sessions = list_sessions(&fixture.control_db_path, &options(eleven_minutes_from_now())).expect("list");

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].pid, Some(std::process::id()));
    assert_eq!(sessions[0].status, SessionLiveStatus::Ended);
    assert_eq!(sessions[0].check_status, SessionCheckStatus::Timeout);
    assert!(!sessions[0].eligible_to_receive);
    assert_eq!(
        list_runtime_mailbox_listeners(&fixture.control_db_path)
            .expect("listeners")
            .len(),
        1
    );
}

#[test]
fn retires_stale_listeners_while_listing() {
    let fixture = Fixture::new();
    fixture.write_session("session-stale", SessionOverrides::default());
    fixture.write_session("session-live", SessionOverrides::default());
    fixture.register_listener("session-stale", std::process::id(), None);
    fixture.register_listener("session-live", std::process::id() + 1, None);

    let mut list_options = options(eleven_minutes_from_now());
    let live_pid = std::process::id() + 1;
    list_options.is_runtime_process_alive = Some(Arc::new(move |pid| pid == live_pid));
    list_sessions(&fixture.control_db_path, &list_options).expect("list");

    let stale = read_session_health(&fixture.control_db_path, "session-stale")
        .expect("health")
        .expect("row");
    assert_eq!(stale.pid, None);
}

#[test]
fn excludes_ended_sessions_when_include_ended_is_false() {
    let fixture = Fixture::new();
    fixture.write_session("session-ended", SessionOverrides::default());
    let mut health = empty_session_health("session-ended", "2026-01-01T00:00:00.000Z");
    health.agent_generation = 1;
    health.check_status = SessionCheckStatus::Dead;
    health.checked_generation = Some(1);
    health.last_checked_at = Some("2026-01-01T00:01:00.000Z".to_owned());
    write_session_health(&fixture.control_db_path, &health).expect("write health");

    let mut list_options = options(fixed_now());
    list_options.include_ended = Some(false);
    assert!(
        list_sessions(&fixture.control_db_path, &list_options)
            .expect("list")
            .is_empty()
    );
}

#[test]
fn keeps_global_inventory_on_the_current_main_session_binding() {
    let fixture = Fixture::new();
    fixture.write_session(
        "session-historical",
        SessionOverrides {
            cwd: Some("/repo/historical"),
            ..Default::default()
        },
    );
    fixture.write_session(
        "session-current",
        SessionOverrides {
            cwd: Some("/repo/current"),
            ..Default::default()
        },
    );
    fixture.register_listener("session-historical", std::process::id(), None);
    fixture.register_listener("session-current", std::process::id(), None);

    let mut list_options = options(fixed_now());
    list_options.include_ended = Some(false);
    let sessions = list_sessions(&fixture.control_db_path, &list_options).expect("list");
    let ids: Vec<_> = sessions.iter().map(|entry| entry.session_id.as_str()).collect();
    assert_eq!(ids, vec!["session-current"]);
    let health = read_session_health(&fixture.control_db_path, "session-historical")
        .expect("health")
        .expect("row");
    assert_eq!(health.pid, None);
    assert_eq!(health.check_status, SessionCheckStatus::Dead);
}

#[test]
fn uses_session_path_as_the_deterministic_tie_break_for_duplicate_metadata() {
    let fixture = Fixture::new();
    fixture.write_session(
        "session-live",
        SessionOverrides {
            cwd: Some("/repo/historical"),
            session_path: Some("/sessions/a-historical.jsonl"),
            modified_at: Some("2026-01-01T00:11:00.000Z"),
            ..Default::default()
        },
    );
    fixture.write_session(
        "session-live",
        SessionOverrides {
            cwd: Some("/repo/current"),
            session_path: Some("/sessions/z-current.jsonl"),
            modified_at: Some("2026-01-01T00:11:00.000Z"),
            ..Default::default()
        },
    );
    fixture.register_listener("session-live", std::process::id(), None);

    let mut list_options = options(fixed_now());
    list_options.include_ended = Some(false);
    let sessions = list_sessions(&fixture.control_db_path, &list_options).expect("list");

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].session_id, "session-live");
    assert_eq!(sessions[0].session_path, "/sessions/z-current.jsonl");
    assert_eq!(sessions[0].cwd.as_deref(), Some("/repo/current"));
}

#[test]
fn broadcasts_only_to_the_newest_main_session_listener_for_a_shared_live_pid() {
    let fixture = Fixture::new();
    fixture.write_session(
        "session-historical",
        SessionOverrides {
            cwd: Some("/repo/historical"),
            ..Default::default()
        },
    );
    fixture.write_session(
        "session-current",
        SessionOverrides {
            cwd: Some("/repo/current"),
            ..Default::default()
        },
    );
    fixture.register_listener("session-historical", std::process::id(), None);
    fixture.register_listener("session-current", std::process::id(), None);

    let now = fixed_now();
    let mut directory_options = options(now);
    directory_options.is_runtime_process_alive = Some(always_alive());
    let sent = broadcast_to_sessions(
        &fixture.control_db_path,
        &BroadcastSessionsInput {
            message: "please restart".to_owned(),
            filters: None,
            sender_session_id: "sender-session".to_owned(),
            sender_session_path: "/sessions/sender.jsonl".to_owned(),
            sender_agent_id: None,
        },
        &directory_options,
    )
    .expect("broadcast");

    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].session_id, "session-current");
    assert_eq!(
        sent[0].outcome,
        pi_coding_agent::core::session_health::SessionBroadcastOutcome::Sent
    );
    assert_eq!(sent[0].check_status, SessionCheckStatus::Ok);

    let messages = list_runtime_mailbox_messages(&fixture.control_db_path).expect("messages");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].recipient.session_id, "session-current");

    let filtered = broadcast_to_sessions(
        &fixture.control_db_path,
        &BroadcastSessionsInput {
            message: "please restart again".to_owned(),
            filters: Some(BroadcastSessionFilters {
                session_ids: Some(vec!["session-historical".to_owned()]),
                cwd: None,
                name: None,
                status: None,
            }),
            sender_session_id: "sender-session".to_owned(),
            sender_session_path: "/sessions/sender.jsonl".to_owned(),
            sender_agent_id: None,
        },
        &directory_options,
    )
    .expect("filtered broadcast");

    assert_eq!(filtered.len(), 1);
    assert_eq!(
        filtered[0].outcome,
        pi_coding_agent::core::session_health::SessionBroadcastOutcome::SkippedFilter
    );
    assert_eq!(
        list_runtime_mailbox_messages(&fixture.control_db_path)
            .expect("messages")
            .len(),
        1
    );
}

#[test]
fn broadcasts_once_to_a_session_with_multiple_metadata_paths() {
    let fixture = Fixture::new();
    fixture.write_session(
        "session-live",
        SessionOverrides {
            session_path: Some("/sessions/session-live-old.jsonl"),
            modified_at: Some("2026-01-01T00:10:00.000Z"),
            ..Default::default()
        },
    );
    fixture.write_session(
        "session-live",
        SessionOverrides {
            session_path: Some("/sessions/session-live-new.jsonl"),
            modified_at: Some("2026-01-01T00:11:00.000Z"),
            ..Default::default()
        },
    );
    fixture.register_listener("session-live", std::process::id(), None);

    let results = broadcast_to_sessions(
        &fixture.control_db_path,
        &BroadcastSessionsInput {
            message: "please restart".to_owned(),
            filters: None,
            sender_session_id: "sender-session".to_owned(),
            sender_session_path: "/sessions/sender.jsonl".to_owned(),
            sender_agent_id: None,
        },
        &options(fixed_now()),
    )
    .expect("broadcast");

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].session_id, "session-live");
    let messages = list_runtime_mailbox_messages(&fixture.control_db_path).expect("messages");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].recipient.session_id, "session-live");
}

#[test]
fn matches_broadcast_filters_against_the_newest_metadata_entry() {
    let fixture = Fixture::new();
    fixture.write_session(
        "session-live",
        SessionOverrides {
            cwd: Some("/repo/old"),
            session_path: Some("/sessions/session-live-old.jsonl"),
            modified_at: Some("2026-01-01T00:10:00.000Z"),
            ..Default::default()
        },
    );
    fixture.write_session(
        "session-live",
        SessionOverrides {
            cwd: Some("/repo/new"),
            session_path: Some("/sessions/session-live-new.jsonl"),
            modified_at: Some("2026-01-01T00:11:00.000Z"),
            ..Default::default()
        },
    );
    fixture.register_listener("session-live", std::process::id(), None);

    let results = broadcast_to_sessions(
        &fixture.control_db_path,
        &BroadcastSessionsInput {
            message: "please restart".to_owned(),
            filters: Some(BroadcastSessionFilters {
                session_ids: None,
                cwd: Some("/repo/old".to_owned()),
                name: None,
                status: None,
            }),
            sender_session_id: "sender-session".to_owned(),
            sender_session_path: "/sessions/sender.jsonl".to_owned(),
            sender_agent_id: None,
        },
        &options(fixed_now()),
    )
    .expect("broadcast");

    assert_eq!(
        results[0].outcome,
        pi_coding_agent::core::session_health::SessionBroadcastOutcome::SkippedFilter
    );
    assert!(
        list_runtime_mailbox_messages(&fixture.control_db_path)
            .expect("messages")
            .is_empty()
    );
}

#[test]
fn broadcasts_only_to_eligible_matching_current_sessions() {
    let fixture = Fixture::new();
    fixture.write_session(
        "session-live",
        SessionOverrides {
            name: Some("Live"),
            cwd: Some("/repo/live"),
            ..Default::default()
        },
    );
    fixture.write_session(
        "session-filtered",
        SessionOverrides {
            name: Some("Other"),
            cwd: Some("/repo/other"),
            ..Default::default()
        },
    );
    fixture.register_listener("session-live", std::process::id(), None);
    fixture.register_listener("session-filtered", std::process::id() + 1, None);

    let mut directory_options = options(fixed_now());
    directory_options.is_runtime_process_alive = Some(always_alive());
    let results = broadcast_to_sessions(
        &fixture.control_db_path,
        &BroadcastSessionsInput {
            message: "please restart".to_owned(),
            filters: Some(BroadcastSessionFilters {
                session_ids: Some(vec!["session-live".to_owned()]),
                cwd: None,
                name: None,
                status: None,
            }),
            sender_session_id: "sender-session".to_owned(),
            sender_session_path: "/sessions/sender.jsonl".to_owned(),
            sender_agent_id: None,
        },
        &directory_options,
    )
    .expect("broadcast");

    let outcome = |session_id: &str| {
        results
            .iter()
            .find(|result| result.session_id == session_id)
            .map(|result| result.outcome)
            .expect("result for session")
    };
    assert_eq!(
        outcome("session-live"),
        pi_coding_agent::core::session_health::SessionBroadcastOutcome::Sent
    );
    assert_eq!(
        outcome("session-filtered"),
        pi_coding_agent::core::session_health::SessionBroadcastOutcome::SkippedFilter
    );

    let messages = list_runtime_mailbox_messages(&fixture.control_db_path).expect("messages");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].recipient.session_id, "session-live");
    let health = read_session_health(&fixture.control_db_path, "session-live")
        .expect("health")
        .expect("row");
    assert_eq!(health.check_status, SessionCheckStatus::Ok);
}

#[test]
fn broadcast_rejects_an_empty_message() {
    let fixture = Fixture::new();
    let error = broadcast_to_sessions(
        &fixture.control_db_path,
        &BroadcastSessionsInput {
            message: "   ".to_owned(),
            filters: None,
            sender_session_id: "sender-session".to_owned(),
            sender_session_path: "/sessions/sender.jsonl".to_owned(),
            sender_agent_id: None,
        },
        &options(fixed_now()),
    )
    .expect_err("empty broadcast must fail");
    assert_eq!(error.to_string(), "broadcast requires a non-empty message");
}
