use super::*;

#[test]
fn edits_preserve_delivery_until_due_changes_and_complete_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db").to_string_lossy().into_owned();
    let reminder = create_reminder(
        path.clone(),
        &CreateReminderInput {
            title: "test".into(),
            notes: None,
            project_path: Some("/repo".into()),
            due_at: "2026-10-26T00:00Z".into(),
        },
    )
    .unwrap();
    let delivered = mark_reminder_delivered(
        path.clone(),
        reminder.id,
        &reminder.due_at,
        &"2026-10-26T01:00:00.000Z".into(),
    )
    .unwrap()
    .unwrap();
    let updated = update_reminder(
        path.clone(),
        reminder.id,
        &ReminderPatch {
            due_at: Some("2026-10-26T01:00+01:00".into()),
            project_path: Some(None),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(updated.delivered_at, delivered.delivered_at);
    assert_eq!(updated.project_path, None);
    let done = complete_reminder(path.clone(), reminder.id).unwrap();
    assert_eq!(
        complete_reminder(path.clone(), reminder.id).unwrap().completed_at,
        done.completed_at
    );
    assert!(
        list_undelivered_due_reminders(path.clone(), &"2027-01-01T00:00:00.000Z".into())
            .unwrap()
            .is_empty()
    );
    let reopened = reopen_reminder(path.clone(), reminder.id).unwrap();
    assert_eq!(reopened.completed_at, None);
    assert_eq!(reopened.delivered_at, None);
    assert_eq!(list_reminders(path, ReminderListStatus::Open).unwrap().len(), 1);
}

#[test]
fn invalid_inputs_return_source_errors() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db").to_string_lossy().into_owned();
    let input = |title: &str, due: &str| CreateReminderInput {
        title: title.into(),
        notes: None,
        project_path: None,
        due_at: due.into(),
    };
    assert_eq!(
        create_reminder(path.clone(), &input(" ", "2026-10-26T00:00Z"))
            .unwrap_err()
            .to_string(),
        "Reminder title must not be empty"
    );
    for date in ["2026-02-30T00:00Z", "2026-10-26T00:00", "2026-13-01T00:00Z"] {
        assert_eq!(
            create_reminder(path.clone(), &input("test", date))
                .unwrap_err()
                .to_string(),
            "Reminder dueAt must be a valid ISO timestamp with an explicit offset"
        );
    }
    assert_eq!(
        create_reminder(path.clone(), &input("\u{feff}", "2026-10-26T00:00Z"))
            .unwrap_err()
            .to_string(),
        "Reminder title must not be empty"
    );
    assert_eq!(
        delete_reminder(path.clone(), 0).unwrap_err().to_string(),
        "Reminder ID must be a positive safe integer"
    );
    assert_eq!(
        update_reminder(path, 1, &ReminderPatch::default())
            .unwrap_err()
            .to_string(),
        "Reminder edit must contain at least one field"
    );
}
