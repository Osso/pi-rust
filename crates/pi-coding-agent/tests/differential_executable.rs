use crate::shared_oracle as oracle;

use pi_coding_agent::utils::executable::{ExecutableAvailabilityOptions, is_executable_available};
use pi_js_compat::js_json::{JsString, JsValue, from_value};
use pi_js_compat::node_fs::decode_node_path;
use pi_js_compat::{js_format, js_json};

#[test]
fn executable_lookup_matches_frozen_with_scalar_and_raw_os_cwds() {
    use std::os::unix::ffi::OsStringExt;
    let temp = tempfile::tempdir().unwrap();
    for name in [
        std::ffi::OsString::from("ordinary"),
        std::ffi::OsString::from_vec(b"raw-\xff".to_vec()),
    ] {
        let cwd = temp.path().join(name);
        std::fs::create_dir(&cwd).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "executable_probe", "--nocapture"])
            .current_dir(cwd)
            .env("PI_Q5_EXECUTABLE_PROBE", "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8(output.stdout).unwrap(),
            String::from_utf8(output.stderr).unwrap()
        );
    }
}

#[test]
#[ignore = "controlled working directory probe"]
fn executable_probe() {
    if std::env::var_os("PI_Q5_EXECUTABLE_PROBE").is_none() {
        return;
    }
    use std::os::unix::fs::{PermissionsExt, symlink};
    let cwd = std::env::current_dir().unwrap();
    std::fs::create_dir("bin").unwrap();
    for name in ["fixture", "nonexec", "raw-\u{fffd}"] {
        std::fs::write(cwd.join("bin").join(name), "fixture bytes").unwrap();
        std::fs::set_permissions(
            cwd.join("bin").join(name),
            std::fs::Permissions::from_mode(if name == "nonexec" { 0o644 } else { 0o755 }),
        )
        .unwrap();
    }
    symlink("fixture", "bin/link").unwrap();
    symlink("missing", "bin/broken").unwrap();
    std::fs::create_dir("bin/directory").unwrap();
    std::fs::write("local", "local fixture").unwrap();
    std::fs::set_permissions("local", std::fs::Permissions::from_mode(0o755)).unwrap();
    let cwd = decode_node_path(cwd.as_os_str());
    let bin = js_format!("{}/bin", cwd);
    let raw = JsString::from("raw-") + JsString::from_utf16(vec![0xd800]);
    let cases = vec![
        js_json!({"command": "", "options": {}}),
        js_json!({"command": "fixture", "options": {"env": {"PATH": bin}}}),
        js_json!({"command": "fixture", "options": {"env": {"path": bin}}}),
        js_json!({"command": "fixture", "options": {"env": {"Path": "", "PATH": bin}}}),
        js_json!({"command": "fixture", "options": {"env": {"PATH": "", "Path": bin}}}),
        js_json!({"command": "fixture", "options": {"env": {"PATH": "bin"}}}),
        js_json!({"command": "local", "options": {"env": {"PATH": ":/missing"}}}),
        js_json!({"command": "local", "options": {"cwd": cwd, "env": {"PATH": ""}}}),
        js_json!({"command": "sh", "options": {"env": {}}}),
        js_json!({"command": "sh", "options": {}}),
        js_json!({"command": "bin/fixture", "options": {"cwd": cwd}}),
        js_json!({"command": "./bin/fixture", "options": {"cwd": "."}}),
        js_json!({"command": js_format!("{}/fixture", bin), "options": {"cwd": "/missing"}}),
        js_json!({"command": "nonexec", "options": {"env": {"PATH": bin}}}),
        js_json!({"command": "directory", "options": {"env": {"PATH": bin}}}),
        js_json!({"command": "link", "options": {"env": {"PATH": bin}}}),
        js_json!({"command": "broken", "options": {"env": {"PATH": bin}}}),
        js_json!({"command": raw, "options": {"env": {"PATH": bin}}}),
        js_json!({"command": "bad\u{0}name", "options": {"env": {"PATH": bin}}}),
    ];
    let mut oracle = oracle::TsOracle::spawn("executable-oracle.ts", "packages/coding-agent/src/utils/executable.ts");
    let expected = oracle.request(&js_json!({"id": 12, "operation": "executable", "cases": cases}));
    assert_eq!(expected["ok"], js_json!(true), "{expected:?}");
    let expected = expected["output"].as_array().unwrap();
    for (index, (case, expected)) in cases.iter().zip(expected).enumerate() {
        let options: ExecutableAvailabilityOptions = from_value(&case["options"]).unwrap();
        let actual = is_executable_available(case["command"].as_string().unwrap(), &options).unwrap();
        assert_eq!(JsValue::Bool(actual), *expected, "case {index}: {case:?}");
    }
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
