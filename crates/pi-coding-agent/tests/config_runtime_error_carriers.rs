use crate::shared_oracle as oracle;
use std::{path::Path, process::Command};

#[test]
fn source_less_callback_messages_keep_raw_utf16_units() {
    use pi_ai::request::CallbackError;
    use pi_js_compat::js_json::JsString;
    for raw in [
        JsString::from_utf16(vec![0xd800, 0x61]),
        JsString::from_utf16(vec![0x61, 0xdc00]),
    ] {
        let errors: Vec<CallbackError> = vec![
            pi_coding_agent::core::agent_session::AgentSessionError::new(raw.clone()).into(),
            pi_coding_agent::core::session_manager::SessionManagerError::new(raw.clone()).into(),
            pi_coding_agent::core::sqlite::ControlDbError::new(raw.clone()).into(),
            pi_coding_agent::core::multi_agent_store::MultiAgentStoreError::new(raw.clone()).into(),
            pi_coding_agent::core::package_manager::PackageManagerError::new(raw.clone()).into(),
            pi_coding_agent::utils::git_worktree::WorktreeStartupError::new(raw.clone()).into(),
            pi_coding_agent::modes::interactive::theme::ThemeError::Syntax { message: raw.clone() }.into(),
        ];
        for error in errors {
            assert_eq!(error.message, raw);
            assert!(error.original.is_some());
        }
    }
}

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

fn assert_failure<T>(result: Result<T, pi_ai::request::CallbackError>, scope: &str) {
    match result {
        Ok(_) => panic!("{scope} must reject failed configuration lookup"),
        Err(error) => assert_system_error(error, scope),
    }
}

struct DefaultCliDependencies;
impl pi_coding_agent::cli::ControlCommandDependencies for DefaultCliDependencies {}
impl pi_coding_agent::cli::DebugCommandDependencies for DefaultCliDependencies {}
impl pi_coding_agent::cli::SessionsCommandDependencies for DefaultCliDependencies {}

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
    assert_failure(
        pi_coding_agent::cli::handle_control_command(&["control".into(), "path".into()], &DefaultCliDependencies),
        "control",
    );
    assert_failure(
        pi_coding_agent::cli::handle_debug_command(
            &["debug".into(), "attach".into(), "fixture".into()],
            &DefaultCliDependencies,
        )
        .await,
        "debug",
    );
    assert_failure(
        pi_coding_agent::cli::handle_sessions_command(
            &["sessions".into(), "truncate-tool-output".into()],
            &DefaultCliDependencies,
        )
        .await,
        "sessions",
    );
    let settings =
        pi_coding_agent::core::settings_manager::SettingsManager::in_memory(Default::default(), Default::default());
    assert_failure(
        pi_coding_agent::cli::startup_ui::load_startup_themes(&settings).await,
        "startup",
    );
    assert_failure(pi_coding_agent::architect::run_architect_service().await, "architect");
    assert_failure(
        pi_coding_agent::supervisor::run_supervisor_service().await,
        "supervisor",
    );
    assert_failure(pi_coding_agent::core::host_relay::run_relay_serve().await, "relay");
    assert_failure(
        pi_coding_agent::core::host_relay_resident::run_relay_connect().await,
        "relayResident",
    );
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
