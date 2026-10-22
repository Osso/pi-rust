#[path = "../../tests/support/oracle.rs"]
mod oracle;
use super::*;
use serde_json::{Value, json};

#[test]
fn writable_metadata_dates_match_frozen_iso_and_invalid_date_errors() {
    let mut oracle = oracle::TsOracle::spawn("session-info-dates-oracle.ts", "packages/coding-agent/src");
    let rows: Vec<_> = [
        ("0", "1000.9"),
        ("NaN", "0"),
        ("0", "Infinity"),
        ("-8640000000000000", "8640000000000000"),
        ("8640000000000001", "0"),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (created, modified))| json!({"id":format!("date{index}"),"created":created,"modified":modified}))
    .collect();
    let response = oracle.request(&json!({"id":7,"operation":"commandMetadata","rows":rows}));
    assert_eq!(response["ok"], true, "{response}");
    let actual: Vec<Value> = rows
        .iter()
        .map(|row| {
            let time = |field: &str| {
                pi_js_compat::date::time_clip_millis(row[field].as_str().unwrap().parse().unwrap())
                    .map_or(f64::NAN, |value| value as f64)
            };
            let session = SessionInfo {
                path: format!("/tmp/{}.jsonl", row["id"].as_str().unwrap()).into(),
                id: row["id"].as_str().unwrap().into(),
                cwd: "/work".into(),
                name: None,
                is_archived: None,
                parent_session_path: None,
                created: time("created"),
                modified: time("modified"),
                message_count: 1,
                first_message: "chair".into(),
            };
            match writable_metadata(&session) {
                Ok(metadata) => json!({"ok":true,"value":metadata}),
                Err(error) => json!({"ok":false,"name":error.name(),"message":error.to_string()}),
            }
        })
        .collect();
    assert_eq!(json!(actual), response["output"]);
}
