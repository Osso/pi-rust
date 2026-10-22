#[path = "../../pi-ai/tests/support/oracle.rs"]
mod oracle;
use pi_agent_core::harness::env::{NodeExecutionEnv, NodeExecutionEnvOptions};
use pi_agent_core::harness::{CreateDirOptions, FileContent, FileSystem, RemoveOptions, Shell, ShellExecOptions};
use pi_js_compat::js_json::{JsString, JsValue, to_value};
use serde_json::{Value, json};
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
) -> Value {
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
