use crate::shared_oracle as oracle;

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
fn minimal_tool_primitive_identities_throw_frozen_buffer_errors() {
    use pi_coding_agent::core::session_manager::{SessionEntry, UnknownSessionEntry};
    use pi_coding_agent::core::session_tool_output::truncate_tool_result_for_persistence;
    use pi_js_compat::js_json::JsValue;
    // Frozen session-tool-output.ts:196-205 passes toolCallId to Buffer.from
    // only after all capped entry candidates remain oversized.
    let mut ts = oracle::TsOracle::spawn("storage-tool-output-oracle.mts", "packages/coding-agent/src/core");
    let mut mismatches = Vec::new();
    for key in ["toolCallId", "toolName"] {
        for invalid in [
            None,
            Some(Value::Null),
            Some(json!(true)),
            Some(json!(7)),
            Some(json!(10000)),
            Some(json!(1e12)),
        ] {
            let mut value = json!({"type":"message","id":"raw","parentId":null,"timestamp":"fixture",
                "oversizedEnvelope":"x".repeat(1_100_000),
                "message":{"role":"toolResult","toolCallId":"fixture","toolName":"fixture",
                "content":[],"isError":false,"timestamp":1}});
            if let Some(invalid) = invalid {
                value["message"][key] = invalid;
            } else {
                value["message"].as_object_mut().unwrap().remove(key);
            }
            let entry = SessionEntry::Unknown(UnknownSessionEntry(JsValue::from(&value).as_object().unwrap().clone()));
            let expected = ts.request(&json!({"id":1,"op":"truncate","dir":"unused","entry":value}));
            assert_eq!(expected["output"]["ok"], false, "frozen must throw at this boundary");
            let error = truncate_tool_result_for_persistence(&entry).unwrap_err();
            let actual = json!({"ok":false,"error":error.value().to_serde().unwrap()});
            if actual != expected["output"] {
                mismatches.push(format!("{key}: {actual} != {}", expected["output"]));
            }
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

#[test]
fn nullish_tool_message_preserves_frozen_property_access_errors() {
    use pi_coding_agent::core::session_manager::{SessionEntry, UnknownSessionEntry};
    use pi_coding_agent::core::session_tool_output::truncate_tool_result_for_persistence;
    use pi_js_compat::js_json::JsValue;
    // Frozen session-tool-output.ts:56-57 reads message.role before content.
    let mut ts = oracle::TsOracle::spawn("storage-tool-output-oracle.mts", "packages/coding-agent/src/core");
    let mut mismatches = Vec::new();
    for message in [None, Some(Value::Null)] {
        let mut value = json!({"type":"message","id":"raw","parentId":null,"timestamp":"fixture"});
        if let Some(message) = message {
            value["message"] = message;
        }
        let expected = ts.request(&json!({"id":1,"op":"truncate","entry":value}));
        assert_eq!(expected["output"]["ok"], false);
        let entry = SessionEntry::Unknown(UnknownSessionEntry(JsValue::from(&value).as_object().unwrap().clone()));
        let actual = match truncate_tool_result_for_persistence(&entry) {
            Ok(_) => json!({"ok":true}),
            Err(error) => json!({"ok":false,"error":error.value().to_serde().unwrap()}),
        };
        if actual != expected["output"] {
            mismatches.push(format!("{actual} != {}", expected["output"]));
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

#[test]
fn buffer_like_tool_identity_is_explicit_approved_gap_not_silent_omission() {
    use pi_coding_agent::core::session_manager::{SessionEntry, UnknownSessionEntry};
    use pi_coding_agent::core::session_tool_output::truncate_tool_result_for_persistence;
    use pi_js_compat::js_json::JsValue;
    // Parent ruling2026-10-25: no single native String conversion; reject explicitly.
    let value = json!({"type":"message","id":"raw","parentId":null,"timestamp":"fixture",
        "oversizedEnvelope":"x".repeat(1_100_000),
        "message":{"role":"toolResult","toolCallId":[65,66],"toolName":"fixture",
        "content":[],"isError":false,"timestamp":1}});
    let mut ts = oracle::TsOracle::spawn("storage-tool-output-oracle.mts", "packages/coding-agent/src/core");
    let expected = ts.request(&json!({"id":1,"op":"truncate","entry":value}));
    assert_eq!(expected["output"]["ok"], true);
    assert_eq!(expected["output"]["entry"]["message"]["toolCallId"], json!([65, 66]));
    let entry = SessionEntry::Unknown(UnknownSessionEntry(JsValue::from(&value).as_object().unwrap().clone()));
    let error = truncate_tool_result_for_persistence(&entry).unwrap_err();
    assert_eq!(
        error.message,
        "Non-string Buffer-like tool identity is not supported at the persistence fallback"
    );
}

#[test]
fn null_record_serializer_and_migration_throw_before_writing() {
    use pi_coding_agent::core::session_manager::parse_session_entries;
    use pi_coding_agent::core::session_tool_output::serialize_session_entry_for_persistence;
    // Frozen session-tool-output.ts:217-219,355-357 access entry.type directly.
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("null-record.jsonl");
    let original = "{\"type\":\"session\",\"id\":\"fixture\"}\nnull\n";
    std::fs::write(&path, original).unwrap();
    let mut ts = oracle::TsOracle::spawn("storage-tool-output-oracle.mts", "packages/coding-agent/src/core");
    let expected = ts.request(&json!({"id":1,"op":"serialize","entry":null}));
    assert_eq!(expected["output"]["ok"], false);
    let actual = match serialize_session_entry_for_persistence(&parse_session_entries("null").remove(0)) {
        Ok(line) => json!({"ok":true,"line":line}),
        Err(error) => json!({"ok":false,"error":error.value().to_serde().unwrap()}),
    };
    let source_report = ts.request(&json!({"id":2,"op":"migrate","dir":tmp.path()}));
    assert_eq!(source_report["output"]["skippedErrorFiles"], 1);
    let report =
        migrate_tool_result_session_files(&tmp.path().to_str().unwrap().into(), || Ok(1792584000000.0)).unwrap();
    let expected_pair = json!([expected["output"], source_report["output"]]);
    let actual_pair = json!([actual, report_value(report)]);
    assert_eq!(actual_pair, expected_pair);
    assert_eq!(std::fs::read_to_string(path).unwrap(), original);
}

#[test]
fn truncation_candidates_match_frozen_wire_and_changed_detection() {
    use pi_coding_agent::core::session_manager::{SessionEntry, UnknownSessionEntry};
    use pi_coding_agent::core::session_tool_output::truncate_tool_result_for_persistence;
    use pi_js_compat::js_json::JsValue;
    let base = json!({"type":"message","id":"raw","parentId":null,"timestamp":"fixture",
        "extra":{"2":2,"1":1},"message":{"role":"toolResult","toolCallId":"fixture","toolName":"fixture",
        "content":[{"type":"text","text":"small"}],"isError":false,"timestamp":1}});
    let mut fixtures = vec![("small", base.clone())];
    for (name, content) in [
        (
            "multiblock-cap",
            json!([{"type":"text","text":"漢😀".repeat(200000),"extra":"keep"},{"type":"text","text":"tail"},{"type":"image","data":"small"}]),
        ),
        ("json-byte-cap", json!([{"type":"text","text":"\"\n\\".repeat(200000)}])),
        (
            "nontext-serialized-cap",
            json!([{"type":"image","data":"x".repeat(1_100_000)}]),
        ),
        (
            "nontext-and-text",
            json!([{"type":"other","data":"x".repeat(1_100_000)},{"type":"text","text":"retained text"}]),
        ),
    ] {
        let mut value = base.clone();
        value["message"]["content"] = content;
        fixtures.push((name, value));
    }
    let mut details = base.clone();
    details["message"]["details"] = json!("x".repeat(1_100_000));
    fixtures.push(("drop-details", details));
    let mut minimal = base.clone();
    minimal["oversizedEnvelope"] = json!("x".repeat(1_100_000));
    minimal["message"]["toolCallId"] = json!("😀".repeat(100));
    minimal["message"]["toolName"] = json!("漢".repeat(100));
    fixtures.push(("minimal", minimal));
    let mut ts = oracle::TsOracle::spawn("storage-tool-output-oracle.mts", "packages/coding-agent/src/core");
    for (name, value) in fixtures {
        let entry = SessionEntry::Unknown(UnknownSessionEntry(JsValue::from(&value).as_object().unwrap().clone()));
        let expected = ts.request(&json!({"id":1,"op":"truncateWire","entry":value}));
        assert_eq!(expected["output"]["ok"], true, "{name}: {expected}");
        let (native, changed) = truncate_tool_result_for_persistence(&entry).unwrap();
        assert_eq!(changed, expected["output"]["changed"], "{name}");
        assert_eq!(
            serde_json::to_string(&native).unwrap(),
            expected["output"]["wire"].as_str().unwrap(),
            "{name}"
        );
    }
}

#[test]
fn throwing_clock_and_invalid_date_are_caught_for_each_migration_file() {
    use pi_js_compat::callback_error::CallbackError;
    use pi_js_compat::node_error::{NativeErrorClass, NativeNodeError};
    let tmp = tempfile::tempdir().unwrap();
    let mut ts = oracle::TsOracle::spawn("storage-tool-output-oracle.mts", "packages/coding-agent/src/core");
    let created = ts.request(&json!({"id":1,"op":"create","dir":tmp.path(),"text":"x".repeat(1_100_000)}));
    let path = created["output"]["path"].as_str().unwrap();
    std::fs::copy(path, tmp.path().join("second.jsonl")).unwrap();
    let original = std::fs::read(path).unwrap();
    for case in ["error", "null", "invalid-date"] {
        let mut request = json!({"id":2,"op":"migrate","dir":tmp.path()});
        match case {
            "error" => request["throwError"] = json!(true),
            "null" => request["throwValue"] = Value::Null,
            _ => request["invalidDate"] = json!(true),
        }
        let expected = ts.request(&request);
        assert_eq!(expected["output"]["skippedErrorFiles"], 2);
        let actual = migrate_tool_result_session_files(&tmp.path().to_str().unwrap().into(), || match case {
            "error" => Err(CallbackError::from(NativeNodeError::from_constructor(
                NativeErrorClass::TypeError,
                "clock callback failed",
            ))),
            "null" => Err(CallbackError::from_rejection(pi_js_compat::abort::AbortReason::Value(
                pi_js_compat::js_json::JsValue::Null,
            ))),
            _ => Ok(f64::NAN),
        })
        .unwrap();
        assert_eq!(report_value(actual), expected["output"], "{case}");
        assert_eq!(std::fs::read(path).unwrap(), original);
    }
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
    let actual =
        migrate_tool_result_session_files(&tmp.path().to_str().unwrap().into(), || Ok(1792584000000.0)).unwrap();
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
        Ok(1792584000000.0)
    })
    .unwrap();
    assert_eq!(report_value(actual), expected["output"]);
    assert_eq!(std::fs::read(path).unwrap(), original);
}
