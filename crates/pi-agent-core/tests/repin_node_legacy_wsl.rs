#[path = "../../pi-ai/tests/support/oracle.rs"]
mod oracle;
use pi_agent_core::harness::env::{NodeExecutionEnv, NodeExecutionEnvOptions};
use pi_agent_core::harness::types::OperationFailure;
use pi_agent_core::harness::{Shell, ShellExecOptions};
use pi_js_compat::{
    js_json as json,
    js_json::{JsString, JsValue, parse_lossless, stringify_lossless},
};
use std::os::unix::fs::PermissionsExt;

#[tokio::test]
async fn legacy_wsl_native_child() {
    let Some(root) = std::env::var_os("PI_CORE_WSL_ROOT") else {
        return;
    };
    std::env::set_current_dir(&root).unwrap();
    let input = parse_lossless(JsString::from(std::fs::read_to_string("input.json").unwrap())).unwrap();
    let env = NodeExecutionEnv::new(NodeExecutionEnvOptions {
        cwd: std::env::current_dir().unwrap().to_str().unwrap().into(),
        shell_path: Some(input["shell"].as_string().unwrap().clone()),
        ..Default::default()
    });
    let command = input["command"].as_string().unwrap();
    let result = env
        .exec(
            command,
            ShellExecOptions {
                timeout: input["timeout"].as_f64(),
                ..Default::default()
            },
        )
        .await;
    let value = match result {
        Ok(result) => {
            json!({"ok":true,"value":{"stdout":result.stdout,"stderr":result.stderr,"exitCode":result.exit_code}})
        }
        Err(OperationFailure::Returned(error)) => {
            json!({"ok":false,"error":{"code":error.code.as_str(),"message":error.message}})
        }
        Err(OperationFailure::Rejected(error)) => panic!("unexpected rejected operation: {error:?}"),
    };
    std::fs::write("output.json", stringify_lossless(&value)).unwrap();
}

#[test]
fn legacy_wsl_path_transport_is_not_excluded_on_posix() {
    let mut oracle = oracle::TsOracle::spawn(
        "agent-core-node-env-oracle.ts",
        "packages/agent-core/src/harness/env/nodejs.ts",
    );
    for shell in [
        "C:/Windows/System32/bash.exe",
        "z:/windows/sysnative/BASH.exe",
        "K:/windows/system32/bash.exe",
        "C:/Windows/System32/not-bash.exe",
    ] {
        let cases = if shell.ends_with("not-bash.exe") {
            vec![(JsString::from("ordinary command"), "/bin/cat", None)]
        } else {
            vec![
                (JsString::from("ordinary command"), "/bin/cat", None),
                (JsString::from_utf16(vec![0x61, 0, 0xd800, 0x62]), "/bin/cat", None),
                (JsString::from("x".repeat(262144)), "/bin/cat", None),
                (JsString::from("x".repeat(262144)), "exit 0", None),
                (
                    JsString::from("x".repeat(262144)),
                    "(sleep 0.2) <&0 >/dev/null 2>&1 &\nexit 0",
                    Some(0.05),
                ),
            ]
        };
        for (command, tail, timeout) in cases {
            let root = tempfile::tempdir().unwrap();
            let executable = root.path().join(shell);
            std::fs::create_dir_all(executable.parent().unwrap()).unwrap();
            std::fs::write(&executable, format!("#!/bin/sh\nprintf '<%s>\\n' \"$@\"\n{tail}\n")).unwrap();
            std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
            let input = json!({"kind":"legacy-wsl","root":root.path(),"shellPath":shell,"shell":shell,"command":command,"timeout":timeout});
            let expected: JsValue = oracle.request(&input);
            std::fs::write(root.path().join("input.json"), stringify_lossless(&input)).unwrap();
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "legacy_wsl_native_child", "--nocapture"])
                .env("PI_CORE_WSL_ROOT", root.path())
                .output()
                .unwrap();
            assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
            let actual = parse_lossless(JsString::from(
                std::fs::read_to_string(root.path().join("output.json")).unwrap(),
            ))
            .unwrap();
            assert_eq!(actual, expected, "shell={shell}");
        }
    }
}
