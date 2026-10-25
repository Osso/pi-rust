use crate::shared_oracle as oracle;

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

#[tokio::test(start_paused = true)]
async fn pyrun_progress_batches_across_reads_and_flushes_like_frozen() {
    use pi_coding_agent::bundled_extensions::pyrun::detached_progress::ArtifactProgressAccumulator;
    use std::sync::{Arc, Mutex};
    let console =
        |text: &str, stream: &str| json!({"kind":"progress","update":{"type":"console","text":text,"stream":stream}});
    let dense: Vec<_> = (0..60).map(|_| console("x", "stdout")).collect();
    let cases = json!([
        [{"records":[console("a","stdout")]},{"flush":true}],
        [{"records":dense},{"records":dense},{"flush":true}],
        [{"records":[console("a","stdout"),console("b","stderr"),{"kind":"progress","update":{"type":"status","message":"working"}}]}],
        [{"records":[console("a","stdout"),{"kind":"truncation","message":"omitted","omittedBytes":10,"omittedRecords":1}]}],
        [{"records":[console("a","stdout")]},{"close":true},{"flush":true}],
        [{"records":[console("a","stdout"),{"kind":"result","result":{"ok":true,"console":[],"value":42}}]}]
    ]);
    let mut oracle = oracle::TsOracle::spawn(
        "pyrun-progress-oracle.ts",
        "packages/coding-agent/extensions/pyrun/src/detached-progress.ts",
    );
    let expected = oracle.request(&json!({"id":1,"cases":cases}));
    assert_eq!(expected["ok"], json!(true), "{expected:?}");
    for (index, steps) in cases.as_array().unwrap().iter().enumerate() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let capture = events.clone();
        let mut accumulator = ArtifactProgressAccumulator::new(
            Arc::new(move |update| {
                capture.lock().unwrap().push(update.raw);
                Ok(())
            }),
            25,
        );
        let mut snapshots = Vec::new();
        for step in steps.as_array().unwrap() {
            let result = match step["records"].as_array() {
                Some(records) => accumulator.consume(records).unwrap(),
                None => None,
            };
            tokio::task::yield_now().await;
            if step["flush"].as_bool() == Some(true) {
                tokio::time::advance(std::time::Duration::from_millis(25)).await;
                tokio::task::yield_now().await;
            }
            if step["close"].as_bool() == Some(true) {
                accumulator.close();
            }
            snapshots.push(json!({"events":events.lock().unwrap().clone(),"result":result}));
        }
        accumulator.close();
        assert_eq!(json!(snapshots), expected["output"][index], "case {index}");
    }
}
