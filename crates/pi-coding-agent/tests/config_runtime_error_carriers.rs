#[path = "support/oracle.rs"]
mod oracle;
use std::{path::Path, process::Command};

#[test]
fn runtime_construction_retains_native_configuration_errors() {
    let directory = tempfile::tempdir().unwrap();
    let library = directory.path().join("passwd-failure.so");
    let output = Command::new("cc")
        .args(["-shared", "-fPIC", "-o"])
        .arg(&library)
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/passwd_failure.c"))
        .arg("-ldl")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let mut oracle = oracle::TsOracle::spawn_with_environment(
        "config-runtime-carriers-oracle.mjs",
        "packages/coding-agent/src/core/sdk.ts",
        &[("LD_PRELOAD", library.as_os_str().to_owned())],
    );
    let expected: serde_json::Value = oracle.request(&serde_json::json!({}));
    assert_eq!(expected["loginHelp"]["ok"], true, "{expected}");
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "runtime_error_child", "--nocapture"])
        .env("LD_PRELOAD", library)
        .env("PI_TEST_FAIL_PASSWD", "1")
        .env("PI_CONFIG_RUNTIME_EXPECTED", expected.to_string())
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
}

fn assert_system_error(error: pi_ai::request::CallbackError, scope: &str) {
    let expected: serde_json::Value =
        serde_json::from_str(&std::env::var("PI_CONFIG_RUNTIME_EXPECTED").unwrap()).unwrap();
    let expected = &expected[scope];
    let pi_js_compat::abort::AbortReason::Error(native) = error.thrown_value() else {
        panic!("configuration rejection must remain an Error");
    };
    assert_eq!(native.constructor_class().name(), "SystemError");
    assert_eq!(native.value().to_serde().unwrap(), expected["value"], "{scope}");
    assert_eq!(
        native.constructor_class().name(),
        expected["constructor"].as_str().unwrap(),
        "{scope}"
    );
    assert_eq!(
        native.string_value().to_utf8().unwrap(),
        expected["text"].as_str().unwrap(),
        "{scope}"
    );
    assert_eq!(native.value()["info"]["code"], pi_js_compat::js_json!("EIO"));
    assert!(error.original.is_some(), "native source must survive the carrier");
}

#[tokio::test]
#[ignore = "isolated forced-NSS construction child"]
async fn runtime_error_child() {
    for name in [
        "HOME",
        "XDG_CONFIG_HOME",
        "XDG_STATE_HOME",
        &pi_coding_agent::config::env_agent_dir(),
        &pi_coding_agent::config::env_state_dir(),
    ] {
        pi_js_compat::process_env::remove(name);
    }
    let error = match pi_coding_agent::core::sdk::create_agent_session(Default::default()).await {
        Ok(_) => panic!("SDK must reject home lookup failure"),
        Err(error) => error,
    };
    assert_system_error(error, "sdk");
    let help = pi_coding_agent::cli::handle_login_command(
        &["login".into(), "--help".into()],
        pi_coding_agent::cli::LoginCommandDependencies {
            auth_storage: None,
            io: None,
            use_browser_cli: None,
            open_browser_cli_url: None,
        },
    )
    .await;
    assert!(
        help.is_ok(),
        "login help must not construct default auth storage: {help:?}"
    );
    let error = match pi_coding_agent::core::agent_session_services::create_agent_session_services(
        pi_coding_agent::core::agent_session_services::CreateAgentSessionServicesOptions {
            cwd: "/fixture".into(),
            agent_dir: None,
            settings_manager: None,
            model_registry: None,
            auth_storage: None,
            extension_flag_values: None,
            resource_loader_options: None,
            resource_loader: None,
            resource_loader_reload_options: None,
        },
    )
    .await
    {
        Ok(_) => panic!("services must reject home lookup failure"),
        Err(error) => error,
    };
    assert_system_error(error, "services");
    let error = pi_coding_agent::cli::handle_login_command(
        &["login".into(), "openai-codex".into()],
        pi_coding_agent::cli::LoginCommandDependencies {
            auth_storage: None,
            io: None,
            use_browser_cli: None,
            open_browser_cli_url: None,
        },
    )
    .await
    .unwrap_err();
    assert_system_error(error, "login");
    let error = pi_coding_agent::main_runner::main(
        &["--version".into()],
        pi_coding_agent::main_runner::MainOptions::default(),
    )
    .await
    .unwrap_err();
    assert_system_error(error, "main");
}
