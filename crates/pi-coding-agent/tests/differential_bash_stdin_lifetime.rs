use crate::shared_executable_script as executable_script;
use crate::shared_oracle as oracle;
use pi_coding_agent::core::tools::bash::{BashExecOptions, BashOperations, create_local_bash_operations};
use pi_js_compat::abort::AbortController;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsString, JsValue};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn run_child(root: &std::path::Path, path: &str, mode: &str) -> JsValue {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "stdin_write_child", "--nocapture"])
        .current_dir(root)
        .env("PI_STDIN_PATH", path)
        .env("PI_STDIN_MODE", mode)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            child.kill().unwrap();
            let output = child.wait_with_output().unwrap();
            panic!("stdin child timed out: {}", String::from_utf8_lossy(&output.stderr));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let text = String::from_utf8(output.stdout).unwrap();
    let line = text
        .lines()
        .find_map(|line| line.strip_prefix("STDIN_RESULT:"))
        .unwrap();
    js_json::parse_lossless(line).unwrap()
}
#[test]
fn stdin_backpressure_does_not_delay_timeout_or_abort() {
    let root = tempfile::tempdir().unwrap();
    let shell = "C:\\Windows\\System32\\bash.exe";
    executable_script::write_executable_script(
        &root.path().join(shell),
        "#!/bin/sh\nsleep .2; printf late; sleep .3\n",
    );
    let path = format!("{}:/usr/bin:/bin", root.path().display());
    let cases = vec![
        json!({"command":"x".repeat(200_000),"shellPath":shell,"timeout":0.05,"env":{"PATH":path}}),
        json!({"command":"x".repeat(200_000),"shellPath":shell,"abortMs":50,"env":{"PATH":path}}),
    ];
    let mut source = oracle::TsOracle::spawn("bash-local-oracle.ts", "packages/coding-agent/src/core/tools/bash.ts");
    let expected = source.request(&json!({"id":1,"cwd":root.path().to_str().unwrap(),"processCwd":true,"cases":cases}));
    assert_eq!(expected["ok"], json!(true), "{expected:?}");
    for (index, error) in ["timeout:0.05", "aborted"].into_iter().enumerate() {
        assert_eq!(
            expected["output"][index],
            json!({"result":{"ok":false,"error":error},"data":[],"artifactExists":false}),
            "{expected:?}"
        );
        assert_eq!(
            run_child(root.path(), &path, if index == 1 { "abort" } else { "timeout" }),
            expected["output"][index]
        );
    }
}
#[test]
fn posix_backslash_shell_names_follow_path_lookup() {
    let root = tempfile::tempdir().unwrap();
    let path_dir = root.path().join("on-path");
    std::fs::create_dir(&path_dir).unwrap();
    let shell = "back\\slash";
    executable_script::write_executable_script(&root.path().join(shell), "#!/bin/sh\nprintf from-cwd\n");
    executable_script::write_executable_script(&path_dir.join(shell), "#!/bin/sh\nprintf from-path\n");
    let path = format!("{}:/usr/bin:/bin", path_dir.display());
    let mut source = oracle::TsOracle::spawn("bash-local-oracle.ts", "packages/coding-agent/src/core/tools/bash.ts");
    let expected = source.request(&json!({"id":2,"cwd":root.path().to_str().unwrap(),"processCwd":true,"cases":[{"command":"probe","shellPath":shell,"env":{"PATH":path}}]}));
    assert_eq!(expected["ok"], json!(true), "{expected:?}");
    assert_eq!(
        expected["output"][0]["data"],
        json!(b"from-path".to_vec()),
        "{expected:?}"
    );
    assert_eq!(run_child(root.path(), &path, "path"), expected["output"][0]);
}
#[test]
#[ignore = "isolated real shell stdin process"]
fn stdin_write_child() {
    let mode = std::env::var("PI_STDIN_MODE").unwrap();
    let shell = JsString::from(if mode == "path" {
        "back\\slash"
    } else {
        "C:\\Windows\\System32\\bash.exe"
    });
    let command = if mode == "path" {
        "probe".into()
    } else {
        "x".repeat(200_000)
    };
    let cwd = pi_js_compat::process_cwd::cwd().unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let controller = AbortController::new();
    let abort = mode == "abort";
    let data = Arc::new(Mutex::new(Vec::new()));
    let capture = data.clone();
    let result = runtime.block_on(async {
        if abort {
            let controller = controller.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(50)).await;
                controller.abort(None);
            });
        }
        create_local_bash_operations(Some(&shell))
            .exec(
                &command.into(),
                &cwd,
                BashExecOptions {
                    on_data: Arc::new(move |bytes| {
                        capture.lock().unwrap().extend_from_slice(bytes);
                        Ok(())
                    }),
                    signal: Some(controller.signal()),
                    timeout: if mode == "timeout" { Some(0.05) } else { None },
                    env: Some(
                        [(
                            JsString::from("PATH"),
                            Some(JsString::from(std::env::var("PI_STDIN_PATH").unwrap())),
                        )]
                        .into(),
                    ),
                    detach: None,
                },
            )
            .await
    });
    let result = match result {
        Ok(value) => json!({"ok":true,"value":{"exitCode":value.exit_code}}),
        Err(error) => json!({"ok":false,"error":error.message}),
    };
    println!(
        "STDIN_RESULT:{}",
        js_json::stringify_lossless(
            &json!({"result":result,"data":data.lock().unwrap().clone(),"artifactExists":false})
        )
    );
}

// Shared support is compiled once per integration target.
#[path = "support/executable_script.rs"]
pub mod shared_executable_script;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
