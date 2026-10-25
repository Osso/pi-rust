use crate::shared_oracle as oracle;

use pi_coding_agent::core::session_manager::SessionManager;
use pi_js_compat::abort::AbortReason;
use serde_json::{Value, json};

fn error_wire(error: pi_ai::request::CallbackError) -> Value {
    let native = match error.thrown_value() {
        AbortReason::Error(error) | AbortReason::DomException(error) => error,
        reason => panic!("cwd must reject with an Error: {reason:?}"),
    };
    let value = native.value();
    json!({
        "ok": false,
        "error": {
            "name": native.name,
            "message": native.message.units(),
            "code": value.get("code"),
            "errno": value.get("errno"),
            "syscall": value.get("syscall"),
            "path": value.get("path").map(|path| path.as_string().unwrap().units()),
            "dest": value.get("dest").map(|path| path.as_string().unwrap().units()),
        },
    })
}

#[test]
fn process_cwd_reads_native_directory_after_removal_instead_of_nodes_cached_path() {
    let temp = tempfile::tempdir().unwrap();
    let mut source = oracle::TsOracle::spawn("process-cwd-oracle.mts", "packages/coding-agent/src");
    let mut cold_source_error = None;
    for warm in [false, true] {
        let directory = temp.path().join(if warm { "warm" } else { "cold" });
        std::fs::create_dir(&directory).unwrap();
        let expected = source.request(&json!({"directory":directory,"warm":warm}));
        assert_eq!(expected["ok"], warm, "source warm={warm}: {expected}");
        std::fs::create_dir(&directory).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "process_cwd_child", "--nocapture"])
            .env("PI_CWD_PROBE_DIRECTORY", &directory)
            .env("PI_CWD_PROBE_WARM", if warm { "1" } else { "0" })
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8(output.stderr).unwrap());
        let stdout = String::from_utf8(output.stdout).unwrap();
        let actual: Value = serde_json::from_str(
            stdout
                .lines()
                .find_map(|line| line.strip_prefix("CWD_PROBE:"))
                .expect("child must print cwd result"),
        )
        .unwrap();
        if warm {
            assert_eq!(
                expected,
                json!({"ok":true,"cwd":directory.to_str().unwrap().encode_utf16().collect::<Vec<_>>()})
            );
            assert_eq!(
                &actual,
                cold_source_error.as_ref().unwrap(),
                "native must read the OS, not Node's cached path"
            );
        } else {
            assert_eq!(actual, expected);
            cold_source_error = Some(expected);
        }
    }
}

#[test]
fn cold_process_cwd_native_error_survives_real_consumers() {
    let temp = tempfile::tempdir().unwrap();
    let mut source = oracle::TsOracle::spawn("process-cwd-oracle.mts", "packages/coding-agent/src");
    let reference = temp.path().join("reference");
    std::fs::create_dir(&reference).unwrap();
    let expected = source.request(&json!({"directory":reference,"warm":false}));
    assert_eq!(expected["ok"], false);
    for consumer in [
        "sdk",
        "settings",
        "startup",
        "executable",
        "restart",
        "architect",
        "worker",
    ] {
        let directory = temp.path().join(consumer);
        std::fs::create_dir(&directory).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "process_cwd_child", "--nocapture"])
            .env("PI_CWD_PROBE_DIRECTORY", directory)
            .env("PI_CWD_PROBE_WARM", "0")
            .env("PI_CWD_PROBE_CONSUMER", consumer)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "consumer {consumer}: {}",
            String::from_utf8(output.stderr).unwrap()
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        let actual: Value =
            serde_json::from_str(stdout.lines().find_map(|line| line.strip_prefix("CWD_PROBE:")).unwrap()).unwrap();
        assert_eq!(actual, expected, "consumer {consumer}");
    }
}

