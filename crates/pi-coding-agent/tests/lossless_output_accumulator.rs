use pi_coding_agent::core::tools::output_accumulator::{OutputAccumulator, OutputAccumulatorOptions, SnapshotOptions};
use pi_js_compat::js_json::{JsString, encode_node_utf8};

#[test]
fn output_snapshot_keeps_raw_artifact_path_until_file_creation() {
    let directory = tempfile::tempdir().unwrap();
    let path =
        JsString::from(directory.path().to_str().unwrap()) + "/output-" + JsString::from_utf16(vec![0xd800]) + ".log";
    let mut output = OutputAccumulator::new_with_temp_file_path(
        OutputAccumulatorOptions {
            max_bytes: 2,
            ..OutputAccumulatorOptions::default()
        },
        &path,
    );
    output.append(b"first\nlast\n");
    output.finish();
    let snapshot = output.snapshot(SnapshotOptions {
        persist_if_truncated: true,
    });
    output.close_temp_file().unwrap();
    assert_eq!(snapshot.full_output_path, Some(path.clone()));
    assert_eq!(std::fs::read(encode_node_utf8(&path)).unwrap(), b"first\nlast\n");
    assert_eq!(snapshot.content, snapshot.truncation.content);
}
