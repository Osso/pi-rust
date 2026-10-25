use crate::shared_oracle as oracle;

use pi_js_compat::js_json as json;

#[test]
fn frozen_stat_dependency_errors_preserve_all_original_fields_without_relocation() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("file");
    std::fs::write(&file, b"file").unwrap();
    let mut oracle = oracle::TsOracle::spawn(
        "change-cwd-stat-fault-oracle.ts",
        "packages/coding-agent/src/core/tools/change-working-directory.ts",
    );
    for (path, code, errno, description) in [
        (temp.path().join("missing"), "ENOENT", -2, "no such file or directory"),
        (file.join("child"), "ENOTDIR", -20, "not a directory"),
    ] {
        let path = pi_js_compat::node_fs::decode_node_path(path.as_os_str());
        let response = oracle.request(&json!({"id": 1, "cwd": temp.path(), "path": path}));
        assert_eq!(response["ok"], json!(true), "{response:?}");
        let message = pi_js_compat::js_format!("{}: {}, stat '{}'", code, description, path);
        assert_eq!(
            response["output"],
            json!({"ok": false, "error": {
            "name": "Error", "message": message, "code": code, "errno": errno, "syscall": "stat", "path": path
        }, "relocations": []})
        );
    }
}
