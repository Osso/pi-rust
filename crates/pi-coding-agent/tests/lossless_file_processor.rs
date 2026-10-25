use crate::shared_ai_oracle as oracle;

use pi_coding_agent::cli::file_processor::process_file_arguments;
use pi_js_compat::js_json::{JsString, encode_node_utf8};

#[tokio::test]
async fn file_attachment_keeps_original_absolute_path_units() {
    let directory = tempfile::tempdir().unwrap();
    let cwd = JsString::from(directory.path().to_str().unwrap());
    let path = cwd.clone() + "/file-" + JsString::from_utf16(vec![0xd800]) + ".txt";
    std::fs::write(encode_node_utf8(&path), "hello\n").unwrap();
    let actual = process_file_arguments(std::slice::from_ref(&path), None, &cwd)
        .await
        .unwrap();
    let mut source = oracle::TsOracle::spawn(
        "lossless-tool-paths-oracle.ts",
        "packages/coding-agent/src/cli/file-processor.ts",
    );
    let empty = source.request(&serde_json::json!({"kind":"fileAttachment","paths":[]}));
    assert_eq!(empty, serde_json::json!({"text":"","images":[]}));
    assert_eq!(
        process_file_arguments(&[] as &[JsString], None, &cwd)
            .await
            .unwrap()
            .text,
        JsString::default()
    );
    let expected = source.request(&pi_js_compat::js_json!({"kind":"fileAttachment","paths":[path]}));
    assert_eq!(&actual.text, expected["text"].as_string().unwrap());
    assert!(actual.images.is_empty());
}
