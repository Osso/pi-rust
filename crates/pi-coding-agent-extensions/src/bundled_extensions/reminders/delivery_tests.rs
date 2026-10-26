use super::delivery::*;
use crate::core::session_control_db::{
    CreateReminderInput, ReminderPatch, create_reminder, list_undelivered_due_reminders, update_reminder,
};
use crate::core::session_manager::SessionManager;
use pi_agent_core::harness::messages::CustomMessageContent;
use pi_js_compat::js_json::{self, JsString};

fn setup() -> (tempfile::TempDir, JsString, JsString) {
    let dir = tempfile::tempdir().unwrap();
    let path = JsString::from(dir.path().join("control.db").to_str().unwrap());
    let now = JsString::from("2026-10-26T12:00:00.000Z");
    create_reminder(
        path.clone(),
        &CreateReminderInput {
            title: "Check garden".into(),
            notes: None,
            project_path: None,
            due_at: "2026-10-26T10:00:00.000Z".into(),
        },
    )
    .unwrap();
    (dir, path, now)
}

#[test]
fn queued_message_is_retried_after_abort_and_written_message_is_acknowledged() {
    let (_dir, path, now) = setup();
    let mut state = DeliveryState::default();
    let first = state.check(&path, &now, &[], |_| Ok(())).unwrap();
    assert_eq!(first.len(), 1);
    assert!(state.check(&path, &now, &[], |_| Ok(())).unwrap().is_empty());
    state.settle(&path, &now, &[]).unwrap();
    let second = state.check(&path, &now, &[], |_| Ok(())).unwrap();
    assert_eq!(second.len(), 1);
    assert_ne!(first[0].token, second[0].token);
    state.settle(&path, &now, &[second[0].token.clone()]).unwrap();
    assert!(list_undelivered_due_reminders(path, &now).unwrap().is_empty());
}

#[test]
fn persisted_transcript_token_acknowledges_but_reschedule_cas_does_not() {
    let (dir, path, now) = setup();
    create_reminder(
        path.clone(),
        &CreateReminderInput {
            title: "Second".into(),
            notes: None,
            project_path: None,
            due_at: "2026-10-26T11:00:00.000Z".into(),
        },
    )
    .unwrap();
    let cwd: JsString = dir.path().to_str().unwrap().into();
    let mut session = SessionManager::create(&cwd, Some(&cwd), None).unwrap();
    let assistant = pi_js_compat::js_json!({"role":"assistant", "content":[], "api":"anthropic-messages", "provider":"anthropic", "model":"test", "usage":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":0,"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"total":0}}, "stopReason":"stop", "timestamp":1});
    session
        .append_message(js_json::from_value(&assistant).unwrap())
        .unwrap();
    let file = session.get_session_file().unwrap().clone();
    let mut state = DeliveryState::default();
    let queued = state.check(&path, &now, &[], |_| Ok(())).unwrap();
    session
        .append_custom_message_entry(
            &"reminder".into(),
            CustomMessageContent::Text("due".into()),
            true,
            Some(pi_js_compat::js_json!({"id":1,"deliveryToken":queued[0].token.clone()})),
        )
        .unwrap();
    session
        .append_custom_message_entry(
            &"reminder".into(),
            CustomMessageContent::Text("due".into()),
            true,
            Some(pi_js_compat::js_json!({"id":2,"deliveryToken":queued[1].token.clone()})),
        )
        .unwrap();
    drop(session);
    let reopened = SessionManager::open(&file, Some(&cwd), None).unwrap();
    let tokens = written_entry_tokens(&reopened.get_entries());
    assert_eq!(tokens, vec![queued[0].token.clone(), queued[1].token.clone()]);
    update_reminder(
        path.clone(),
        1,
        &ReminderPatch {
            due_at: Some("2026-10-27T10:00:00Z".into()),
            ..Default::default()
        },
    )
    .unwrap();
    state.settle(&path, &now, &tokens).unwrap();
    let future: JsString = "2026-10-28T00:00:00.000Z".into();
    let due = list_undelivered_due_reminders(path, &future).unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].id, 1);
}

#[test]
fn restart_loses_unwritten_pending_and_retries() {
    const PATH_ENV: &str = "PI_REMINDER_RESTART_TEST_PATH";
    let now: JsString = "2026-10-26T12:00:00.000Z".into();
    if let Ok(path) = std::env::var(PATH_ENV) {
        let second = DeliveryState::default()
            .check(&path.into(), &now, &[], |_| Ok(()))
            .unwrap();
        assert_eq!(second.len(), 1);
        std::fs::write(
            std::env::var("PI_REMINDER_RESTART_TOKEN_FILE").unwrap(),
            second[0].token.to_utf8().unwrap(),
        )
        .unwrap();
        return;
    }
    let (dir, path, _) = setup();
    let first = DeliveryState::default().check(&path, &now, &[], |_| Ok(())).unwrap();
    let token_file = dir.path().join("token");
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "bundled_extensions::reminders::delivery_tests::restart_loses_unwritten_pending_and_retries",
        ])
        .env(PATH_ENV, path.to_utf8().unwrap())
        .env("PI_REMINDER_RESTART_TOKEN_FILE", &token_file)
        .status()
        .unwrap();
    assert!(status.success());
    let second_token = std::fs::read_to_string(token_file).unwrap();
    assert_ne!(first[0].token.to_utf8().unwrap(), second_token);
}
