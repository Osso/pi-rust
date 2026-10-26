use super::actions::*;
use crate::core::session_control_db::{Reminder, ReminderStatus};
use pi_js_compat::js_json::JsString;

fn request(action: ReminderAction) -> ReminderRequest {
    ReminderRequest {
        action,
        ..Default::default()
    }
}

#[test]
fn action_validation_precedes_database_access() {
    let path = JsString::from("/nonexistent/reminders.db");
    assert_eq!(
        execute_reminder_action(&path, &request(ReminderAction::Add), 0.0).unwrap_err(),
        "title is required."
    );
    assert_eq!(
        execute_reminder_action(&path, &request(ReminderAction::Complete), 0.0).unwrap_err(),
        "Reminder id is required."
    );
    let mut add = request(ReminderAction::Add);
    add.title = Some("  title  ".into());
    assert_eq!(
        execute_reminder_action(&path, &add, 0.0).unwrap_err(),
        "dueAt is required."
    );
}

#[test]
fn formats_notes_and_preserves_utf16() {
    let reminder = Reminder {
        id: 7,
        title: JsString::from_utf16(vec![0xd800, b'!' as u16]),
        notes: JsString::from_utf16(vec![b'a' as u16, 10, 0xdc00]),
        project_path: Some("/work".into()),
        due_at: "2026-10-26T12:00:00.000Z".into(),
        status: ReminderStatus::Open,
        created_at: "2026-01-01T00:00:00.000Z".into(),
        completed_at: None,
        delivered_at: None,
    };
    let formatted = format_reminder(&reminder).unwrap();
    let mut expected = JsString::from(format!(
        "#7 [open] due {}: ",
        pi_js_compat::reminder_time::format_reminder_due_at("2026-10-26T12:00:00.000Z").unwrap()
    ));
    expected.push_str(reminder.title.clone());
    expected.push_str(" (project: /work)\n  a\n  ");
    expected.push_unit(0xdc00);
    assert_eq!(formatted, expected);
}

#[test]
fn edit_reschedule_reopen_and_due_list() {
    let dir = tempfile::tempdir().unwrap();
    let path = JsString::from(dir.path().join("control.db").to_str().unwrap());
    let mut add = request(ReminderAction::Add);
    add.title = Some("first".into());
    add.project_path = Some(Some("/project".into()));
    add.due_at = Some("2020-01-01T00:00Z".into());
    let created = execute_reminder_action(&path, &add, 0.0).unwrap();
    let id = Some(created[0].id);
    let mut edit = request(ReminderAction::Edit);
    edit.id = id;
    edit.title = Some("  revised  ".into());
    edit.project_path = Some(None);
    edit.notes = Some("note".into());
    let edited = execute_reminder_action(&path, &edit, 0.0).unwrap();
    assert_eq!(edited[0].title, JsString::from("  revised  "));
    assert_eq!(edited[0].project_path, None);
    assert!(
        describe_result(&edit, &edited)
            .unwrap()
            .starts_with("Edited reminder #1 [open] due ")
    );
    let mut reschedule = request(ReminderAction::Reschedule);
    reschedule.id = id;
    reschedule.due_at = Some(" 2021-01-01T00:00Z ".into());
    let changed = execute_reminder_action(&path, &reschedule, 0.0).unwrap();
    assert_eq!(changed[0].due_at, JsString::from("2021-01-01T00:00:00.000Z"));
    let mut due = request(ReminderAction::List);
    due.status = Some(ListStatus::Due);
    assert_eq!(
        execute_reminder_action(&path, &due, 1_735_689_600_000.0).unwrap().len(),
        1
    );
    let mut complete = request(ReminderAction::Complete);
    complete.id = id;
    execute_reminder_action(&path, &complete, 0.0).unwrap();
    assert!(
        execute_reminder_action(&path, &due, 1_735_689_600_000.0)
            .unwrap()
            .is_empty()
    );
    let mut reopen = request(ReminderAction::Reopen);
    reopen.id = id;
    assert_eq!(
        execute_reminder_action(&path, &reopen, 0.0).unwrap()[0].status,
        ReminderStatus::Open
    );
}

#[test]
fn crud_actions_and_empty_list_descriptions() {
    let dir = tempfile::tempdir().unwrap();
    let path = JsString::from(dir.path().join("control.db").to_str().unwrap());
    let mut add = request(ReminderAction::Add);
    add.title = Some("  Test  ".into());
    add.due_at = Some("2026-10-26T12:00Z".into());
    let created = execute_reminder_action(&path, &add, 0.0).unwrap();
    assert_eq!(created[0].title, JsString::from("Test"));
    let mut complete = request(ReminderAction::Complete);
    complete.id = Some(created[0].id);
    assert_eq!(
        execute_reminder_action(&path, &complete, 0.0).unwrap()[0].status,
        ReminderStatus::Done
    );
    let list = request(ReminderAction::List);
    assert!(execute_reminder_action(&path, &list, 0.0).unwrap().is_empty());
    assert_eq!(
        describe_result(&list, &[]).unwrap(),
        JsString::from("No open reminders.")
    );
    let mut delete = request(ReminderAction::Delete);
    delete.id = complete.id;
    assert!(execute_reminder_action(&path, &delete, 0.0).unwrap().is_empty());
    assert_eq!(
        describe_result(&delete, &[]).unwrap(),
        JsString::from("Deleted reminder #1.")
    );
}
