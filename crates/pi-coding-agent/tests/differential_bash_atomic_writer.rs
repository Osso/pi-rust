use crate::shared_oracle as oracle;
use pi_coding_agent::core::detached_bash_runner::write_detached_bash_activation;
use pi_coding_agent::core::session_control_db::DetachedJobOwnershipIdentity;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsString, JsValue};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
fn identity() -> DetachedJobOwnershipIdentity {
    js_json::from_value(&js_json::parse_lossless(r#"{"jobId":"job","owner":{"sessionId":"session","agentId":null},"processIdentity":{"pid":123,"startTimeTicks":456},"outputLabel":"Bash output"}"#).unwrap()).unwrap()
}
fn snapshot(path: &Path) -> JsValue {
    if !path.exists() {
        return JsValue::Null;
    }
    if path.is_dir() {
        return json!({"directory":true});
    }
    use std::os::unix::fs::PermissionsExt;
    json!({"bytes":std::fs::read(path).unwrap().iter().map(|byte|format!("{byte:02x}")).collect::<String>(),"mode":std::fs::metadata(path).unwrap().permissions().mode()&0o777})
}
fn normalize(value: &JsValue, root: &str) -> JsValue {
    match value {
        JsValue::String(value) => json!(value.replace(root, "<ROOT>")),
        JsValue::Array(values) => JsValue::Array(values.iter().map(|value| normalize(value, root)).collect()),
        JsValue::Object(values) => JsValue::Object(
            values
                .iter()
                .map(|(key, value)| (key.clone(), normalize(value, root)))
                .collect(),
        ),
        value => value.clone(),
    }
}
#[test]
fn atomic_writer_native_errors_and_finally_close_precedence_match_frozen() {
    let root = tempfile::tempdir().unwrap();
    let library = root.path().join("atomic-failure.so");
    let compiled = Command::new("cc")
        .args(["-shared", "-fPIC", "-o"])
        .arg(&library)
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/bash_spill_failure.c"))
        .arg("-ldl")
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let modes = [
        "success",
        "open-directory",
        "write",
        "close",
        "write-close",
        "fsync",
        "close-after-sync",
        "fsync-close",
        "rename",
        "dir-fsync",
        "dir-close-after-sync",
        "dir-fsync-close",
    ];
    let mut source = oracle::TsOracle::spawn_with_environment(
        "bash-atomic-writer-oracle.ts",
        "packages/coding-agent/src/core/detached-bash-runner.ts",
        &[("LD_PRELOAD", library.as_os_str().to_owned())],
    );
    let expected =
        source.request(&json!({"id":1,"root":root.path().to_str().unwrap(),"identity":identity(),"cases":modes}));
    assert_eq!(expected["ok"], json!(true), "{expected:?}");
    for (index, mode) in modes.iter().enumerate() {
        assert_eq!(
            expected["output"][index]["result"]["ok"],
            json!(index == 0),
            "source fixture must actually fault {mode}: {:?}",
            expected["output"][index]
        );
    }
    let mut failures = Vec::new();
    for (index, mode) in modes.iter().enumerate() {
        let directory = root.path().join(format!("native-{index}"));
        std::fs::create_dir(&directory).unwrap();
        let dir_fault = mode.starts_with("dir-");
        let needle = if dir_fault {
            directory.clone()
        } else {
            directory.join("activation.json.tmp")
        };
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["--ignored", "--exact", "atomic_writer_child", "--nocapture"])
            .env("LD_PRELOAD", &library)
            .env("PI_ATOMIC_MODE", mode)
            .env("PI_ATOMIC_DIR", &directory)
            .env("PI_BASH_SPILL_PATH", needle)
            .env("PI_BASH_SPILL_EXACT", "1")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if *mode == "success" || *mode == "open-directory" {
            command.env_remove("PI_BASH_SPILL_FAIL");
        } else {
            command.env("PI_BASH_SPILL_FAIL", mode.strip_prefix("dir-").unwrap_or(mode));
        }
        let mut child = command.spawn().unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while child.try_wait().unwrap().is_none() {
            if Instant::now() >= deadline {
                child.kill().unwrap();
                let output = child.wait_with_output().unwrap();
                panic!("writer child timed out: {}", String::from_utf8_lossy(&output.stderr));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let stdout = String::from_utf8(output.stdout).unwrap();
        let line = stdout
            .lines()
            .find_map(|line| line.strip_prefix("ATOMIC_RESULT:"))
            .unwrap_or_else(|| panic!("missing record: {stdout}"));
        let actual = normalize(&js_json::parse_lossless(line).unwrap(), directory.to_str().unwrap());
        let expected = normalize(
            &expected["output"][index],
            root.path().join(format!("source-{index}")).to_str().unwrap(),
        );
        if actual != expected {
            failures.push(json!({"mode":mode,"actual":actual,"expected":expected}));
        }
    }
    assert!(failures.is_empty(), "{}", js_json::stringify_lossless(&json!(failures)));
}
#[test]
#[ignore = "isolated native atomic-writer syscall failures"]
fn atomic_writer_child() {
    let directory = std::env::var("PI_ATOMIC_DIR").unwrap();
    let mode = std::env::var("PI_ATOMIC_MODE").unwrap();
    let path = Path::new(&directory).join("activation.json");
    let temporary = Path::new(&directory).join("activation.json.tmp");
    if mode == "open-directory" {
        std::fs::create_dir(&temporary).unwrap();
    }
    let result = match write_detached_bash_activation(&JsString::from(path.to_str().unwrap()), &identity()) {
        Ok(()) => json!({"ok":true}),
        Err(error) => json!({"ok":false,"error":error.message()}),
    };
    println!(
        "ATOMIC_RESULT:{}",
        js_json::stringify_lossless(&json!({"result":result,"temporary":snapshot(&temporary),"final":snapshot(&path)}))
    );
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
