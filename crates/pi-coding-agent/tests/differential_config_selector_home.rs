use crate::shared_oracle as oracle;

use pi_coding_agent::{
    core::{
        package_manager::{MetadataOrigin, PathMetadata, ResolvedPaths, ResolvedResource, SourceScope},
        settings_manager::{Settings, SettingsManager, SettingsManagerCreateOptions},
    },
    modes::interactive::components::config_selector::{ConfigSelectorComponent, ConfigSelectorOptions},
};
use serde_json::{Value, json};
use std::{cell::RefCell, process::Command, rc::Rc};

#[test]
fn config_constructor_propagates_home_lookup_failure() {
    let directory = tempfile::tempdir().unwrap();
    let library = directory.path().join("passwd-failure.so");
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
        "config-selector-home-oracle.mts",
        "packages/coding-agent/src",
        &[("LD_PRELOAD", library.as_os_str().to_owned())],
    );
    for scope in ["user", "project"] {
        let expected: Value = source.request(&json!({"scope":scope,"source":"auto"}));
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "config_home_child", "--nocapture"])
            .env_remove("HOME")
            .env("LD_PRELOAD", &library)
            .env("PI_TEST_FAIL_PASSWD", "1")
            .env("PI_TEST_SCOPE", scope)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let stdout = String::from_utf8(output.stdout).unwrap();
        let actual: Value =
            serde_json::from_str(stdout.lines().find_map(|line| line.strip_prefix("RESULT:")).unwrap()).unwrap();
        assert_eq!(actual, expected, "{scope}");
        assert_eq!(actual["threw"], true);
    }
}

#[test]
#[ignore = "isolated HOME/passwd child"]
fn config_home_child() {
    let scope = if std::env::var("PI_TEST_SCOPE").unwrap() == "user" {
        SourceScope::User
    } else {
        SourceScope::Project
    };
    let resolved_paths = ResolvedPaths {
        extensions: vec![ResolvedResource {
            path: "/repo/extension.ts".into(),
            enabled: true,
            metadata: PathMetadata {
                source: "auto".into(),
                scope,
                origin: MetadataOrigin::TopLevel,
                base_dir: Some("/repo".into()),
            },
        }],
        ..Default::default()
    };
    let options = ConfigSelectorOptions {
        resolved_paths,
        settings_manager: Rc::new(RefCell::new(SettingsManager::in_memory(
            Settings::new(),
            SettingsManagerCreateOptions::default(),
        ))),
        cwd: "/repo".into(),
        agent_dir: "/agent".into(),
        on_close: Rc::new(|| {}),
        on_exit: Rc::new(|| {}),
        request_render: Rc::new(|| {}),
        terminal_height: Some(24),
    };
    let result = ConfigSelectorComponent::new(options);
    let value = match result {
        Ok(_) => json!({"threw":false}),
        Err(error) => json!({"threw":true,"message":error.message,"error":error.string_value()}),
    };
    println!("RESULT:{value}");
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
