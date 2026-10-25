use crate::shared_oracle as oracle;

use pi_coding_agent::core::{
    package_manager::{DefaultPackageManager, SettingsManagerStore},
    settings_manager::{Settings, SettingsManager, SettingsManagerCreateOptions},
};
use serde_json::{Value, json};
use std::{process::Command, sync::Arc};

#[test]
fn relative_constructor_paths_resolve_against_process_cwd_once() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(directory.path().join("project")).unwrap();
    std::fs::create_dir_all(directory.path().join("agent/pkg")).unwrap();
    let mut source = oracle::TsOracle::spawn(
        "package-manager-oracle.ts",
        "packages/coding-agent/src/core/package-manager.ts",
    );
    let expected: Value = source.request(
        &json!({"id":"relative","operation":"get_installed_path","processCwd":directory.path(),
        "scenario":{"cwd":"project","agentDir":"agent","trusted":true},"source":"pkg","scope":"user"}),
    );
    assert_eq!(expected["ok"], true, "{expected}");
    let child = Command::new(std::env::current_exe().unwrap())
        .current_dir(directory.path())
        .args(["--ignored", "--exact", "relative_constructor_child", "--nocapture"])
        .output()
        .unwrap();
    assert!(child.status.success(), "{child:?}");
    let text = String::from_utf8(child.stdout).unwrap();
    let actual: Value =
        serde_json::from_str(text.lines().find_map(|line| line.strip_prefix("RESULT:")).unwrap()).unwrap();
    assert_eq!(actual, expected["output"]);
}

#[test]
#[ignore = "isolated constructor cwd child"]
fn relative_constructor_child() {
    let settings = SettingsManager::in_memory(Settings::new(), SettingsManagerCreateOptions::default());
    let manager = DefaultPackageManager::new(
        &"project".into(),
        &"agent".into(),
        Arc::new(SettingsManagerStore::new(settings)),
    )
    .unwrap();
    let path = manager
        .get_installed_path(
            &"pkg".into(),
            pi_coding_agent::core::package_manager::InstalledScope::User,
        )
        .unwrap();
    println!("RESULT:{}", json!({"path":path}));
}

#[test]
fn package_manager_propagates_constructor_resolve_and_install_home_failures() {
    let directory = tempfile::tempdir().unwrap();
    let library = directory.path().join("passwd-failure.so");
    let marker = directory.path().join("fail-passwd");
    let home = directory.path().join("home");
    std::fs::create_dir(&home).unwrap();
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/passwd_failure.c");
    let output = Command::new("cc")
        .args(["-shared", "-fPIC", "-o"])
        .arg(&library)
        .arg(source)
        .arg("-ldl")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let mut source = oracle::TsOracle::spawn_with_environment(
        "package-manager-home-oracle.mts",
        "packages/coding-agent/src",
        &[("LD_PRELOAD", library.as_os_str().to_owned())],
    );
    for operation in [
        "constructor",
        "resolve",
        "install",
        "installed-path",
        "identity",
        "list",
        "remove",
        "add-match",
        "check-updates",
    ] {
        let expected: Value =
            source.request(&json!({"operation":operation,"base":directory.path(),"home":home,"marker":marker}));
        if marker.exists() {
            std::fs::remove_file(&marker).unwrap();
        }
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "package_home_child", "--nocapture"])
            .env_remove("HOME")
            .env("PI_OFFLINE", "0")
            .env("LD_PRELOAD", &library)
            .env(
                "PI_TEST_FAIL_PASSWD",
                if operation == "constructor" { "1" } else { "0" },
            )
            .env("PI_TEST_PASSWD_FAILURE_FILE", &marker)
            .env("PI_TEST_PASSWD_HOME", &home)
            .env("PI_TEST_PACKAGE_BASE", directory.path())
            .env("PI_TEST_PACKAGE_OPERATION", operation)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let stdout = String::from_utf8(output.stdout).unwrap();
        let actual: Value =
            serde_json::from_str(stdout.lines().find_map(|line| line.strip_prefix("RESULT:")).unwrap()).unwrap();
        assert_eq!(actual, expected, "{operation}");
        assert_eq!(actual["threw"], true, "{operation}");
        if marker.exists() {
            std::fs::remove_file(&marker).unwrap();
        }
    }
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "isolated HOME/passwd child"]
async fn package_home_child() {
    let mut values = Settings::new();
    values.insert("packages".into(), pi_js_compat::js_json!(["~"]));
    let settings = SettingsManager::in_memory(values, SettingsManagerCreateOptions::default());
    let base = pi_js_compat::js_json::JsString::from(std::env::var("PI_TEST_PACKAGE_BASE").unwrap());
    let operation = std::env::var("PI_TEST_PACKAGE_OPERATION").unwrap();
    let result = DefaultPackageManager::new(&base, &base, Arc::new(SettingsManagerStore::new(settings)));
    let result = match result {
        Err(error) => Err(error),
        Ok(manager) => {
            if operation == "constructor" {
                Ok(())
            } else {
                std::fs::write(std::env::var("PI_TEST_PASSWD_FAILURE_FILE").unwrap(), "fail").unwrap();
                match operation.as_str() {
                    "resolve" => manager.resolve(None).await.map(|_| ()),
                    "install" => manager.install(&"~".into(), false).await,
                    "installed-path" => manager
                        .get_installed_path(
                            &"~".into(),
                            pi_coding_agent::core::package_manager::InstalledScope::User,
                        )
                        .map(|_| ()),
                    "identity" => manager.get_package_identity(&"~".into(), None).map(|_| ()),
                    "list" => manager.list_configured_packages().map(|_| ()),
                    "remove" => manager.remove_source_from_settings(&"~".into(), false).map(|_| ()),
                    "add-match" => manager
                        .add_source_to_settings(&"npm:test-package".into(), false)
                        .map(|_| ()),
                    "check-updates" => manager.check_for_available_updates().await.map(|_| ()),
                    _ => panic!("unexpected operation {operation}"),
                }
            }
        }
    };
    let value = match result {
        Ok(()) => json!({"threw":false}),
        Err(error) => {
            let native = error.native_error().expect("native SystemError source");
            json!({"threw":true,"message":error.message(),"error":native.string_value()})
        }
    };
    println!("RESULT:{value}");
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
