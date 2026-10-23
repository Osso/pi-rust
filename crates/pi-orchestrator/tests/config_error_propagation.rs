use std::process::Command;

#[test]
fn default_startup_retains_the_original_configuration_failure() {
    let directory = tempfile::tempdir().unwrap();
    let library = directory.path().join("passwd-failure.so");
    let source =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../pi-coding-agent/tests/support/passwd_failure.c");
    let output = Command::new("cc")
        .args(["-shared", "-fPIC", "-o"])
        .arg(&library)
        .arg(source)
        .arg("-ldl")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "startup_failure_child", "--nocapture"])
        .env("LD_PRELOAD", library)
        .env("PI_TEST_FAIL_PASSWD", "1")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "version_failure_child", "--nocapture"])
        .env("LD_PRELOAD", directory.path().join("passwd-failure.so"))
        .env("PI_TEST_FAIL_PASSWD", "1")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        String::from_utf8(output.stderr).unwrap().contains(
            "SystemError [ERR_SYSTEM_ERROR]: A system error occurred: uv_os_homedir returned EIO (i/o error)"
        )
    );
    assert!(!String::from_utf8(output.stdout).unwrap().contains("DISPATCH_COMPLETED"));
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lock: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("docs/porting/source-lock.json")).unwrap()).unwrap();
    let source = std::path::Path::new(lock["worktree"].as_str().unwrap());
    let mut command = Command::new(source.join("node_modules/.bin/tsx"));
    command
        .arg("--tsconfig")
        .arg(source.join("tsconfig.json"))
        .arg(source.join("packages/orchestrator/src/cli.ts"))
        .arg("--version")
        .env("LD_PRELOAD", directory.path().join("passwd-failure.so"))
        .env("PI_TEST_FAIL_PASSWD", "1");
    for name in [
        "HOME",
        "XDG_CONFIG_HOME",
        "XDG_STATE_HOME",
        &pi_coding_agent::config::env_agent_dir(),
        &pi_coding_agent::config::env_state_dir(),
    ] {
        command.env_remove(name);
    }
    let output = command.output().unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        output.stdout.is_empty(),
        "import failure must precede version output: {output:?}"
    );
    assert!(
        String::from_utf8(output.stderr).unwrap().contains(
            "SystemError [ERR_SYSTEM_ERROR]: A system error occurred: uv_os_homedir returned EIO (i/o error)"
        )
    );
}

#[tokio::test]
#[ignore = "isolated forced-NSS startup child"]
async fn startup_failure_child() {
    use pi_js_compat::{abort::AbortReason, process_env};
    for name in [
        "HOME",
        "XDG_CONFIG_HOME",
        "XDG_STATE_HOME",
        &pi_coding_agent::config::env_agent_dir(),
        &pi_coding_agent::config::env_state_dir(),
    ] {
        process_env::remove(name);
    }
    let config = pi_orchestrator::config::Config {
        dir: "/unused-before-auth-failure".into(),
    };
    let error = match pi_orchestrator::serve::RunningOrchestrator::start_default(config).await {
        Ok(_) => panic!("startup must reject the failed home lookup"),
        Err(error) => error,
    };
    let AbortReason::Error(native) = error.thrown_value() else {
        panic!("configuration must reject an Error");
    };
    assert_eq!(native.constructor_class().name(), "SystemError");
    assert_eq!(native.value()["code"], pi_js_compat::js_json!("ERR_SYSTEM_ERROR"));
    assert_eq!(native.value()["info"]["code"], pi_js_compat::js_json!("EIO"));
    let source = error.original.unwrap();
    let config_error = source.downcast_ref::<pi_coding_agent::config::ConfigError>().unwrap();
    assert_eq!(config_error.native_error(), Some(&native));
}

#[tokio::test]
#[ignore = "isolated uncaught module-evaluation child"]
async fn version_failure_child() {
    pi_coding_agent::utils::uncaught::install_uncaught_handler();
    for name in [
        "HOME",
        "XDG_CONFIG_HOME",
        "XDG_STATE_HOME",
        &pi_coding_agent::config::env_agent_dir(),
        &pi_coding_agent::config::env_state_dir(),
    ] {
        pi_js_compat::process_env::remove(name);
    }
    pi_orchestrator::cli::run_cli(
        &["--version".into()],
        &"/unused".into(),
        pi_orchestrator::config::Config { dir: "/unused".into() },
        tokio::io::empty(),
        tokio::io::sink(),
        tokio::io::sink(),
    )
    .await;
    println!("DISPATCH_COMPLETED");
}
