use super::*;
use serde_json::Value;
use std::process::Command;

fn fixture(action: &str, path: &str) -> Value {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new("node")
        .current_dir(root)
        .args([
            "--experimental-loader",
            "./oracle/reminder-dependencies-loader.mjs",
            "./oracle/reminder-store-fixture.mjs",
            action,
            path,
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "fixture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn locked_typescript_and_rust_read_each_others_reminders() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("control.sqlite").to_string_lossy().into_owned();
    let ts = fixture("create", &path);
    let rust = list_reminders(path.clone(), ReminderListStatus::All).unwrap();
    assert_eq!(serde_json::to_value(&rust[0]).unwrap(), ts);
    let created = create_reminder(
        path.clone(),
        &CreateReminderInput {
            title: "from Rust".into(),
            notes: Some("native notes".into()),
            project_path: None,
            due_at: "2026-10-27T00:00:00Z".into(),
        },
    )
    .unwrap();
    let ts_rows = fixture("list", &path);
    assert_eq!(ts_rows, serde_json::to_value(vec![rust[0].clone(), created]).unwrap());
}
