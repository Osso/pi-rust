use crate::shared_oracle as oracle;

use pi_coding_agent::core::tools::node_fs::{node_fs_error, node_read_error};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::JsString;
use std::fs;

#[test]
fn directory_read_message_matches_real_node() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().to_str().expect("utf-8 test path");
    let error = fs::read(path).unwrap_err();
    let mut node = oracle::TsOracle::spawn("node-fs-error-oracle.ts", "packages/coding-agent/package.json");
    let expected = node.request(&json!({"operation":"read", "path":path}));
    assert_eq!(
        node_read_error(&error, &JsString::from(path)),
        *expected["message"].as_string().unwrap()
    );
}

#[test]
fn symlink_loop_read_message_matches_real_node() {
    let dir = tempfile::tempdir().unwrap();
    let link = dir.path().join("loop");
    std::os::unix::fs::symlink("loop", &link).unwrap();
    let path = link.to_str().expect("utf-8 test path");
    let error = fs::read(path).unwrap_err();
    let mut node = oracle::TsOracle::spawn("node-fs-error-oracle.ts", "packages/coding-agent/package.json");
    let expected = node.request(&json!({"operation":"read", "path":path}));
    assert_eq!(
        node_read_error(&error, &JsString::from(path)),
        *expected["message"].as_string().unwrap()
    );
}

#[test]
fn directory_write_message_matches_real_node() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().to_str().expect("utf-8 test path");
    let error = fs::write(path, "fixture").unwrap_err();
    let mut node = oracle::TsOracle::spawn("node-fs-error-oracle.ts", "packages/coding-agent/package.json");
    let expected = node.request(&json!({"operation":"write", "path":path}));
    assert_eq!(
        node_fs_error(&error, "open", &JsString::from(path)),
        *expected["message"].as_string().unwrap()
    );
}
