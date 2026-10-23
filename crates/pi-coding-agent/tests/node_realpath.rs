#![cfg(unix)]

use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsString, JsValue};
use pi_js_compat::node_error::NativeNodeError;
use pi_js_compat::node_fs::{realpath, realpath_sync_native};
use std::os::unix::ffi::OsStringExt;
use std::process::Command;

const NODE_REALPATH: &str = r#"
const fs = require('node:fs');
const [sync, path] = JSON.parse(process.argv[1]);
(async () => {
    try {
        const value = sync ? fs.realpathSync.native(path) : await fs.promises.realpath(path);
        console.log(JSON.stringify({ok:true, value}));
    } catch (error) {
        const properties = Object.fromEntries(Object.getOwnPropertyNames(error)
            .filter(key => key !== 'stack').map(key => [key, error[key]]));
        console.log(JSON.stringify({ok:false, value:{name:error.name, message:error.message, ...properties},
            constructor:error.constructor.name, string:String(error)}));
    }
})();
"#;

fn node_result(sync: bool, path: &JsString) -> JsValue {
    let request = js_json::stringify_lossless(&json!([sync, path]));
    let output = Command::new("node")
        .args(["-e", NODE_REALPATH, &request])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    js_json::parse_lossless(String::from_utf8(output.stdout).unwrap()).unwrap()
}

fn native_result(result: Result<JsString, Box<NativeNodeError>>) -> JsValue {
    match result {
        Ok(path) => json!({"ok":true,"value":path}),
        Err(error) => json!({"ok":false,"value":error.value(),
            "constructor":error.constructor_class().name(),"string":error.string_value()}),
    }
}

#[tokio::test]
async fn shared_realpath_keeps_node_values_and_original_error_identity() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("directory")).unwrap();
    std::fs::write(root.path().join("file"), "fixture").unwrap();
    std::os::unix::fs::symlink("directory", root.path().join("alias")).unwrap();
    std::os::unix::fs::symlink("loop", root.path().join("loop")).unwrap();
    let raw_name = std::ffi::OsString::from_vec(b"raw-\xff".to_vec());
    std::fs::create_dir(root.path().join(&raw_name)).unwrap();
    std::os::unix::fs::symlink(&raw_name, root.path().join("raw-alias")).unwrap();
    let prefix = JsString::from(root.path().to_str().unwrap()) + "/";
    let cases = [
        prefix.clone() + "directory",
        prefix.clone() + "alias/../file",
        prefix.clone() + "raw-alias",
        prefix.clone() + "missing/deep/file",
        prefix.clone() + "file/child",
        prefix.clone() + "loop",
        prefix.clone() + "null\0path",
        prefix.clone() + JsString::from_utf16(vec![0xd800]) + "\0path",
        prefix + "x".repeat(140) + "🐱\0path",
    ];
    for path in cases {
        assert_eq!(
            native_result(realpath(&path).await),
            node_result(false, &path),
            "async {path:?}"
        );
        assert_eq!(
            native_result(realpath_sync_native(&path)),
            node_result(true, &path),
            "sync native {path:?}"
        );
    }
}
