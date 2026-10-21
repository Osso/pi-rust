use pi_coding_agent::core::detached_bash_runner::{append_artifact_record, hash_json};

#[test]
fn artifact_hashes_and_lines_use_javascript_json_bytes() {
    let value = serde_json::from_str(r#"{"2":2.0,"1":1.0}"#).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("output.jsonl");
    append_artifact_record(output.to_str().unwrap(), &value).unwrap();
    assert_eq!(std::fs::read_to_string(output).unwrap(), "{\"1\":1,\"2\":2}\n");
    assert_eq!(
        hash_json(&value),
        "c7d5aab3a2da4e00e26472c62c4a41e00b32b539cd7d5d7df0c80c20673be456"
    );
}