#[test]
fn absolute_project_spec_does_not_read_a_cold_deleted_process_cwd() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    std::fs::create_dir_all(project.join("docs/specs")).unwrap();
    std::fs::write(project.join("docs/specs/README.md"), "owned project spec").unwrap();
    let directory = temp.path().join("deleted");
    std::fs::create_dir(&directory).unwrap();
    let mut source = oracle::TsOracle::spawn("process-cwd-oracle.mts", "packages/coding-agent/src");
    let expected = source.request(&json!({"directory":directory,"warm":false,"project":project}));
    assert_eq!(expected, json!({"ok":true,"text":"owned project spec"}));
    std::fs::create_dir(&directory).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "process_cwd_child", "--nocapture"])
        .env("PI_CWD_PROBE_DIRECTORY", directory)
        .env("PI_CWD_PROBE_WARM", "0")
        .env("PI_CWD_PROBE_CONSUMER", "architect-absolute")
        .env("PI_CWD_PROBE_PROJECT", project)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8(output.stderr).unwrap());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let actual: Value =
        serde_json::from_str(stdout.lines().find_map(|line| line.strip_prefix("CWD_PROBE:")).unwrap()).unwrap();
    assert_eq!(actual, expected);
}

fn consumer_error(consumer: &str) -> pi_ai::request::CallbackError {
    use pi_coding_agent::core::settings_manager::SettingsManager;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    match consumer {
        "sdk" => runtime
            .block_on(pi_coding_agent::core::sdk::create_agent_session(Default::default()))
            .err()
            .expect("SDK must reject cold cwd"),
        "settings" => {
            pi_coding_agent::core::settings_manager::FileSettingsStorage::new(&"/project".into(), &"/agent".into())
                .unwrap_err()
                .into()
        }
        "startup" => runtime
            .block_on(pi_coding_agent::cli::startup_ui::load_startup_themes(
                &SettingsManager::in_memory(Default::default(), Default::default()),
            ))
            .unwrap_err(),
        "executable" => {
            let error = *pi_coding_agent::utils::executable::is_executable_available(
                &"cwd-probe-command".into(),
                &Default::default(),
            )
            .unwrap_err();
            pi_ai::request::CallbackError::from_error(error.message.clone(), error)
        }
        "restart" => runtime
            .block_on(pi_coding_agent::core::self_restart::spawn_self_restart(
                &pi_coding_agent::core::self_restart::SelfRestartRequest {
                    session_file: "/session.jsonl".into(),
                    prompt: None,
                    old_pid: None,
                },
                &pi_coding_agent::core::self_restart::SelfRestartDependencies::default(),
            ))
            .unwrap_err(),
        "architect" => runtime
            .block_on(pi_coding_agent::architect::project_spec::read_project_spec(
                "relative",
                "README.md",
            ))
            .unwrap_err()
            .into(),
        "worker" => pi_coding_agent::bundled_extensions::bwrap::worker::run_operation_json(
            "exists",
            &pi_js_compat::js_json!({}),
        )
        .unwrap_err(),
        _ => panic!("unknown consumer {consumer}"),
    }
}

#[test]
#[ignore = "isolated process cwd child, invoked by parent test"]
fn process_cwd_child() {
    // Raw fixture setup/restoration, not a ported process.cwd/chdir consumer.
    let original = std::env::current_dir().unwrap();
    let directory = std::env::var_os("PI_CWD_PROBE_DIRECTORY").unwrap();
    pi_js_compat::process_cwd::chdir(&pi_js_compat::node_fs::decode_node_path(std::ffi::OsStr::new(
        &(&directory),
    )))
    .unwrap();
    if std::env::var("PI_CWD_PROBE_WARM").unwrap() == "1" {
        SessionManager::in_memory(None, None).unwrap();
    }
    std::fs::remove_dir(&directory).unwrap();
    let result = if let Ok(consumer) = std::env::var("PI_CWD_PROBE_CONSUMER") {
        if consumer == "architect-absolute" {
            let project = std::env::var("PI_CWD_PROBE_PROJECT").unwrap();
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            match runtime.block_on(pi_coding_agent::architect::project_spec::read_project_spec(
                &project,
                "README.md",
            )) {
                Ok(text) => json!({"ok":true,"text":text}),
                Err(error) => error_wire(error.into()),
            }
        } else {
            error_wire(consumer_error(&consumer))
        }
    } else {
        match SessionManager::in_memory(None, None) {
            Ok(manager) => json!({"ok":true,"cwd":manager.get_cwd().units()}),
            Err(error) => error_wire(error.into()),
        }
    };
    pi_js_compat::process_cwd::chdir(&pi_js_compat::node_fs::decode_node_path(std::ffi::OsStr::new(
        &(original),
    )))
    .unwrap();
    println!("CWD_PROBE:{result}");
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
