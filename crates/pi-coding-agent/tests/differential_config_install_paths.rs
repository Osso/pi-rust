#![cfg(unix)]

#[path = "support/oracle.rs"]
mod oracle;

use pi_coding_agent::config::{self, InstallContext};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsValue};
use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

fn context(request: &JsValue) -> InstallContext {
    InstallContext {
        package_dir: PathBuf::from(request["packageDir"].as_string().unwrap().to_utf8().unwrap()),
        exec_path: "/fixture/npm/node".into(),
        entrypoint: request
            .get("entrypoint")
            .and_then(JsValue::as_string)
            .map(|value| value.to_utf8().unwrap()),
        is_bun_binary: false,
        is_bun_runtime: false,
        platform: "linux".into(),
        home: std::env::temp_dir(),
        path_env: Some("/fixture/no-package-managers".into()),
        env: BTreeMap::new(),
    }
}

fn payload(request: &JsValue) -> JsValue {
    let context = context(request);
    let result = if request["mode"] == json!("instruction") {
        config::get_update_instruction(&context, &"package".into()).map(Some)
    } else {
        let command = [request["tool"].as_string().unwrap().clone()];
        config::get_self_update_command(&context, &"package".into(), Some(&command), None)
            .map(|command| command.map(|command| command.display))
    };
    match result {
        Ok(value) => json!({"value":value}),
        Err(error) => json!({"error":{"name":error.label(),"message":error.message()}}),
    }
}

#[test]
fn install_paths_preserve_posix_cwd_and_entrypoint_semantics() {
    let mut oracle = oracle::TsOracle::spawn("config-path-branches-oracle.mjs", "packages/coding-agent/src/config.ts");
    let mut failures = Vec::new();
    for mode in ["relative-package", "deleted-cwd", "backslash-entrypoint"] {
        let temp = tempfile::tempdir().unwrap();
        let managed = temp.path().join("managed");
        let package = if mode == "backslash-entrypoint" {
            temp.path().join("private-package")
        } else {
            managed.join("package")
        };
        std::fs::create_dir_all(&package).unwrap();
        std::fs::create_dir_all(&managed).unwrap();
        let tool = temp.path().join("manager");
        std::fs::write(
            &tool,
            format!("#!/bin/sh\n/bin/printf '%s\\n' '{}'\n", managed.display()),
        )
        .unwrap();
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o700)).unwrap();
        let source_cwd = managed.join("source-cwd");
        let native_cwd = managed.join("native-cwd");
        std::fs::create_dir(&source_cwd).unwrap();
        std::fs::create_dir(&native_cwd).unwrap();
        let mut request = json!({"mode":"command","packageDir":package,"tool":tool,"cwd":native_cwd,"deleteCwd":mode == "deleted-cwd"});
        let mut source = request.clone();
        source.as_object_mut().unwrap().insert("cwd".into(), json!(source_cwd));
        if mode == "relative-package" {
            request
                .as_object_mut()
                .unwrap()
                .insert("packageDir".into(), json!("package"));
            source
                .as_object_mut()
                .unwrap()
                .insert("packageDir".into(), json!("package"));
            request.as_object_mut().unwrap().insert("cwd".into(), json!(managed));
            source.as_object_mut().unwrap().insert("cwd".into(), json!(managed));
        }
        if mode == "backslash-entrypoint" {
            let entry = managed.join("entry\\folder");
            std::fs::create_dir(&entry).unwrap();
            std::fs::write(entry.join("package.json"), "{}").unwrap();
            request
                .as_object_mut()
                .unwrap()
                .insert("entrypoint".into(), json!(entry.join("cli.js")));
            source
                .as_object_mut()
                .unwrap()
                .insert("entrypoint".into(), json!(entry.join("cli.js")));
        }
        let expected: JsValue = oracle.request(&source);
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "install_path_child",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("PI_CONFIG_INSTALL_PATH_REQUEST", js_json::stringify_lossless(&request))
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let stdout = String::from_utf8(output.stdout).unwrap();
        let actual = js_json::parse_lossless(
            stdout
                .lines()
                .find_map(|line| line.find("RESULT:").map(|index| &line[index + 7..]))
                .unwrap(),
        )
        .unwrap();
        if actual != expected {
            failures.push(format!(
                "{mode}: {} != {}",
                js_json::stringify_lossless(&actual),
                js_json::stringify_lossless(&expected)
            ));
        }
    }
    for package in [
        r"C:lib\node_modules\package",
        r"C:/prefix/lib/node_modules\package",
        r"\\server\share\lib\node_modules\package",
        r"\\server\lib\node_modules\package",
        r"C:\prefix\lib\node_modules\@scope\package",
        r"C:\β\lib\node_modules\package",
        r"relative\lib\node_modules\package",
        r"C:\\prefix\\lib\\node_modules\\package",
        "lib//node_modules//package",
        "//lib/node_modules/package",
        "relative/lib/node_modules/@scope/package",
        "relative/lib/node_modules/package/",
    ] {
        let request = json!({"mode":"instruction","packageDir":package});
        let expected: JsValue = oracle.request(&request);
        let actual = payload(&request);
        if actual != expected {
            failures.push(format!(
                "{package}: {} != {}",
                js_json::stringify_lossless(&actual),
                js_json::stringify_lossless(&expected)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
#[ignore = "isolated real cwd mutation"]
fn install_path_child() {
    let request = js_json::parse_lossless(std::env::var("PI_CONFIG_INSTALL_PATH_REQUEST").unwrap()).unwrap();
    let previous = std::env::current_dir().unwrap();
    let cwd = request["cwd"].as_string().unwrap().to_utf8().unwrap();
    pi_js_compat::process_cwd::chdir(&pi_js_compat::node_fs::decode_node_path(std::ffi::OsStr::new(&(&cwd)))).unwrap();
    if request["deleteCwd"] == JsValue::Bool(true) {
        std::fs::remove_dir(&cwd).unwrap();
    }
    let result = payload(&request);
    pi_js_compat::process_cwd::chdir(&pi_js_compat::node_fs::decode_node_path(std::ffi::OsStr::new(
        &(previous),
    )))
    .unwrap();
    println!("RESULT:{}", js_json::stringify_lossless(&result));
}
