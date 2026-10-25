#![cfg(unix)]
use crate::shared_oracle as oracle;

use pi_agent_core::agent::AgentTool;
use pi_coding_agent::core::tools::edit::EditTool;
use pi_coding_agent::utils::paths::node_join;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsString, JsValue, encode_node_utf8};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn file_case(bytes: &[u8], old: impl Into<JsString>, new: impl Into<JsString>) -> JsValue {
    json!({"path":"file.txt","edits":[{"oldText":old.into(),"newText":new.into()}],
        "fixture":{"path":"file.txt","bytes":bytes}})
}
fn create_fixture(root: &Path, input: &JsValue) {
    if let Some(fixture) = input.get("fixture") {
        let path = PathBuf::from(encode_node_utf8(&node_join(
            JsString::from(root.to_str().unwrap()),
            fixture["path"].as_string().unwrap(),
        )));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let bytes: Vec<u8> = fixture["bytes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| u8::try_from(n.as_u64().unwrap()).unwrap())
            .collect();
        std::fs::write(&path, bytes).unwrap();
        if let Some(mode) = fixture.get("mode") {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode.as_u64().unwrap() as u32)).unwrap();
        }
    }
    match input["setup"].as_string().map(|s| s.to_utf8().unwrap()).as_deref() {
        Some("directory") => std::fs::create_dir(root.join("folder")).unwrap(),
        Some("parent-file") => std::fs::write(root.join("leaf"), "untouched").unwrap(),
        Some("symlink-file") => std::os::unix::fs::symlink("file.txt", root.join("alias.txt")).unwrap(),
        Some("symlink-loop") => std::os::unix::fs::symlink("loop.txt", root.join("loop.txt")).unwrap(),
        None => {}
        Some(other) => panic!("unknown fixture {other}"),
    }
}
fn probe_file(root: &Path, input: &JsValue) -> (Option<JsString>, Option<u32>) {
    if input.get("probe") == Some(&JsValue::Null) {
        return (None, None);
    }
    let relative = input
        .get("probe")
        .and_then(JsValue::as_string)
        .unwrap_or_else(|| input["path"].as_string().unwrap());
    let absolute = node_join(JsString::from(root.to_str().unwrap()), relative);
    let path = PathBuf::from(encode_node_utf8(&absolute));
    match std::fs::read(&path) {
        Ok(bytes) => (
            Some(String::from_utf8_lossy(&bytes).as_ref().into()),
            Some(std::fs::metadata(path).unwrap().permissions().mode() & 0o777),
        ),
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound
                    | std::io::ErrorKind::NotADirectory
                    | std::io::ErrorKind::IsADirectory
                    | std::io::ErrorKind::PermissionDenied
                    | std::io::ErrorKind::InvalidInput
            ) =>
        {
            (None, None)
        }
        Err(error) => panic!("probe: {error}"),
    }
}

#[tokio::test]
async fn default_edit_filesystem_effects_and_failures_match_frozen() {
    let mut source = oracle::TsOracle::spawn(
        "edit-filesystem-oracle.ts",
        "packages/coding-agent/src/core/tools/edit.ts",
    );
    let mut cases = vec![
        file_case(b"before\nkeep\n", "before", "after"),
        file_case(b"\xef\xbb\xbfbefore\r\nkeep\r\n", "before", "after\nmore"),
        file_case(b"before\rkeep\r", "before", "after"),
        file_case(b"before\xff\xed\xa0\x80", "before", "after"),
        file_case("🐱".as_bytes(), JsString::from_utf16(vec![0xdc31]), "x"),
        file_case(b"before", "before", JsString::from_utf16(vec![0xd800])),
        file_case(b"before", "before", "after\0body"),
        file_case(b"before", "missing", "after"),
        file_case(b"before\nbefore", "before", "after"),
        file_case(b"before", "before", "before"),
        json!({"path":"missing.txt","edits":[{"oldText":"before","newText":"after"}]}),
        json!({"setup":"directory","path":"folder","edits":[{"oldText":"before","newText":"after"}]}),
        json!({"setup":"parent-file","path":"leaf/child.txt","edits":[{"oldText":"before","newText":"after"}]}),
        json!({"setup":"symlink-loop","path":"loop.txt","edits":[{"oldText":"before","newText":"after"}],"probe":null}),
        json!({"path":"null\0file.txt","edits":[{"oldText":"before","newText":"after"}],"probe":null}),
        json!({"path":"null\0file.txt","edits":[],"probe":null}),
        json!({"path":"","edits":[{"oldText":"before","newText":"after"}],"probe":null}),
    ];
    for mode in [0o444, 0] {
        let mut case = file_case(b"before", "before", "after");
        case["fixture"]
            .as_object_mut()
            .unwrap()
            .insert("mode".into(), json!(mode));
        cases.push(case);
    }
    let mut linked = file_case(b"before", "before", "after");
    linked
        .as_object_mut()
        .unwrap()
        .insert("setup".into(), json!("symlink-file"));
    linked
        .as_object_mut()
        .unwrap()
        .insert("path".into(), json!("alias.txt"));
    cases.push(linked);
    let raw = JsString::from("raw-") + JsString::from_utf16(vec![0xd800]) + ".txt";
    cases.push(json!({"path":raw,"edits":[{"oldText":"before","newText":"after"}],"fixture":{"path":raw,"bytes":b"before".to_vec()}}));
    let mut failures = Vec::new();
    for (index, input) in cases.into_iter().enumerate() {
        let source_root = tempfile::tempdir().unwrap();
        let native_root = tempfile::tempdir().unwrap();
        let mut request = input.clone();
        request
            .as_object_mut()
            .unwrap()
            .insert("root".into(), json!(source_root.path()));
        let expected: JsValue = source.request(&json!({"id":index,"cases":[request]}));
        assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
        create_fixture(native_root.path(), &input);
        let root = JsString::from(native_root.path().to_str().unwrap());
        let result = EditTool::new(&root)
            .execute("edit-fs".into(), input.clone(), None, Arc::new(|_| Ok(())), None)
            .await;
        let result = match result {
            Ok(value) => json!({"ok":true,"value":js_json::to_value(&value).unwrap()}),
            Err(error) => json!({"ok":false,"error":error.message.replace(&root,"ROOT")}),
        };
        let (content, mode) = probe_file(native_root.path(), &input);
        let mut entries: Vec<_> = std::fs::read_dir(native_root.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        entries.sort();
        let actual = json!({"result":result,"content":content,"mode":mode,"entries":entries});
        if actual != expected["output"][0] {
            failures.push(format!(
                "case {index}: {} != {}",
                js_json::stringify_lossless(&actual),
                js_json::stringify_lossless(&expected["output"][0])
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
