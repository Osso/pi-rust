use pi_js_compat::js_json::{JsString, encode_node_utf8};
use pi_js_compat::node_error::NativeNodeError;
use serde_json::{Value, json};
use std::fs;
use std::path::Path;

fn error_wire(error: NativeNodeError) -> Value {
    let value = error.value();
    json!({"ok":false,"error":{"name":error.name,"message":error.message.units(),"code":value.get("code"),"errno":value.get("errno"),"syscall":value.get("syscall"),"path":value.get("path").map(|path|path.as_string().unwrap().units()),"dest":value.get("dest").map(|path|path.as_string().unwrap().units())}})
}

struct TempDirectory(std::path::PathBuf);

impl TempDirectory {
    fn new() -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("pi-process-cwd-{}-{nonce}", std::process::id()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDirectory {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.0) {
            eprintln!("failed to remove process-cwd fixture {}: {error}", self.0.display());
        }
    }
}

fn create_directories(root: &Path) {
    for name in ["initial", "nul", "\u{fffd}", "cold", "recovered"] {
        fs::create_dir_all(root.join(name)).unwrap();
    }
    fs::write(root.join("file"), "regular file").unwrap();
}

fn operation(kind: &str, path: JsString) -> Value {
    json!({"kind":kind,"path":path.units()})
}

#[test]
fn cwd_chdir_match_node_cache_invalidation_and_native_boundaries() {
    let temp = TempDirectory::new();
    create_directories(temp.path());
    let root = JsString::from(temp.path().to_str().unwrap());
    let initial = root.clone() + "/initial";
    let cold = root.clone() + "/cold";
    let recovered = root.clone() + "/recovered";
    let missing = root.clone() + "/missing";
    let operations = vec![
        operation("chdir", initial.clone()),
        json!({"kind":"cwd"}),
        operation("chdir", missing.clone()),
        json!({"kind":"cwd"}),
        operation("chdir", root.clone() + "/file"),
        operation("chdir", JsString::default()),
        operation("chdir", root.clone() + "/nul\0tail"),
        json!({"kind":"cwd"}),
        operation("chdir", missing.clone() + "\0tail"),
        operation("chdir", root.clone() + "/" + JsString::from_utf16(vec![0xd800])),
        json!({"kind":"cwd"}),
        operation("chdir", missing.clone() + JsString::from_utf16(vec![0xd800])),
        operation("chdir", initial.clone()),
        json!({"kind":"cwd"}),
        operation("remove", initial),
        json!({"kind":"cwd"}),
        json!({"kind":"chdir","path":missing.units(),"pendingOldCwdGap":true}),
        json!({"kind":"cwd"}),
        operation("chdir", cold.clone()),
        operation("remove", cold),
        json!({"kind":"cwd"}),
        operation("chdir", recovered.clone()),
        json!({"kind":"cwd"}),
        operation("chdir", missing.clone()),
        operation("remove", recovered),
        json!({"kind":"cwd"}),
    ];
    let request = temp.path().join("request.json");
    fs::write(
        &request,
        serde_json::to_vec(&json!({"base":root,"operations":operations})).unwrap(),
    )
    .unwrap();
    let reference = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../oracle/process-cwd-primitive-oracle.mjs");
    let source = std::process::Command::new("node")
        .arg(reference)
        .arg(&request)
        .current_dir(temp.path())
        .output()
        .unwrap();
    assert!(source.status.success(), "{}", String::from_utf8(source.stderr).unwrap());
    let expected: Value = serde_json::from_slice(&source.stdout).unwrap();
    create_directories(temp.path());
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "process_cwd_primitive_child", "--nocapture"])
        .env("PI_CWD_PRIMITIVE_REQUEST", &request)
        .current_dir(temp.path())
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8(output.stderr).unwrap());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let actual: Value = serde_json::from_str(
        stdout
            .lines()
            .find_map(|line| line.strip_prefix("CWD_PRIMITIVE:"))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(actual.as_array().unwrap().len(), operations.len());
    assert_eq!(expected.as_array().unwrap().len(), operations.len());
    for (index, (actual, expected)) in actual
        .as_array()
        .unwrap()
        .iter()
        .zip(expected.as_array().unwrap())
        .enumerate()
    {
        if operations[index].get("pendingOldCwdGap") == Some(&Value::Bool(true)) {
            assert_failed_chdir_deleted_old_path(actual, expected, &missing);
        } else {
            assert_eq!(actual, expected, "operation {index}: {}", operations[index]);
        }
    }
}

fn assert_failed_chdir_deleted_old_path(actual: &Value, source: &Value, destination: &JsString) {
    // Pending user approval: Node's old-path buffer is unstable when raw uv_cwd fails.
    assert_eq!(actual["ok"], false);
    assert_eq!(source["ok"], false);
    for field in ["name", "code", "errno", "syscall", "dest"] {
        assert_eq!(actual["error"][field], source["error"][field], "field {field}");
    }
    assert_eq!(
        actual["error"]["path"],
        json!([]),
        "native old path is explicitly empty"
    );
    let native_message = pi_js_compat::js_format!("ENOENT: no such file or directory, chdir '' -> '{}'", destination);
    assert_eq!(actual["error"]["message"], json!(native_message.units()));
    let source_old = JsString::from_utf16(
        source["error"]["path"]
            .as_array()
            .unwrap()
            .iter()
            .map(|unit| u16::try_from(unit.as_u64().unwrap()).unwrap())
            .collect(),
    );
    let source_message = pi_js_compat::js_format!(
        "ENOENT: no such file or directory, chdir '{}' -> '{}'",
        source_old,
        destination
    );
    assert_eq!(
        source["error"]["message"],
        json!(source_message.units()),
        "source keeps its own observed path/message relation"
    );
}

#[test]
#[ignore = "isolated process cwd child invoked by parent test"]
fn process_cwd_primitive_child() {
    let request: Value =
        serde_json::from_slice(&fs::read(std::env::var_os("PI_CWD_PRIMITIVE_REQUEST").unwrap()).unwrap()).unwrap();
    let results: Vec<Value> = request["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|operation| {
            let kind = operation["kind"].as_str().unwrap();
            if kind == "cwd" {
                return match pi_js_compat::process_cwd::cwd() {
                    Ok(cwd) => json!({"ok":true,"cwd":cwd.units()}),
                    Err(error) => error_wire(error),
                };
            }
            let path = JsString::from_utf16(
                operation["path"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|unit| u16::try_from(unit.as_u64().unwrap()).unwrap())
                    .collect(),
            );
            match kind {
                "chdir" => match pi_js_compat::process_cwd::chdir(&path) {
                    Ok(()) => json!({"ok":true}),
                    Err(error) => error_wire(error),
                },
                "remove" => {
                    fs::remove_dir(encode_node_utf8(&path)).unwrap();
                    json!({"ok":true})
                }
                _ => panic!("unknown operation {kind}"),
            }
        })
        .collect();
    pi_js_compat::process_cwd::chdir(&request["base"].as_str().unwrap().into()).unwrap();
    println!("CWD_PRIMITIVE:{}", json!(results));
}
