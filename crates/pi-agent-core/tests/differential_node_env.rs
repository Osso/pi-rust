#[path = "../../pi-ai/tests/support/oracle.rs"]
mod oracle;
use pi_agent_core::harness::env::{NodeExecutionEnv, NodeExecutionEnvOptions};
use pi_agent_core::harness::{CreateDirOptions, FileContent, FileSystem, RemoveOptions, Shell, ShellExecOptions};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{JsString, JsValue, to_value};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

fn oracle() -> oracle::TsOracle {
    oracle::TsOracle::spawn(
        "agent-core-node-env-oracle.ts",
        "packages/agent-core/src/harness/env/nodejs.ts",
    )
}
fn env(root: &str) -> Arc<NodeExecutionEnv> {
    Arc::new(NodeExecutionEnv::new(NodeExecutionEnvOptions {
        cwd: root.into(),
        shell_env: [("CORE_CONFIG".into(), "configured".into())].into(),
        ..Default::default()
    }))
}
fn shell_result(
    result: Result<pi_agent_core::harness::ShellExecResult, pi_agent_core::harness::ExecutionError>,
) -> JsValue {
    match result {
        Ok(value) => {
            json!({"ok":true,"value":{"stdout":value.stdout,"stderr":value.stderr,"exitCode":value.exit_code}})
        }
        Err(error) => json!({"ok":false,"error":{"code":error.code.as_str(),"message":error.message}}),
    }
}
#[tokio::test]
async fn node_env_paths_match_frozen_posix_literals() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    let paths = ["~", "~/child", "file:///tmp/a%20b", "/a/../b//", "nested/../x", ""];
    let joins = vec![
        vec!["a", "b/"],
        vec![".", ""],
        vec!["a", "..", ""],
        vec!["", ""],
        vec!["/a", "/b"],
        vec!["/", ".."],
    ];
    let expected = oracle().request(&json!({"kind":"paths","root":root,"paths":paths,"joins":joins}));
    let env = env(root);
    let mut actual = Vec::new();
    for path in paths {
        actual.push(json!({"ok":true,"value":env.absolute_path(&path.into(),None).await.unwrap()}));
    }
    for parts in joins {
        actual.push(json!({"ok":true,"value":env.join_path(&parts.into_iter().map(Into::into).collect::<Vec<_>>(),None).await.unwrap()}));
    }
    assert_eq!(actual, expected.as_array().unwrap().clone());
}
#[tokio::test]
async fn node_env_cleanup_is_noop_and_exec_waits_for_inherited_pipes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    let mut oracle = oracle();
    for (kind, command) in [
        ("tail", "(sleep 0.25; printf late) & printf early"),
        ("cleanup", "printf ready; sleep 0.2; printf done"),
    ] {
        let expected = oracle.request(&json!({"kind":kind,"root":root}));
        let env = env(root);
        let (notify, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let options = ShellExecOptions {
            on_stdout: Some(Arc::new(move |_| {
                let _ = notify.send(());
                Ok(())
            })),
            ..Default::default()
        };
        let runner = env.clone();
        let task = tokio::spawn(async move { runner.exec(&command.into(), options).await });
        receiver.recv().await.unwrap();
        if kind == "cleanup" {
            env.cleanup().await;
        }
        let result = tokio::time::timeout(std::time::Duration::from_secs(3), task)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(shell_result(result), expected, "{kind}");
    }
}
#[tokio::test]
async fn node_env_exec_environment_and_spawn_failures_match_frozen() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    let env = env(root);
    let mut oracle = oracle();
    for kind in ["env", "missing-cwd"] {
        let expected = oracle.request(&json!({"kind":kind,"root":root}));
        let options = if kind == "env" {
            ShellExecOptions {
                env: Some([("CORE_EXTRA".into(), "extra".into())].into()),
                ..Default::default()
            }
        } else {
            ShellExecOptions {
                cwd: Some("missing".into()),
                ..Default::default()
            }
        };
        let command = if kind == "env" {
            "printf '%s:%s' \"$CORE_CONFIG\" \"$CORE_EXTRA\""
        } else {
            "printf never"
        };
        assert_eq!(
            shell_result(env.exec(&command.into(), options).await),
            expected,
            "{kind}"
        );
    }
}
#[tokio::test]
async fn node_concrete_ignores_extra_abort_arguments() {
    let native = tempfile::tempdir().unwrap();
    let reference = tempfile::tempdir().unwrap();
    let env = env(native.path().to_str().unwrap());
    let expected = oracle().request(&json!({"kind":"ignored-abort","root":reference.path()}));
    let token = CancellationToken::new();
    token.cancel();
    let path = JsString::from("nested/file");
    let content = JsString::from("a");
    env.append_file(&path, FileContent::Text(&content), Some(&token))
        .await
        .unwrap();
    let exists = env.exists(&path, Some(&token)).await.unwrap();
    env.create_dir(&"extra".into(), CreateDirOptions::default())
        .await
        .unwrap();
    env.remove(
        &"extra".into(),
        RemoveOptions {
            recursive: Some(true),
            force: None,
        },
    )
    .await
    .unwrap();
    let text = env.read_text_file(&path, None).await.unwrap();
    let actual = json!([{"ok":true},{"ok":true,"value":exists},{"ok":true},{"ok":true},{"ok":true,"value":text}]);
    assert_eq!(actual, expected);
    // The native JSON authority must not change the external result shape.
    assert_eq!(to_value(&text).unwrap(), JsValue::String("a".into()));
}

