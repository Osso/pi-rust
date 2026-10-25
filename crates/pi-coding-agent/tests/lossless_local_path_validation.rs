//! Local Operations validate Node paths before OS encoding or syscall entry.
use crate::shared_ai_oracle as oracle;

use pi_coding_agent::core::tools::{
    edit::{EditOperations, LocalEditOperations},
    grep::{GrepOperations, LocalGrepOperations},
    ls::{LocalLsOperations, LsOperations},
    node_fs::operation_error_message,
    read::{LocalReadOperations, ReadOperations},
    write::{LocalWriteOperations, WriteOperations},
};
use pi_js_compat::js_json::JsString;

#[tokio::test]
async fn local_filesystem_operations_reject_raw_nul_paths_like_node_before_io() {
    let directory = tempfile::tempdir().unwrap();
    let marker = directory.path().join("untouched");
    std::fs::write(&marker, b"original").unwrap();
    let root = JsString::from(directory.path().to_str().unwrap());
    let mut source = oracle::TsOracle::spawn(
        "node-fs-error-oracle.ts",
        "packages/coding-agent/src/core/tools/read.ts",
    );
    for suffix in [
        JsString::from_utf16(vec![0xd800, 0, 0xdfff]),
        JsString::from_utf16(vec![0xd800, 0]) + "x".repeat(160) + JsString::from_utf16(vec![0xdfff]),
    ] {
        let path = root.clone() + "/invalid-" + suffix;
        let expected = source.request(&pi_js_compat::js_json!({"operation":"readAsync","path":path}));
        assert_eq!(expected["name"], pi_js_compat::js_json!("TypeError"));
        assert_eq!(expected["own"]["code"], pi_js_compat::js_json!("ERR_INVALID_ARG_VALUE"));
        let content = JsString::from("must not be written");
        let errors = [
            (
                "read.read_file",
                LocalReadOperations.read_file(&path).await.unwrap_err(),
            ),
            (
                "read.read_range",
                LocalReadOperations.read_range().unwrap()(&path, 0.0, 1.0)
                    .await
                    .unwrap_err(),
            ),
            ("read.stat", LocalReadOperations.stat(&path).await.unwrap_err()),
            ("read.access", LocalReadOperations.access(&path).await.unwrap_err()),
            (
                "write.write_file",
                LocalWriteOperations.write_file(&path, &content).await.unwrap_err(),
            ),
            ("write.mkdir", LocalWriteOperations.mkdir(&path).await.unwrap_err()),
            (
                "edit.read_file",
                LocalEditOperations.read_file(&path).await.unwrap_err(),
            ),
            (
                "edit.write_file",
                LocalEditOperations.write_file(&path, &content).await.unwrap_err(),
            ),
            ("edit.access", LocalEditOperations.access(&path).await.unwrap_err()),
            (
                "ls.is_directory",
                LocalLsOperations.is_directory(&path).await.unwrap_err(),
            ),
            ("ls.readdir", LocalLsOperations.readdir(&path).await.unwrap_err()),
            (
                "grep.is_directory",
                LocalGrepOperations.is_directory(&path).await.unwrap_err(),
            ),
            (
                "grep.read_file",
                LocalGrepOperations.read_file(&path).await.unwrap_err(),
            ),
        ];
        for (operation, error) in errors {
            assert_eq!(
                operation_error_message(&error),
                expected["message"].as_string().unwrap().clone(),
                "{operation}"
            );
            assert_eq!(
                error.raw_os_error(),
                None,
                "{operation}: argument validation is not an errno"
            );
        }
        assert_eq!(std::fs::read(&marker).unwrap(), b"original");
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }
}
