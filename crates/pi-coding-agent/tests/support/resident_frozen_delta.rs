//! Exact frozen-baseline difference contract for approved TS delta 8f8f3c26.

use serde_json::{Value, json};

pub(super) fn expected_discrepancies(matrix: &str) -> Vec<Value> {
    let record: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/porting/deltas/resident-session-frozen-discrepancies.json"
    )))
    .expect("approved resident discrepancy record must be valid JSON");
    record["matrices"][matrix]
        .as_array()
        .expect("matrix must have an explicit discrepancy set")
        .clone()
}

pub(super) fn snapshot_difference(case: &str, source: &Value, native: &Value) -> Result<Option<Value>, String> {
    let mut source = source.clone();
    let mut native = native.clone();
    let source_rows = source["observed"]["metadata"].take();
    let native_rows = native["observed"]["metadata"].take();
    if source != native {
        return Err(format!(
            "{case}: nonmetadata snapshot differs: source={source} native={native}"
        ));
    }
    let source_rows = source_rows.as_array().expect("source metadata must be an array");
    let native_rows = native_rows.as_array().expect("native metadata must be an array");
    let source_only: Vec<_> = source_rows.iter().filter(|row| !native_rows.contains(row)).collect();
    let native_only: Vec<_> = native_rows.iter().filter(|row| !source_rows.contains(row)).collect();
    let source_common: Vec<_> = source_rows.iter().filter(|row| native_rows.contains(row)).collect();
    let native_common: Vec<_> = native_rows.iter().filter(|row| source_rows.contains(row)).collect();
    if source_common != native_common {
        return Err(format!("{case}: common metadata order/count differs"));
    }
    if source_only.is_empty() && native_only.is_empty() {
        return Ok(None);
    }
    Ok(Some(
        json!({"case":case,"sourceOnly":source_only,"nativeOnly":native_only}),
    ))
}

pub(super) fn verify_discrepancies(actual: &[Value], expected: &[Value]) -> Result<(), String> {
    let mut actual = actual.to_vec();
    let mut expected = expected.to_vec();
    actual.sort_by(|a, b| a["case"].as_str().cmp(&b["case"].as_str()));
    expected.sort_by(|a, b| a["case"].as_str().cmp(&b["case"].as_str()));
    if actual != expected {
        return Err(format!(
            "approved delta 8f8f3c26 discrepancy set differs: expected={expected:?} actual={actual:?}"
        ));
    }
    Ok(())
}

#[test]
fn approved_frozen_difference_guard_rejects_missing_difference() {
    assert!(verify_discrepancies(&[], &[backslash_difference()]).is_err());
}

#[test]
fn approved_frozen_difference_guard_rejects_extra_difference() {
    assert!(verify_discrepancies(&[backslash_difference()], &[]).is_err());
}

#[test]
fn approved_frozen_difference_guard_rejects_altered_row() {
    let expected = backslash_difference();
    let mut actual = expected.clone();
    actual["sourceOnly"][0]["id"] = json!("other-role");
    assert!(verify_discrepancies(&[actual], &[expected]).is_err());
}

#[test]
fn approved_frozen_difference_guard_preserves_exact_difference() {
    let mut source = snapshot();
    let native = snapshot();
    source["observed"]["metadata"]
        .as_array_mut()
        .unwrap()
        .push(backslash_difference()["sourceOnly"][0].clone());
    assert_eq!(
        snapshot_difference("architect/ordinary", &source, &native).unwrap(),
        Some(backslash_difference())
    );
    assert!(verify_discrepancies(&[backslash_difference()], &[backslash_difference()]).is_ok());
}

#[test]
fn approved_frozen_difference_guard_rejects_unexpected_file_change() {
    let source = snapshot();
    let mut native = source.clone();
    native["observed"]["entries"] = json!(["unexpected.jsonl"]);
    assert!(snapshot_difference("architect/ordinary", &source, &native).is_err());
}

#[test]
fn approved_frozen_difference_guard_rejects_common_metadata_reordering() {
    let source = snapshot();
    let mut native = source.clone();
    native["observed"]["metadata"].as_array_mut().unwrap().reverse();
    assert!(snapshot_difference("architect/ordinary", &source, &native).is_err());
}

fn backslash_difference() -> Value {
    json!({"case":"architect/ordinary", "sourceOnly":[{"id":"backslash", "sessionPath":"ROOT\\architect-sessions\\missing.jsonl"}], "nativeOnly":[]})
}

fn snapshot() -> Value {
    json!({"prepared":{"ok":true,"value":{"sessionPath":"ROOT/agent/architect-sessions/900_architect.jsonl"}}, "observed":{"entries":["900_architect.jsonl"],"metadata":[{"id":"latest","sessionPath":"ROOT/agent/architect-sessions/900_architect.jsonl"},{"id":"ordinary","sessionPath":"ROOT/missing.jsonl"}]}})
}