#[tokio::test]
async fn post_exit_pipe_wait_keeps_timeout_abort_and_callback_live() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    let env = env(root);
    let mut oracle = oracle();
    for mode in ["timeout", "abort", "callback"] {
        let expected = oracle.request(&json!({"kind":"post-exit","root":root,"mode":mode}));
        let abort = CancellationToken::new();
        let signal = abort.clone();
        let mode_string = mode.to_owned();
        let options = ShellExecOptions {
            timeout: (mode == "timeout").then_some(0.05),
            abort: (mode == "abort").then_some(abort),
            on_stdout: Some(Arc::new(move |chunk| {
                if mode_string == "abort" && chunk.contains("early") {
                    let signal = signal.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                        signal.cancel();
                    });
                }
                if mode_string == "callback" && chunk.contains("late") {
                    return Err(JsString::from("late") + JsString::from_utf16(vec![0xd800]) + "failure");
                }
                Ok(())
            })),
            ..Default::default()
        };
        let actual = shell_result(
            env.exec(&"(sleep 0.25; printf late) & printf early".into(), options)
                .await,
        );
        assert_eq!(actual, expected, "{mode}");
    }
}

#[tokio::test]
async fn relative_env_paths_decode_raw_process_cwd_like_node() {
    const PROBE: &str = "PI_RAW_CWD_PROBE";
    if std::env::var_os(PROBE).is_some() {
        let env = env("base");
        let actual = env.absolute_path(&"child".into(), None).await.unwrap();
        let output = std::process::Command::new("node")
            .args([
                "--eval",
                "console.log(JSON.stringify({cwd:process.cwd(),path:require('node:path').resolve('base','child')}))",
            ])
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let expected = pi_js_compat::js_json::parse_lossless(std::str::from_utf8(&output.stdout).unwrap()).unwrap();
        assert_eq!(actual, expected["path"].as_string().unwrap().clone());
        assert_eq!(
            pi_js_compat::node_fs::decode_node_path(std::env::current_dir().unwrap().as_os_str()),
            expected["cwd"].as_string().unwrap().clone()
        );
        assert!(actual.units().contains(&0xfffd));
        return;
    }
    use std::os::unix::ffi::OsStringExt;
    let root = tempfile::tempdir().unwrap();
    let raw = root
        .path()
        .join(std::ffi::OsString::from_vec(b"cwd-\xff-\xed\xa0\x80".to_vec()));
    std::fs::create_dir(&raw).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "relative_env_paths_decode_raw_process_cwd_like_node",
            "--nocapture",
        ])
        .env(PROBE, "1")
        .current_dir(&raw)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
