//! Evidence-only frozen source discrepancy; not authorization to preserve or fix Rust behavior.
use crate::shared_oracle as oracle;
use serde_json::{Value, json};
#[test]
fn source_all_sync_failures_after_rotation_returns_memory_only_success() {
    let file = std::env::temp_dir().join(format!("auth-source-discrepancy-{}.json", std::process::id()));
    std::fs::write(&file,json!({"phase1-rotation":{"type":"oauth","refresh":"A","access":"old","expires":0},"other":{"type":"api_key","key":"unrelated"}}).to_string()).unwrap();
    let mut oracle = oracle::TsOracle::spawn(
        "auth-storage-oracle.ts",
        "packages/coding-agent/src/core/auth-storage.ts",
    );
    let proof = oracle.request(&json!({"id":1,"op":"rotation-all-sync-fail","authPath":file}));
    println!("SOURCE_DISCREPANCY {proof}");
    assert_eq!(proof["ok"], true, "{proof}");
    let out = &proof["output"];
    assert_eq!(out["result"], "new");
    assert_eq!(out["memory"]["phase1-rotation"]["refresh"], "B");
    let disk: Value = serde_json::from_str(out["file"].as_str().unwrap()).unwrap();
    assert_eq!(disk["phase1-rotation"]["refresh"], "A");
    assert_eq!(
        out["calls"],
        json!([
            "sync 1",
            "async callback",
            "refresh A->B",
            "after callback: write denied",
            "sync 2",
            "sync 3"
        ])
    );
    assert_eq!(
        out["errors"],
        json!(["locked write denied", "sync denied", "sync denied", "sync denied"])
    );
}
