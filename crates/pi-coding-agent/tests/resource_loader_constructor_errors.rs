use crate::shared_oracle as oracle;

use pi_ai::request::CallbackError;
use pi_coding_agent::core::{
    auth_storage::AuthStorage,
    model_registry::ModelRegistry,
    resource_loader::{DefaultResourceLoader, DefaultResourceLoaderOptions},
    session_manager::SessionManager,
    settings_manager::{SettingsManager, SettingsManagerCreateOptions},
};
use pi_coding_agent::package_manager_cli::{DefaultProjectTrustExtensionsLoader, ProjectTrustExtensionsLoader};
use pi_js_compat::{abort::AbortReason, js_json::JsString};
use serde_json::{Value, json};
use std::{
    path::Path,
    process::Command,
    sync::{Arc, Mutex},
};

#[test]
fn resource_loader_constructor_nss_errors_keep_native_identity_through_callers() {
    let directory = tempfile::tempdir().unwrap();
    let library = directory.path().join("passwd-sequence.so");
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/passwd_sequence_failure.c");
    let compiled = Command::new("cc")
        .args(["-shared", "-fPIC", "-o"])
        .arg(&library)
        .arg(source)
        .arg("-ldl")
        .output()
        .unwrap();
    assert!(compiled.status.success(), "{compiled:?}");
    let cwd = directory.path().join("project");
    let agent_dir = directory.path().join("agent");
    std::fs::create_dir(&cwd).unwrap();
    std::fs::create_dir(&agent_dir).unwrap();
    let models = directory.path().join("models.json");
    std::fs::write(&models, "{}").unwrap();
    let mut mismatches = Vec::new();
    for (operation, allow) in [
        ("constructor", 0),
        ("sdk", 4),
        ("services", 4),
        ("trust-loader", 0),
        ("constructor", 4),
        ("sdk", 8),
        ("services", 8),
        ("trust-loader", 4),
    ] {
        let source_marker = directory.path().join(format!("source-{operation}-{allow}"));
        let native_marker = directory.path().join(format!("native-{operation}-{allow}"));
        let mut source = oracle::TsOracle::spawn_with_environment(
            "resource-loader-constructor-errors-oracle.mts",
            "packages/coding-agent/src",
            &[("LD_PRELOAD", library.as_os_str().to_owned())],
        );
        let expected=source.request(&json!({"operation":operation,"allow":allow,"marker":source_marker,"cwd":cwd,"agentDir":agent_dir,"models":models}));
        assert_eq!(expected["ok"], true, "{expected}");
        let child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "resource_loader_constructor_nss_child",
                "--nocapture",
            ])
            .env("LD_PRELOAD", &library)
            .env("PI_TEST_NSS_FAILURE_FILE", &native_marker)
            .env("PI_TEST_NSS_ALLOW", allow.to_string())
            .env("PI_RESOURCE_LOADER_CASE", operation)
            .env("PI_RESOURCE_LOADER_CWD", &cwd)
            .env("PI_RESOURCE_LOADER_AGENT", &agent_dir)
            .env_remove("HOME")
            .output()
            .unwrap();
        assert!(child.status.success(), "{child:?}");
        let stdout = String::from_utf8(child.stdout).unwrap();
        let actual: Value =
            serde_json::from_str(stdout.lines().find_map(|line| line.strip_prefix("RESULT:")).unwrap()).unwrap();
        if actual != expected["output"] {
            mismatches.push(json!({"operation":operation,"allow":allow,"actual":actual,"expected":expected["output"]}));
        }
    }
    assert!(mismatches.is_empty(), "{mismatches:#?}");
}

fn native_error_wire(error: impl Into<CallbackError>) -> Value {
    match error.into().thrown_value() {
        AbortReason::Error(error) | AbortReason::DomException(error) => error.value().to_serde().unwrap(),
        AbortReason::Value(value) => value.to_serde().unwrap(),
    }
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "private HOME-unset sequenced NSS process"]
async fn resource_loader_constructor_nss_child() {
    let operation = std::env::var("PI_RESOURCE_LOADER_CASE").unwrap();
    let cwd = JsString::from(std::env::var("PI_RESOURCE_LOADER_CWD").unwrap());
    let agent_dir = JsString::from(std::env::var("PI_RESOURCE_LOADER_AGENT").unwrap());
    let settings = SettingsManager::in_memory(Default::default(), SettingsManagerCreateOptions::default());
    let auth = AuthStorage::in_memory([] as [(&str, Value); 0]);
    let registry = ModelRegistry::in_memory(auth.clone()).unwrap();
    let session = SessionManager::in_memory(Some(&cwd), None).unwrap();
    std::fs::write(std::env::var_os("PI_TEST_NSS_FAILURE_FILE").unwrap(), "").unwrap();
    let output = match operation.as_str() {
        "sdk" => {
            pi_coding_agent::core::sdk::create_agent_session(pi_coding_agent::core::sdk::CreateAgentSessionOptions {
                no_supervisor: Some(true),
                cwd: Some(cwd),
                agent_dir: Some(agent_dir),
                auth_storage: Some(auth),
                model_registry: Some(registry),
                settings_manager: Some(Arc::new(Mutex::new(settings))),
                session_manager: Some(session),
                ..Default::default()
            })
            .await
            .err()
            .map(native_error_wire)
        }
        "services" => pi_coding_agent::core::agent_session_services::create_agent_session_services(
            pi_coding_agent::core::agent_session_services::CreateAgentSessionServicesOptions {
                cwd,
                agent_dir: Some(agent_dir),
                settings_manager: Some(settings),
                model_registry: Some(registry),
                auth_storage: Some(auth),
                extension_flag_values: None,
                resource_loader_options: None,
                resource_loader: None,
                resource_loader_reload_options: None,
            },
        )
        .await
        .err()
        .map(native_error_wire),
        "trust-loader" => DefaultProjectTrustExtensionsLoader::default()
            .load(&cwd, &agent_dir, &Arc::new(Mutex::new(settings)), &[])
            .await
            .err()
            .map(native_error_wire),
        _ => DefaultResourceLoader::new(DefaultResourceLoaderOptions {
            cwd,
            agent_dir,
            settings_manager: Some(Arc::new(Mutex::new(settings))),
            ..Default::default()
        })
        .err()
        .map(native_error_wire),
    };
    println!("RESULT:{}", output.unwrap_or(Value::Null));
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
