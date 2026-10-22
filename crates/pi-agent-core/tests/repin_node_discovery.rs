#[path = "../../pi-ai/tests/support/oracle.rs"]
mod oracle;
use pi_agent_core::harness::env::{NodeExecutionEnv, NodeExecutionEnvOptions};
use pi_agent_core::harness::{Shell, ShellExecOptions};
use pi_js_compat::{js_json as json, js_json::JsString};
use std::os::unix::fs::PermissionsExt;

fn write_executable(path: &std::path::Path, source: &str) {
    std::fs::write(path, source).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[tokio::test]
async fn posix_shell_discovery_matches_frozen_in_private_mount_namespace() {
    const PROBE: &str = "PI_SHELL_DISCOVERY_PROBE";
    if let Some(root) = std::env::var_os(PROBE) {
        let root = std::path::PathBuf::from(root);
        assert!(!std::path::Path::new("/bin/bash").exists());
        let cwd = JsString::from(root.to_str().unwrap());
        let mut oracle = oracle::TsOracle::spawn(
            "agent-core-node-env-oracle.ts",
            "packages/agent-core/src/harness/env/nodejs.ts",
        );
        let expected = oracle.request(&json!({"kind":"shell-boundary","root":cwd,"command":"printf command"}));
        let native = NodeExecutionEnv::new(NodeExecutionEnvOptions {
            cwd,
            ..Default::default()
        });
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(7),
            native.exec(&"printf command".into(), ShellExecOptions::default()),
        )
        .await
        .expect("discovery timeout must settle, including non-group kill fallback")
        .unwrap();
        assert_eq!(
            json!({"ok":true,"value":{"stdout":result.stdout,"stderr":result.stderr,"exitCode":result.exit_code}}),
            expected
        );
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    for name in ["node", "env", "git", "sleep"] {
        std::fs::copy(format!("/usr/bin/{name}"), bin.join(name)).unwrap();
    }
    let support = root.path().join("support-shell");
    std::fs::copy("/usr/bin/sh", &support).unwrap();
    write_executable(
        &bin.join("sh"),
        &format!(
            "#!{}\nprintf fallback; exec {} \"$@\"\n",
            support.display(),
            support.display()
        ),
    );
    let which = bin.join("which");
    for mode in ["bom", "trailing-space", "found", "missing", "invalid", "timeout"] {
        let script = match mode {
            "found" => format!("printf '%s\\n' '{}'", support.display()),
            "missing" => "exit 1".into(),
            "invalid" => "printf '/does-not-exist\\n'".into(),
            "trailing-space" => format!("printf '%s \\nother\\n' '{}'", support.display()),
            "bom" => format!("printf '\\357\\273\\277%s\\n' '{}'", support.display()),
            "timeout" => format!("exec {}/sleep 60", root.path().join("bin").display()),
            _ => unreachable!(),
        };
        write_executable(&which, &format!("#!{}\n{script}\n", support.display()));
        let output = std::process::Command::new("bwrap")
            .args([
                "--die-with-parent",
                "--unshare-pid",
                "--ro-bind",
                "/",
                "/",
                "--dev-bind",
                "/dev",
                "/dev",
                "--proc",
                "/proc",
                "--bind",
            ])
            .arg(root.path())
            .arg(root.path())
            .arg("--ro-bind")
            .arg(&bin)
            .arg("/usr/bin")
            .args(["--setenv", "PATH", "/usr/bin", "--"])
            .arg(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "posix_shell_discovery_matches_frozen_in_private_mount_namespace",
                "--nocapture",
            ])
            .env(PROBE, root.path())
            .env("TMPDIR", root.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{mode}: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
