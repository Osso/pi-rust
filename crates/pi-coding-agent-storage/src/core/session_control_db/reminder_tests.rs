use super::*;
use rusqlite::Connection;

#[test]
fn existing_v15_database_gains_reminders_table() {
    let db = Connection::open_in_memory().unwrap();
    db.pragma_update(None, "user_version", 15).unwrap();
    initialize_schema(&db, None).unwrap();
    db.execute("INSERT INTO reminders (title, notes, due_at, status, created_at) VALUES ('test', '', '2026-10-26T00:00:00.000Z', 'open', '2026-10-26T00:00:00.000Z')", []).unwrap();
    let count: i64 = db
        .query_row("SELECT count(*) FROM reminders", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn reminder_delivery_rescheduling_and_reopening() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("control.sqlite").to_string_lossy().into_owned();
    let input = CreateReminderInput {
        title: "one".into(),
        notes: None,
        project_path: None,
        due_at: "2026-10-26T01:00:00+01:00".into(),
    };
    let reminder = create_reminder(path.clone(), &input).unwrap();
    assert_eq!(reminder.due_at, "2026-10-26T00:00:00.000Z");
    let due = list_undelivered_due_reminders(path.clone(), &"2026-10-26T00:00:00.000Z".into()).unwrap();
    assert_eq!(due.len(), 1);
    assert!(
        mark_reminder_delivered(
            path.clone(),
            reminder.id,
            &reminder.due_at,
            &"2026-10-26T00:01:00.000Z".into()
        )
        .unwrap()
        .is_some()
    );
    assert!(
        mark_reminder_delivered(
            path.clone(),
            reminder.id,
            &reminder.due_at,
            &"2026-10-26T00:01:00.000Z".into()
        )
        .unwrap()
        .is_none()
    );
    let patch = ReminderPatch {
        due_at: Some("2026-10-27T00:00:00Z".into()),
        ..Default::default()
    };
    let updated = update_reminder(path.clone(), reminder.id, &patch).unwrap();
    assert!(updated.delivered_at.is_none());
    assert!(
        mark_reminder_delivered(
            path.clone(),
            reminder.id,
            &reminder.due_at,
            &"2026-10-26T00:01:00.000Z".into()
        )
        .unwrap()
        .is_none()
    );
    assert_eq!(
        complete_reminder(path.clone(), reminder.id).unwrap().status,
        ReminderStatus::Done
    );
    assert_eq!(
        reopen_reminder(path.clone(), reminder.id).unwrap().status,
        ReminderStatus::Open
    );
    delete_reminder(path.clone(), reminder.id).unwrap();
    assert!(
        delete_reminder(path, reminder.id)
            .unwrap_err()
            .to_string()
            .contains("Reminder 1 not found")
    );
}
