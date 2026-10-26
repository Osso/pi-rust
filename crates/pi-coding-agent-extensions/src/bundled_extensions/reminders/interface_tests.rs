use super::actions::{ListStatus, ReminderAction};
use super::interface::*;
use pi_js_compat::js_json::JsString;

#[test]
fn command_parses_js_whitespace_dates_and_ids() {
    let request = parse_reminders_command(&"\u{feff}add\u{a0}2026-10-26\t09:30  A  title".into()).unwrap();
    assert_eq!(request.action, ReminderAction::Add);
    assert_eq!(request.due_at, Some("2026-10-26 09:30".into()));
    assert_eq!(request.title, Some("A title".into()));
    let request = parse_reminders_command(&"done 0x10 ignored".into()).unwrap();
    assert_eq!(request.id, Some(16));
    assert_eq!(request.action, ReminderAction::Complete);
    assert_eq!(
        parse_reminders_command(&"list due extra".into()).unwrap().status,
        Some(ListStatus::Due)
    );
    assert_eq!(
        parse_reminders_command(&"done 9007199254740992".into()).unwrap_err(),
        COMMAND_USAGE
    );
}

#[test]
fn command_errors_are_notifications_and_actions_execute() {
    assert_eq!(
        command_notification(&"done nope".into(), None, 0.0),
        (JsString::from(COMMAND_USAGE), "error")
    );
    assert_eq!(
        command_notification(&"".into(), None, 0.0),
        (JsString::from("Reminders require the Pi control database."), "error")
    );
    let dir = tempfile::tempdir().unwrap();
    let path = JsString::from(dir.path().join("control.db").to_str().unwrap());
    let (text, level) = command_notification(&"add 2026-10-26T12:00Z Buy milk".into(), Some(&path), 0.0);
    assert_eq!(level, "info");
    assert!(text.starts_with("Added reminder #1 [open] due "));
    assert!(text.ends_with("Buy milk"));
    let (text, level) = command_notification(&"done 1".into(), Some(&path), 0.0);
    assert_eq!(level, "info");
    assert!(text.starts_with("Completed reminder #1 [done] due "));
}

#[test]
fn tool_executes_actions_and_preserves_text() {
    let dir = tempfile::tempdir().unwrap();
    let path = JsString::from(dir.path().join("control.db").to_str().unwrap());
    let params = pi_js_compat::js_json!({"action": "add", "title": "A title", "dueAt": "2026-10-26T12:00Z", "projectPath": "/work", "notes": "line 1\nline 2"});
    let result = execute_reminders_tool(&params, Some(&path), 0.0).unwrap();
    assert!(result.starts_with("Added reminder #1 [open] due "));
    assert!(result.ends_with("A title (project: /work)\n  line 1\n  line 2"));
    let list = pi_js_compat::js_json!({"action": "list", "status": "all"});
    assert!(
        execute_reminders_tool(&list, Some(&path), 0.0)
            .unwrap()
            .starts_with("#1 [open] due ")
    );
    let delete = pi_js_compat::js_json!({"action": "delete", "id": 1});
    assert_eq!(
        execute_reminders_tool(&delete, Some(&path), 0.0).unwrap(),
        JsString::from("Deleted reminder #1.")
    );
    assert_eq!(
        execute_reminders_tool(&list, Some(&path), 0.0).unwrap(),
        JsString::from("No all reminders.")
    );
    assert_eq!(
        execute_reminders_tool(&list, None, 0.0).unwrap_err(),
        "Reminders require the Pi control database."
    );
}

#[test]
fn tool_schema_exposes_frozen_fields() {
    let schema = reminder_tool_schema();
    assert_eq!(schema["required"], serde_json::json!(["action"]));
    assert_eq!(schema["properties"]["id"]["type"], "integer");
    assert_eq!(schema["properties"]["action"]["anyOf"][6]["const"], "delete");
    assert_eq!(schema["properties"]["dueAt"]["description"], DUE_AT_HELP);
}
