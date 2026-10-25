#[path = "support/oracle.rs"]
mod oracle;

use pi_coding_agent::bundled_extensions::pyrun::detached_output::PyrunArtifactWriter;
use pi_coding_agent::bundled_extensions::pyrun::runner::CanonicalPyrunProgressUpdate;
use pi_js_compat::{js_json as json, js_json::JsValue};
use sha2::{Digest, Sha256};

#[test]
fn pyrun_capped_artifacts_match_frozen_bytes_and_preserve_live_offsets() {
    let cases = json!([
        {"count":3,"size":10,"terminalSize":5},
        {"count":56,"size":1048576,"terminalSize":5},
        {"count":1,"size":10,"terminalSize":53477376}
    ]);
    let mut oracle = oracle::TsOracle::spawn(
        "pyrun-artifacts-oracle.ts",
        "packages/coding-agent/extensions/pyrun/src/detached-output.ts",
    );
    let expected = oracle.request(&json!({"id":1,"cases":cases}));
    assert_eq!(expected["ok"], json!(true), "{expected:?}");
    for (index, case) in cases.as_array().unwrap().iter().enumerate() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("output");
        let mut writer = PyrunArtifactWriter::new(path.to_str().unwrap().into());
        for record in 0..case["count"].as_u64().unwrap() {
            writer.append_progress(&CanonicalPyrunProgressUpdate { raw: json!({"type":"console","text":"x".repeat(case["size"].as_u64().unwrap() as usize),"index":record}) }).unwrap();
        }
        let head = if path.exists() {
            std::fs::read(&path).unwrap()
        } else {
            Vec::new()
        };
        writer
            .finish(&json!({"kind":"error","error":"e".repeat(case["terminalSize"].as_u64().unwrap() as usize)}))
            .unwrap();
        let bytes = std::fs::read(path).unwrap();
        let actual: JsValue = json!({"headBytes":head.len(),"bytes":bytes.len(),"hash":format!("{:x}", Sha256::digest(&bytes)),"appendOnly":bytes.starts_with(&head)});
        assert_eq!(actual, expected["output"][index], "case {index}");
    }
}
