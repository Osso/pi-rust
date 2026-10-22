use pi_coding_agent::core::session_health::{
    SessionCheckStatus, SessionHealthRecord, SessionLiveStatus, advance_generation_for_pid, derive_live_status,
    empty_session_health, end_session_health, is_session_eligible_to_receive, is_sticky_dead, parse_goal_objective,
};
use pi_js_compat::js_json::JsString;

const NOW_ISO: &str = "2026-01-01T00:00:00.000Z";

fn live() -> SessionHealthRecord {
    SessionHealthRecord {
        agent_generation: 1,
        pid: Some(1234),
        check_status: SessionCheckStatus::Ok,
        checked_generation: Some(1),
        last_active_at: Some("2025-12-31T23:59:00.000Z".into()),
        ..empty_session_health("session-a", NOW_ISO)
    }
}

#[test]
fn parses_goal_objectives_from_json_payloads() {
    assert_eq!(
        parse_goal_objective(Some(&JsString::from(r#"{"objective":"ship list_sessions"}"#))),
        Some("ship list_sessions".into())
    );
    assert_eq!(
        parse_goal_objective(Some(&JsString::from("raw goal text"))),
        Some("raw goal text".into())
    );
    assert_eq!(parse_goal_objective(None), None);
    assert_eq!(parse_goal_objective(Some(&JsString::from(""))), None);
    assert_eq!(parse_goal_objective(Some(&JsString::from("   \n "))), None);
    assert_eq!(
        parse_goal_objective(Some(&JsString::from(r#"  {"objective":"  padded  "}  "#))),
        Some("padded".into())
    );
    assert_eq!(
        parse_goal_objective(Some(&JsString::from(r#"{"objective":"   "}"#))),
        Some(r#"{"objective":"   "}"#.into())
    );
    assert_eq!(
        parse_goal_objective(Some(&JsString::from(r#"{"objective":5}"#))),
        Some(r#"{"objective":5}"#.into())
    );
    assert_eq!(parse_goal_objective(Some(&JsString::from("[1]"))), Some("[1]".into()));
    assert_eq!(parse_goal_objective(Some(&JsString::from("{bad"))), Some("{bad".into()));
    let lone = JsString::from(r#"{"objective":"\ud800 x"}"#);
    assert_eq!(
        parse_goal_objective(Some(&lone)),
        Some(JsString::from_utf16(vec![0xd800, 0x20, 0x78])),
        "escaped lone surrogate objective survives"
    );
    assert_eq!(
        parse_goal_objective(Some(&JsString::from_utf16(vec![0x20, 0xdc00, 0x20]))),
        Some(JsString::from_utf16(vec![0xdc00])),
        "raw lone surrogate payload falls through untouched"
    );
}

#[test]
fn marks_sticky_dead_only_for_the_same_generation() {
    let health = SessionHealthRecord {
        agent_generation: 2,
        check_status: SessionCheckStatus::Dead,
        checked_generation: Some(2),
        ..empty_session_health("session-a", NOW_ISO)
    };
    assert!(is_sticky_dead(&health));
    assert!(!is_sticky_dead(&SessionHealthRecord {
        agent_generation: 3,
        ..health.clone()
    }));
    assert!(!is_sticky_dead(&SessionHealthRecord {
        check_status: SessionCheckStatus::Timeout,
        ..health.clone()
    }));
    assert!(!is_sticky_dead(&SessionHealthRecord {
        checked_generation: None,
        ..health
    }));
}

#[test]
fn builds_empty_health_records() {
    assert_eq!(
        serde_json::to_value(empty_session_health("s", NOW_ISO)).unwrap(),
        serde_json::json!({
            "sessionId": "s",
            "agentGeneration": 0,
            "pid": null,
            "lastActiveAt": null,
            "lastCheckedAt": null,
            "checkStatus": "never",
            "checkedGeneration": null,
            "checkLatencyMs": null,
            "updatedAt": NOW_ISO
        })
    );
}

#[test]
fn requires_ok_status_and_no_sticky_death_to_receive() {
    assert!(is_session_eligible_to_receive(&live()));
    assert!(!is_session_eligible_to_receive(&SessionHealthRecord {
        check_status: SessionCheckStatus::Never,
        ..live()
    }));
    assert!(!is_session_eligible_to_receive(&SessionHealthRecord {
        check_status: SessionCheckStatus::Timeout,
        ..live()
    }));
    assert!(!is_session_eligible_to_receive(&end_session_health(&live(), NOW_ISO)));
}

#[test]
fn derives_live_status_from_status_pid_and_staleness() {
    let stale_ms = 300_000;
    let now = 1_767_225_600_000;
    assert_eq!(
        derive_live_status(&live(), live().last_active_at.as_deref(), now, stale_ms),
        SessionLiveStatus::Running
    );
    assert_eq!(
        derive_live_status(&live(), Some("2025-12-31T23:50:00.000Z"), now, stale_ms),
        SessionLiveStatus::Idle
    );
    assert_eq!(
        derive_live_status(&live(), Some("2025-12-31T23:55:00.000Z"), now, stale_ms),
        SessionLiveStatus::Running,
        "exactly staleMs old still counts as running (nowMs - lastActiveMs <= staleMs)"
    );
    assert_eq!(
        derive_live_status(&live(), None, now, stale_ms),
        SessionLiveStatus::Idle,
        "missing last activity is idle, not running"
    );
    assert_eq!(
        derive_live_status(&live(), Some("not a date"), now, stale_ms),
        SessionLiveStatus::Idle
    );
    assert_eq!(
        derive_live_status(
            &SessionHealthRecord { pid: None, ..live() },
            Some(NOW_ISO),
            now,
            stale_ms
        ),
        SessionLiveStatus::Ended
    );
    assert_eq!(
        derive_live_status(
            &SessionHealthRecord {
                check_status: SessionCheckStatus::Timeout,
                ..live()
            },
            Some(NOW_ISO),
            now,
            stale_ms
        ),
        SessionLiveStatus::Ended
    );
    assert_eq!(
        derive_live_status(
            &SessionHealthRecord {
                check_status: SessionCheckStatus::Dead,
                ..live()
            },
            Some(NOW_ISO),
            now,
            stale_ms
        ),
        SessionLiveStatus::Ended
    );
}

#[test]
fn derives_running_for_future_and_offset_timestamps() {
    let now = 1_767_225_600_000;
    let stale_ms = 300_000;
    assert_eq!(
        derive_live_status(&live(), Some("2027-01-01T00:00:00.000Z"), now, stale_ms),
        SessionLiveStatus::Running
    );
    assert_eq!(
        derive_live_status(&live(), Some("2026-01-01T02:00:00.000+02:00"), now, stale_ms),
        SessionLiveStatus::Running
    );
    assert_eq!(
        derive_live_status(&live(), Some("2025-12-31T19:00:00.000-05:00"), now, stale_ms),
        SessionLiveStatus::Running
    );
    assert_eq!(
        derive_live_status(&live(), Some("2026-01-01"), now, stale_ms),
        SessionLiveStatus::Running,
        "date-only strings parse as UTC midnight"
    );
}

#[test]
fn ends_session_health_without_probing_the_historical_pid() {
    let ended = end_session_health(&live(), NOW_ISO);
    assert_eq!(
        serde_json::to_value(&ended).unwrap(),
        serde_json::json!({
            "sessionId": "session-a",
            "agentGeneration": 1,
            "pid": null,
            "lastActiveAt": "2025-12-31T23:59:00.000Z",
            "lastCheckedAt": NOW_ISO,
            "checkStatus": "dead",
            "checkedGeneration": 1,
            "checkLatencyMs": 0,
            "updatedAt": NOW_ISO
        })
    );
    assert!(is_sticky_dead(&ended));
    assert!(!is_session_eligible_to_receive(&ended));
}

#[test]
fn advances_generation_only_when_the_pid_changes() {
    let first = advance_generation_for_pid(&empty_session_health("s", NOW_ISO), 42, "2026-01-01T00:00:01.000Z");
    assert_eq!(first.agent_generation, 1);
    assert_eq!(first.pid, Some(42));
    assert_eq!(first.updated_at, "2026-01-01T00:00:01.000Z");

    let same = advance_generation_for_pid(&first, 42, "2026-01-01T00:00:02.000Z");
    assert_eq!(same.agent_generation, 1);
    assert_eq!(same.last_active_at.as_deref(), Some("2026-01-01T00:00:02.000Z"));
    assert_eq!(same.updated_at, "2026-01-01T00:00:02.000Z");

    let changed = advance_generation_for_pid(&same, 43, "2026-01-01T00:00:03.000Z");
    assert_eq!(changed.agent_generation, 2);
    assert_eq!(changed.pid, Some(43));
    assert!(!is_sticky_dead(&SessionHealthRecord {
        check_status: SessionCheckStatus::Dead,
        checked_generation: Some(1),
        ..changed
    }));
}

#[test]
fn restarts_generation_for_a_zero_generation_same_pid() {
    let health = SessionHealthRecord {
        pid: Some(7),
        agent_generation: 0,
        ..empty_session_health("s", NOW_ISO)
    };
    assert_eq!(advance_generation_for_pid(&health, 7, NOW_ISO).agent_generation, 1);
}

#[test]
fn serializes_check_status_wire_names() {
    let statuses = [
        SessionCheckStatus::Ok,
        SessionCheckStatus::Dead,
        SessionCheckStatus::Timeout,
        SessionCheckStatus::Never,
    ];
    assert_eq!(
        serde_json::to_value(statuses).unwrap(),
        serde_json::json!(["ok", "dead", "timeout", "never"])
    );
}
