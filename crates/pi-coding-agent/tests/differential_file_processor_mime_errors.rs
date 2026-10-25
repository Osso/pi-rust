use crate::shared_oracle as oracle;
use pi_coding_agent::cli::file_processor::process_file_arguments;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::JsString;

#[tokio::test]
async fn file_processor_preserves_mime_open_and_read_failures() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let blocked = root.path().join("write-only.txt");
    std::fs::write(&blocked, "fixture").unwrap();
    std::fs::set_permissions(&blocked, std::fs::Permissions::from_mode(0o200)).unwrap();
    let mut oracle = oracle::TsOracle::spawn(
        "file-processor-mime-errors-oracle.ts",
        "packages/coding-agent/src/cli/file-processor.ts",
    );
    for path in [root.path(), blocked.as_path()] {
        let paths = vec![JsString::from(path.to_str().unwrap())];
        let expected = oracle.request(&json!({"id": 44, "paths": paths}));
        assert_eq!(expected["ok"], json!(true), "{expected:?}");
        let actual = process_file_arguments(&paths, None, &JsString::from(root.path().to_str().unwrap())).await;
        let actual = match actual {
            Ok(result) => json!({"result": {"text": result.text, "images": result.images}}),
            Err(error) => json!({"error": error.message()}),
        };
        assert_eq!(actual, expected["output"]);
    }
}
