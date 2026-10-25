use crate::shared_oracle as oracle;

use std::fs;

use pi_coding_agent::core::external_session_importer::{ExternalSessionImportOptions, import_external_session_alias};
use pi_coding_agent::core::session_manager::build_session_info;
use pi_js_compat::date::time_clip_millis;
use pi_js_compat::js_json::JsString;
use serde_json::{Value, json};
use tempfile::TempDir;

#[test]
fn session_info_clips_fractional_message_time_only_at_date_construction() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("fractional.jsonl");
    let mut oracle = oracle::TsOracle::spawn(
        "timestamp-consumers-oracle.ts",
        "packages/coding-agent/src/core/session-manager.ts",
    );
    for timestamp in [0.75, 12.75, -12.75, 8_640_000_000_000_000.0] {
        let lines = [
            json!({"type":"session","version":3,"id":"fractional","timestamp":"2026-01-02T03:04:05.000Z","cwd":"/fixture"}),
            json!({"type":"message","id":"m","parentId":null,"timestamp":"2026-01-02T03:04:05.000Z",
                "message":{"role":"user","content":"hi","timestamp":timestamp}}),
        ];
        fs::write(
            &path,
            lines.iter().map(Value::to_string).collect::<Vec<_>>().join("\n") + "\n",
        )
        .unwrap();
        let expected = oracle.request(&json!({"operation":"info","path":path}));
        assert_eq!(expected["ok"], true, "{expected}");
        let actual = build_session_info(&JsString::from(path.to_str().unwrap())).unwrap();
        assert_eq!(
            json!({"created":pi_js_compat::number::js_number_value(actual.created),"modified":pi_js_compat::number::js_number_value(actual.modified)}),
            expected["output"]
        );
    }
}

#[test]
fn imported_message_and_entry_clocks_match_frozen_date_boundary() {
    let home = TempDir::new().unwrap();
    let native_sessions = TempDir::new().unwrap();
    let source_sessions = TempDir::new().unwrap();
    let source = home.path().join(".claude/projects/fixture/clock.jsonl");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    let mut oracle = oracle::TsOracle::spawn(
        "timestamp-consumers-oracle.ts",
        "packages/coding-agent/src/core/external-session-importer.ts",
    );
    for timestamp in ["1970-01-01T00:00:00.012750Z", "not a date"] {
        fs::write(
            &source,
            json!({"sessionId":"clock","cwd":"/fixture","timestamp":timestamp,
            "message":{"role":"user","content":"hi"}})
            .to_string()
                + "\n",
        )
        .unwrap();
        let expected = oracle.request(&json!({"operation":"import","alias":"claude/clock","options":{
            "homeDir":home.path(),"sessionDir":source_sessions.path(),"fallbackCwd":"/fixture"}}));
        let result = import_external_session_alias(
            &"claude/clock".into(),
            &ExternalSessionImportOptions {
                home_dir: Some(home.path().to_str().unwrap().into()),
                session_dir: Some(native_sessions.path().to_str().unwrap().into()),
                fallback_cwd: Some("/fixture".into()),
            },
        );
        if expected["ok"] == false {
            assert_eq!(result.unwrap_err().message(), expected["error"].as_str().unwrap());
        } else {
            let imported = result.unwrap().unwrap();
            let content = fs::read_to_string(imported.path.to_utf8().unwrap()).unwrap();
            let actual = content
                .lines()
                .skip(1)
                .map(|line| {
                    let entry: Value = serde_json::from_str(line).unwrap();
                    json!({"entry":entry["timestamp"],"message":entry["message"]["timestamp"]})
                })
                .collect::<Vec<_>>();
            assert_eq!(json!(actual), expected["output"]);
        }
        for directory in [&native_sessions, &source_sessions] {
            for file in fs::read_dir(directory.path()).unwrap() {
                fs::remove_file(file.unwrap().path()).unwrap();
            }
        }
    }
}

#[test]
fn time_clip_rejects_invalid_dates_without_narrowing_message_numbers() {
    for timestamp in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 8_640_000_000_000_001.0] {
        assert_eq!(time_clip_millis(timestamp), None);
    }
    assert_eq!(time_clip_millis(-12.75), Some(-12));
    assert_eq!(time_clip_millis(12.75), Some(12));
    assert_eq!(time_clip_millis(-0.0), Some(0));
}
