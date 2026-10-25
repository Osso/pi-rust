#![cfg(unix)]
use crate::shared_oracle as oracle;

use pi_agent_core::agent::AgentTool;
use pi_coding_agent::core::tools::write::WriteTool;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsString, JsValue};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Arc;

fn create_filesystem_fixture(root: &Path, setup: Option<&JsString>) {
    let setup = setup.map(|value| value.to_utf8().unwrap());
    match setup.as_deref() {
        Some("directory") => std::fs::create_dir(root.join("folder")).unwrap(),
        Some("parent-file") => std::fs::write(root.join("leaf"), "untouched").unwrap(),
        Some("overwrite") => {
            let path = root.join("plain.txt");
            std::fs::write(&path, "longer untouched initial").unwrap();
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o640)).unwrap();
        }
        Some("readonly") => {
            let path = root.join("locked.txt");
            std::fs::write(&path, "untouched").unwrap();
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o444)).unwrap();
        }
        Some("symlink-parent") => {
            std::fs::create_dir(root.join("target")).unwrap();
            std::os::unix::fs::symlink("target", root.join("link")).unwrap();
        }
        Some("symlink-loop") => {
            std::os::unix::fs::symlink("loop.txt", root.join("loop.txt")).unwrap();
        }
        Some("symlink-file") => {
            std::fs::write(root.join("target.txt"), "untouched").unwrap();
            std::os::unix::fs::symlink("target.txt", root.join("link.txt")).unwrap();
        }
        None => {}
        Some(other) => panic!("unknown setup {other}"),
    }
}
fn read_probe(root: &Path, input: &JsValue) -> (Option<JsString>, Option<u32>) {
    if input.get("probe") == Some(&JsValue::Null) {
        return (None, None);
    }
    let probe = input
        .get("probe")
        .and_then(JsValue::as_string)
        .unwrap_or_else(|| input["path"].as_string().unwrap());
    let absolute = pi_coding_agent::utils::paths::node_join(JsString::from(root.to_str().unwrap()), probe);
    let path = std::path::PathBuf::from(js_json::encode_node_utf8(&absolute));
    match std::fs::read(&path) {
        Ok(bytes) => (
            Some(String::from_utf8(bytes).unwrap().into()),
            Some(std::fs::metadata(path).unwrap().permissions().mode() & 0o777),
        ),
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound
                    | std::io::ErrorKind::NotADirectory
                    | std::io::ErrorKind::IsADirectory
                    | std::io::ErrorKind::InvalidInput
            ) =>
        {
            (None, None)
        }
        Err(error) => panic!("probe: {error}"),
    }
}

#[tokio::test]
async fn local_write_bytes_and_failure_boundaries_match_frozen() {
    let mut source = oracle::TsOracle::spawn(
        "write-filesystem-oracle.ts",
        "packages/coding-agent/src/core/tools/write.ts",
    );
    let cases = [
        json!({"path":"nested/file.txt","content":JsString::from_utf16(vec![0x61,0xd800,0,0xdfff,0xd83d,0xdc31])}),
        json!({"path":"nested/empty.txt","content":""}),
        json!({"setup":"overwrite","path":"plain.txt","content":"new"}),
        json!({"path":"nested/raw-".to_owned()+"🦀.txt","content":"Unicode"}),
        json!({"path":"nested/../plain.txt","content":"normalized"}),
        json!({"path":"","content":"directory"}),
        json!({"setup":"directory","path":"folder","content":"directory"}),
        json!({"setup":"parent-file","path":"leaf/child.txt","content":"blocked"}),
        json!({"setup":"parent-file","path":"leaf/deep/child.txt","content":"blocked"}),
        json!({"setup":"readonly","path":"locked.txt","content":"blocked"}),
        json!({"setup":"symlink-parent","path":"link/file.txt","content":"linked"}),
        json!({"setup":"symlink-file","path":"link.txt","content":"replaced"}),
        json!({"setup":"symlink-loop","path":"loop.txt","content":"blocked","probe":null}),
        json!({"path":"nested/null\0file.txt","content":"blocked","probe":null}),
        json!({"path":"null\0dir/file.txt","content":"blocked","probe":null}),
        json!({"path":JsString::from("nested/lone-")+JsString::from_utf16(vec![0xd800])+".txt","content":"encoded path"}),
    ];
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
        create_filesystem_fixture(native_root.path(), input.get("setup").and_then(JsValue::as_string));
        let root = JsString::from(native_root.path().to_str().unwrap());
        let result = WriteTool::new(&root)
            .execute("write-fs".into(), input.clone(), None, Arc::new(|_| Ok(())), None)
            .await;
        let result = match result {
            Ok(result) => json!({"ok":true,"value":js_json::to_value(&result).unwrap()}),
            Err(error) => json!({"ok":false,"error":error.message.replace(&root, "ROOT")}),
        };
        let (content, mode) = read_probe(native_root.path(), &input);
        let mut entries: Vec<_> = std::fs::read_dir(native_root.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
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
