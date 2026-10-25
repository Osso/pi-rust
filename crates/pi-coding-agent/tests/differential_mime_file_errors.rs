use crate::shared_oracle as oracle;
use pi_coding_agent::utils::mime::detect_supported_image_mime_type_from_file;
use pi_js_compat::js_json as json;
use pi_js_compat::node_error::NativeNodeError;
use pi_js_compat::node_fs::node_file_read_error_record;

#[tokio::test]
async fn mime_open_and_file_handle_read_errors_keep_node_metadata() {
    use std::os::unix::fs::PermissionsExt;
    let cwd = tempfile::tempdir().unwrap();
    let blocked = cwd.path().join("write-only.txt");
    std::fs::write(&blocked, "fixture").unwrap();
    std::fs::set_permissions(&blocked, std::fs::Permissions::from_mode(0o200)).unwrap();
    let mut oracle = oracle::TsOracle::spawn(
        "mime-file-error-records-oracle.ts",
        "packages/coding-agent/src/utils/mime.ts",
    );
    for path in [
        cwd.path().to_owned(),
        blocked,
        cwd.path().join("missing"),
        cwd.path().join("nul\0name"),
    ] {
        let expected = oracle.request(&json!({"id": 48, "path": path.to_str().unwrap()}));
        assert_eq!(expected["ok"], json!(true), "{expected:?}");
        let actual = detect_supported_image_mime_type_from_file(&path).await;
        let actual = match actual {
            Ok(mime) => json!({"mime": mime}),
            Err(error) => json!({"error": error.value()}),
        };
        assert_eq!(actual, expected["output"]);
    }
    let expected = oracle.request(&json!({"id": 49, "operation": "pipe-read"}));
    assert_eq!(expected["ok"], json!(true), "{expected:?}");
    let errno = expected["output"]["error"]["errno"].as_f64().unwrap();
    let error = std::io::Error::from_raw_os_error((-errno) as i32);
    let actual = NativeNodeError::from(node_file_read_error_record(&error));
    assert_eq!(actual.value(), expected["output"]["error"]);
}
