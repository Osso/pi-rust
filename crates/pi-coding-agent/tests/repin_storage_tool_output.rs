#[path = "support/oracle.rs"]
mod oracle;

use pi_coding_agent::core::session_tool_output::{ToolResultSessionMigrationReport, migrate_tool_result_session_files};
use serde_json::{Value, json};
use std::time::{Duration, SystemTime};

fn report_value(report: ToolResultSessionMigrationReport) -> Value {
    json!({"scannedFiles":report.scanned_files,"changedFiles":report.changed_files,
        "truncatedMessages":report.truncated_messages,"skippedMalformedFiles":report.skipped_malformed_files,
        "skippedNonSessionFiles":report.skipped_non_session_files,"skippedErrorFiles":report.skipped_error_files,
        "backupPaths":report.backup_paths,"errors":report.errors})
}

#[test]
fn migration_reports_follow_frozen_directory_entry_order() {
    // Frozen session-tool-output.ts:215-238 iterates readdirSync entries in returned order.
    let tmp = tempfile::tempdir().unwrap();
    for name in ["z.jsonl", "é.jsonl", "a.jsonl", "漢.jsonl"] {
        std::fs::write(tmp.path().join(name), b"{malformed").unwrap();
    }
    let mut ts = oracle::TsOracle::spawn("storage-tool-output-oracle.mts", "packages/coding-agent/src/core");
    let expected = ts.request(&json!({"id":1,"op":"migrate","dir":tmp.path()}));
    assert_eq!(expected["ok"], true);
    let actual = migrate_tool_result_session_files(&tmp.path().to_str().unwrap().into(), || 1792584000000);
    assert_eq!(report_value(actual), expected["output"]);
    assert_eq!(expected["output"]["skippedMalformedFiles"], 4);
}

#[test]
fn migration_detects_changes_between_negative_epoch_timestamps() {
    // Frozen session-tool-output.ts:322-326 compares Node mtimeMs, including negative values.
    let tmp = tempfile::tempdir().unwrap();
    let mut ts = oracle::TsOracle::spawn("storage-tool-output-oracle.mts", "packages/coding-agent/src/core");
    let created = ts.request(&json!({"id":1,"op":"create","dir":tmp.path(),"text":"x".repeat(1_100_000)}));
    assert_eq!(created["ok"], true);
    let path = created["output"]["path"].as_str().unwrap();
    let original = std::fs::read(path).unwrap();
    let file = std::fs::File::options().write(true).open(path).unwrap();
    file.set_modified(SystemTime::UNIX_EPOCH - Duration::from_secs(2))
        .unwrap();
    let expected = ts.request(&json!({"id":2,"op":"migrate","dir":tmp.path(),"path":path,"advanceMtime":-1}));
    assert_eq!(expected["output"]["skippedErrorFiles"], 1);
    assert_eq!(expected["output"]["changedFiles"], 0);
    assert_eq!(std::fs::read(path).unwrap(), original);
    file.set_modified(SystemTime::UNIX_EPOCH - Duration::from_secs(2))
        .unwrap();
    let actual = migrate_tool_result_session_files(&tmp.path().to_str().unwrap().into(), || {
        file.set_modified(SystemTime::UNIX_EPOCH - Duration::from_secs(1))
            .unwrap();
        1792584000000
    });
    assert_eq!(report_value(actual), expected["output"]);
    assert_eq!(std::fs::read(path).unwrap(), original);
}
