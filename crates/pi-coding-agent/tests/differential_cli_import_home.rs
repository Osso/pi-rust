use std::{path::Path, process::Command};

#[test]
fn cli_import_failure_precedes_version_dispatch_and_preserves_native_error() {
    let directory = tempfile::tempdir().unwrap();
    let library = directory.path().join("passwd-failure.so");
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let compiled = Command::new("cc")
        .args(["-shared", "-fPIC", "-o"])
        .arg(&library)
        .arg(manifest.join("tests/support/passwd_failure.c"))
        .arg("-ldl")
        .output()
        .unwrap();
    assert!(compiled.status.success(), "{compiled:?}");
    let lock: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(manifest.join("../../docs/porting/source-lock.json")).unwrap())
            .unwrap();
    let source = Path::new(lock["worktree"].as_str().unwrap());
    let mut frozen = Command::new(source.join("node_modules/.bin/tsx"));
    frozen
        .arg("--tsconfig")
        .arg(source.join("tsconfig.json"))
        .arg(source.join("packages/coding-agent/src/cli.ts"));
    let mut native = Command::new(env!("CARGO_BIN_EXE_pi"));
    for command in [&mut frozen, &mut native] {
        command
            .arg("--version")
            .env("LD_PRELOAD", &library)
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
    }
    let signature = "SystemError [ERR_SYSTEM_ERROR]: A system error occurred: uv_os_homedir returned EIO (i/o error)";
    for mut command in [frozen, native] {
        let output = command.output().unwrap();
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        assert!(
            output.stdout.is_empty(),
            "import failure must precede version: {output:?}"
        );
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(stderr.contains(signature), "{stderr}");
        assert!(!stderr.contains("Error: A system error occurred"), "{stderr}");
    }
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "module_home_snapshot_child", "--nocapture"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
}

#[test]
#[ignore = "isolated module-state snapshot child"]
fn module_home_snapshot_child() {
    use pi_coding_agent::bundled_extensions::agents_core::runtime::default_browser_agent_config_root;
    pi_js_compat::process_env::set("HOME", "/initial-home");
    assert_eq!(
        default_browser_agent_config_root().unwrap(),
        "/initial-home/AgentConfig"
    );
    pi_js_compat::process_env::set("HOME", "/later-home");
    assert_eq!(
        default_browser_agent_config_root().unwrap(),
        "/initial-home/AgentConfig"
    );
}
