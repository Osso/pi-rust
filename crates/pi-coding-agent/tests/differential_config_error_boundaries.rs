use crate::shared_oracle as oracle;

use pi_coding_agent::config::{self, ConfigError};
use serde_json::{Value, json};
use std::process::Command;

type Getter = fn() -> Result<pi_js_compat::js_json::JsString, ConfigError>;
const GETTERS: &[(&str, Getter)] = &[
    ("getPackageDir", config::get_package_dir),
    ("getReadmePath", config::get_readme_path),
    ("getDocsPath", config::get_docs_path),
    ("getExamplesPath", config::get_examples_path),
    ("getUserConfigRoot", config::get_user_config_root),
    ("getUserStateRoot", config::get_user_state_root),
    ("getUserCacheRoot", config::get_user_cache_root),
    ("getLegacyAgentDir", config::get_legacy_agent_dir),
    ("getAgentDir", config::get_agent_dir),
    ("getCustomThemesDir", config::get_custom_themes_dir),
    ("getModelsPath", config::get_models_path),
    ("getAuthPath", config::get_auth_path),
    ("getSettingsPath", config::get_settings_path),
    ("getToolsDir", config::get_tools_dir),
    ("getBinDir", config::get_bin_dir),
    ("getPromptsDir", config::get_prompts_dir),
    ("getPromptHistoryPath", config::get_prompt_history_path),
    ("getSessionsDir", config::get_sessions_dir),
    ("getDebugLogPath", config::get_debug_log_path),
];

#[test]
fn config_getters_preserve_native_error_class_properties_and_source() {
    let directory = tempfile::tempdir().unwrap();
    let library = directory.path().join("passwd-failure.so");
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/passwd_failure.c");
    let compiled = Command::new("cc")
        .args(["-shared", "-fPIC", "-o"])
        .arg(&library)
        .arg(source)
        .arg("-ldl")
        .output()
        .unwrap();
    assert!(compiled.status.success(), "{compiled:?}");
    let mut oracle = oracle::TsOracle::spawn_with_environment(
        "config-error-boundaries-oracle.mjs",
        "packages/coding-agent/src/config.ts",
        &[("LD_PRELOAD", library.as_os_str().to_owned())],
    );
    let home_getters: Vec<_> = GETTERS.iter().skip(4).map(|(name, _)| *name).collect();
    let package_getters = vec!["getPackageDir", "getReadmePath", "getDocsPath", "getExamplesPath"];
    let mut cases = vec![
        json!({"fail":false,"getters":home_getters}),
        json!({"fail":true,"getters":home_getters}),
        json!({"home":"/explicit/home","fail":true,"getters":home_getters}),
        json!({"home":"","fail":true,"getters":home_getters}),
        json!({"home":"/fixture-home", "cacheHome":"~/cache-data", "getters":["getUserCacheRoot"]}),
        json!({"fail":true, "cacheHome":"/absolute/cache", "getters":["getUserCacheRoot"]}),
        json!({"home":"/fixture-home", "cacheHome":"file://not-local/cache", "getters":["getUserCacheRoot"]}),
    ];
    for package in [
        "file://not-local/pkg",
        "file:///x%2fy",
        "file:///bad%ZZ",
        "file://[",
        "relative/pkg",
    ] {
        cases.push(json!({"packageDir":package,"getters":package_getters}));
    }
    let mut mismatches = Vec::new();
    for case in cases {
        let expected: Value = oracle.request(&case);
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "config_error_child", "--nocapture"])
            .env_remove("PI_PACKAGE_DIR")
            .env_remove("HOME")
            .env(
                "PI_TEST_FAIL_PASSWD",
                if case["fail"].as_bool().unwrap_or(false) {
                    "1"
                } else {
                    "0"
                },
            )
            .env("LD_PRELOAD", &library)
            .env("PI_CONFIG_ERROR_CASE", case.to_string())
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let stdout = String::from_utf8(output.stdout).unwrap();
        let actual: Value =
            serde_json::from_str(stdout.lines().find_map(|line| line.strip_prefix("RESULT:")).unwrap()).unwrap();
        if actual != expected {
            mismatches.push(format!("{case}: {actual}\nexpected {expected}"));
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

#[test]
#[ignore = "isolated post-import environment/NSS child"]
fn config_error_child() {
    use pi_js_compat::process_env;
    use std::error::Error;
    let case: Value = serde_json::from_str(&std::env::var("PI_CONFIG_ERROR_CASE").unwrap()).unwrap();
    let agent_env = config::env_agent_dir();
    let state_env = config::env_state_dir();
    for name in [
        "HOME",
        "XDG_CONFIG_HOME",
        "XDG_STATE_HOME",
        "XDG_CACHE_HOME",
        &agent_env,
        &state_env,
    ] {
        process_env::remove(name);
    }
    if let Some(home) = case["home"].as_str() {
        process_env::set("HOME", home);
    }
    if let Some(cache) = case["cacheHome"].as_str() {
        process_env::set("XDG_CACHE_HOME", cache);
    }
    process_env::set(
        "PI_TEST_FAIL_PASSWD",
        if case["fail"].as_bool().unwrap_or(false) {
            "1"
        } else {
            "0"
        },
    );
    if let Some(package) = case["packageDir"].as_str() {
        process_env::set("PI_PACKAGE_DIR", package);
    }
    let mut output = serde_json::Map::new();
    for name in case["getters"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
    {
        let getter = GETTERS.iter().find(|(candidate, _)| *candidate == name).unwrap().1;
        let value = match getter() {
            Ok(path) => json!({"value":path}),
            Err(error) => {
                let native = error
                    .native_error()
                    .expect("original native error must remain available");
                let source = error
                    .source()
                    .unwrap()
                    .downcast_ref::<pi_js_compat::node_error::NativeNodeError>()
                    .unwrap();
                assert_eq!(source, native);
                json!({"error":native.value().to_serde().unwrap(),"constructor":native.constructor_class().name(),"text":native.string_value()})
            }
        };
        output.insert(name.into(), value);
    }
    println!("RESULT:{}", Value::Object(output));
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
