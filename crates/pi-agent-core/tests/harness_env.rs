//! Behavioral port of `packages/agent/test/harness/nodejs-env.test.ts`.

use crate::shared_operation as operation;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::encode_node_utf8;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use pi_agent_core::harness::env::{NodeExecutionEnv, NodeExecutionEnvOptions};
use pi_agent_core::harness::types::{
    CreateDirOptions, CreateTempFileOptions, ExecutionErrorCode, FileContent, FileErrorCode, FileKind, FileSystem,
    ReadTextLinesOptions, RemoveOptions, Shell, ShellExecOptions,
};
use tempfile::TempDir;

fn temp_dir() -> TempDir {
    tempfile::tempdir().expect("test temp dir must be created")
}

fn env_at(root: &Path) -> NodeExecutionEnv {
    NodeExecutionEnv::new(NodeExecutionEnvOptions {
        cwd: JsString::from(root.to_str().expect("utf-8 test path")),
        shell_path: None,
        shell_env: Default::default(),
    })
}

fn text(value: &JsString) -> FileContent<'_> {
    FileContent::Text(value)
}

#[tokio::test]
async fn reads_writes_lists_and_removes_files_and_directories() {
    let root = temp_dir();
    let env = env_at(root.path());

    assert_eq!(
        env.absolute_path(&JsString::from("nested/child"), None).await.unwrap(),
        root.path().join("nested/child").to_str().expect("utf-8 test path")
    );
    assert_eq!(
        env.join_path(
            &[
                JsString::from(root.path().to_str().expect("utf-8 test path")),
                JsString::from("nested"),
                JsString::from("child")
            ],
            None
        )
        .await
        .unwrap(),
        root.path().join("nested/child").to_str().expect("utf-8 test path")
    );

    env.create_dir(
        &JsString::from("nested/child"),
        CreateDirOptions { recursive: Some(true) },
    )
    .await
    .unwrap();
    env.write_file(
        &JsString::from("nested/child/file.txt"),
        text(&JsString::from("hel")),
        None,
    )
    .await
    .unwrap();
    env.append_file(
        &JsString::from("nested/child/file.txt"),
        text(&JsString::from("lo")),
        None,
    )
    .await
    .unwrap();
    assert_eq!(
        env.read_text_file(&JsString::from("nested/child/file.txt"), None)
            .await
            .unwrap(),
        "hello"
    );
    assert_eq!(
        env.read_text_lines(
            &JsString::from("nested/child/file.txt"),
            ReadTextLinesOptions::default()
        )
        .await
        .unwrap(),
        vec!["hello".to_owned()]
    );
    assert_eq!(
        env.read_binary_file(&JsString::from("nested/child/file.txt"), None)
            .await
            .unwrap(),
        b"hello".to_vec()
    );

    let entries = env.list_dir(&JsString::from("nested/child"), None).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "file.txt");
    assert_eq!(
        entries[0].path,
        root.path()
            .join("nested/child/file.txt")
            .to_str()
            .expect("utf-8 test path")
    );
    assert_eq!(entries[0].kind, FileKind::File);
    assert_eq!(entries[0].size, 5.0);
    assert!(entries[0].mtime_ms > 0.0);

    assert!(
        env.exists(&JsString::from("nested/child/file.txt"), None)
            .await
            .unwrap()
    );
    env.remove(&JsString::from("nested/child/file.txt"), RemoveOptions::default())
        .await
        .unwrap();
    assert!(
        !env.exists(&JsString::from("nested/child/file.txt"), None)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn reports_file_kinds_without_following_symlinks() {
    let root = temp_dir();
    let env = env_at(root.path());
    env.create_dir(&JsString::from("dir"), CreateDirOptions::default())
        .await
        .unwrap();
    env.write_file(&JsString::from("dir/file.txt"), text(&JsString::from("hello")), None)
        .await
        .unwrap();
    symlink(root.path().join("dir/file.txt"), root.path().join("file-link")).unwrap();
    symlink(root.path().join("dir"), root.path().join("dir-link")).unwrap();

    let dir = env.file_info(&JsString::from("dir")).await.unwrap();
    assert_eq!(
        (dir.name.clone(), dir.kind),
        (JsString::from("dir"), FileKind::Directory)
    );
    let file = env.file_info(&JsString::from("dir/file.txt")).await.unwrap();
    assert_eq!(
        (file.name.clone(), file.kind, file.size),
        (JsString::from("file.txt"), FileKind::File, 5.0)
    );
    assert_eq!(
        env.file_info(&JsString::from("file-link")).await.unwrap().kind,
        FileKind::Symlink
    );
    assert_eq!(
        env.file_info(&JsString::from("dir-link")).await.unwrap().kind,
        FileKind::Symlink
    );
    assert_eq!(
        env.canonical_path(&JsString::from("file-link")).await.unwrap(),
        std::fs::canonicalize(root.path().join("dir/file.txt"))
            .unwrap()
            .to_str()
            .expect("utf-8 test path")
    );
}

#[tokio::test]
async fn lists_symlinks_as_symlinks() {
    let root = temp_dir();
    let env = env_at(root.path());
    env.write_file(&JsString::from("target.txt"), text(&JsString::from("hello")), None)
        .await
        .unwrap();
    symlink(root.path().join("target.txt"), root.path().join("link.txt")).unwrap();

    let mut entries = env
        .list_dir(&JsString::from("."), None)
        .await
        .unwrap()
        .into_iter()
        .map(|entry| (entry.name, entry.kind))
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    assert_eq!(
        entries,
        vec![
            (JsString::from("link.txt"), FileKind::Symlink),
            (JsString::from("target.txt"), FileKind::File)
        ]
    );
}

#[tokio::test]
async fn stops_reading_text_lines_at_the_requested_limit() {
    let root = temp_dir();
    let env = env_at(root.path());
    env.write_file(
        &JsString::from("file.txt"),
        text(&JsString::from("one\ntwo\nthree")),
        None,
    )
    .await
    .unwrap();
    assert_eq!(
        env.read_text_lines(
            &JsString::from("file.txt"),
            ReadTextLinesOptions {
                max_lines: Some(1.0),
                abort: None
            }
        )
        .await
        .unwrap(),
        vec!["one".to_owned()]
    );
    assert_eq!(
        env.read_text_lines(
            &JsString::from("file.txt"),
            ReadTextLinesOptions {
                max_lines: Some(0.0),
                abort: None
            }
        )
        .await
        .unwrap(),
        Vec::<String>::new()
    );
}

#[tokio::test]
async fn returns_file_errors_for_missing_paths_and_listing_non_directories() {
    let root = temp_dir();
    let env = env_at(root.path());
    let missing = env
        .file_info(&JsString::from("missing.txt"))
        .await
        .map_err(operation::returned_error)
        .unwrap_err();
    assert_eq!(missing.name, "FileError");
    assert_eq!(missing.code, FileErrorCode::NotFound);
    assert_eq!(
        missing.path.as_ref(),
        Some(JsString::from(
            root.path().join("missing.txt").to_str().expect("utf-8 test path")
        ))
        .as_ref()
    );
    assert_eq!(
        missing.message,
        format!(
            "ENOENT: no such file or directory, lstat '{}'",
            root.path().join("missing.txt").display()
        )
    );
    assert!(!env.exists(&JsString::from("missing.txt"), None).await.unwrap());

    env.write_file(&JsString::from("file.txt"), text(&JsString::from("hello")), None)
        .await
        .unwrap();
    let error = env
        .list_dir(&JsString::from("file.txt"), None)
        .await
        .map_err(operation::returned_error)
        .unwrap_err();
    assert_eq!(error.code, FileErrorCode::NotDirectory);
    assert_eq!(
        error.message,
        format!(
            "ENOTDIR: not a directory, scandir '{}'",
            root.path().join("file.txt").display()
        )
    );
}

#[tokio::test]
async fn appends_to_new_files_and_creates_parent_directories() {
    let root = temp_dir();
    let env = env_at(root.path());
    env.append_file(&JsString::from("new/nested/file.txt"), text(&JsString::from("a")), None)
        .await
        .unwrap();
    env.append_file(&JsString::from("new/nested/file.txt"), text(&JsString::from("b")), None)
        .await
        .unwrap();
    assert_eq!(
        env.read_text_file(&JsString::from("new/nested/file.txt"), None)
            .await
            .unwrap(),
        "ab"
    );
}

#[tokio::test]
async fn creates_temporary_directories_and_files() {
    let root = temp_dir();
    let env = env_at(root.path());
    let directory = env.create_temp_dir(&JsString::from("node-env-test-")).await.unwrap();
    assert!(Path::new(&encode_node_utf8(&directory)).is_dir());
    let file = env
        .create_temp_file(CreateTempFileOptions {
            prefix: Some(JsString::from("prefix-")),
            suffix: Some(JsString::from(".txt")),
            abort: None,
        })
        .await
        .unwrap();
    assert!(Path::new(&encode_node_utf8(&file)).is_file());
    assert!(file.ends_with(".txt"));
    assert!(
        Path::new(&encode_node_utf8(&file))
            .file_name()
            .unwrap()
            .to_str()
            .expect("utf-8 test path")
            .starts_with("prefix-")
    );
}

#[tokio::test]
async fn honors_create_and_remove_options() {
    let root = temp_dir();
    let env = env_at(root.path());
    let error = env
        .create_dir(
            &JsString::from("missing/child"),
            CreateDirOptions { recursive: Some(false) },
        )
        .await
        .map_err(operation::returned_error)
        .unwrap_err();
    assert_eq!(error.code, FileErrorCode::NotFound);

    env.write_file(
        &JsString::from("dir/child/file.txt"),
        text(&JsString::from("hello")),
        None,
    )
    .await
    .unwrap();
    let error = env
        .remove(
            &JsString::from("dir"),
            RemoveOptions {
                recursive: Some(false),
                force: Some(false),
            },
        )
        .await
        .map_err(operation::returned_error)
        .unwrap_err();
    assert_eq!(error.code, FileErrorCode::Unknown);
    env.remove(
        &JsString::from("dir"),
        RemoveOptions {
            recursive: Some(true),
            force: Some(false),
        },
    )
    .await
    .unwrap();
    assert!(!env.exists(&JsString::from("dir"), None).await.unwrap());

    let error = env
        .remove(
            &JsString::from("missing"),
            RemoveOptions {
                recursive: Some(false),
                force: Some(false),
            },
        )
        .await
        .map_err(operation::returned_error)
        .unwrap_err();
    assert_eq!(error.code, FileErrorCode::NotFound);
    env.remove(
        &JsString::from("missing"),
        RemoveOptions {
            recursive: Some(false),
            force: Some(true),
        },
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn returns_aborted_results_for_pre_aborted_cancellable_operations() {
    let root = temp_dir();
    let env = env_at(root.path());
    env.write_file(&JsString::from("file.txt"), text(&JsString::from("hello")), None)
        .await
        .unwrap();
    let controller_signal = pi_js_compat::abort::AbortController::new();
    let signal = controller_signal.signal();
    controller_signal.abort(None);

    let join_arg_0 = JsString::from("file.txt");
    let join_arg_1 = JsString::from("file.txt");
    let join_arg_2 = JsString::from("file.txt");
    let join_arg_3 = JsString::from("other.txt");
    let join_arg_4 = JsString::from("x");
    let join_arg_7 = JsString::from(".");
    let results = tokio::join!(
        env.read_text_file(&join_arg_0, Some(&signal)),
        env.read_text_lines(
            &join_arg_1,
            ReadTextLinesOptions {
                max_lines: None,
                abort: Some(signal.clone())
            }
        ),
        env.read_binary_file(&join_arg_2, Some(&signal)),
        env.write_file(&join_arg_3, text(&join_arg_4), Some(&signal)),
        env.list_dir(&join_arg_7, Some(&signal)),
    );
    for code in [
        results.0.map_err(operation::returned_error).unwrap_err().code,
        results.1.map_err(operation::returned_error).unwrap_err().code,
        results.2.map_err(operation::returned_error).unwrap_err().code,
        results.3.map_err(operation::returned_error).unwrap_err().code,
        results.4.map_err(operation::returned_error).unwrap_err().code,
    ] {
        assert_eq!(code, FileErrorCode::Aborted);
    }
}

#[tokio::test]
async fn executes_commands_in_cwd_with_environment_overrides() {
    let root = temp_dir();
    let env = env_at(root.path());
    let result = env
        .exec(
            &JsString::from("printf '%s:%s' \"$PWD\" \"$NODE_ENV_TEST\""),
            ShellExecOptions {
                env: Some([("NODE_ENV_TEST".into(), "ok".into())].into()),
                ..ShellExecOptions::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(
        result,
        pi_agent_core::harness::types::ShellExecResult {
            stdout: format!("{}:ok", std::fs::canonicalize(root.path()).unwrap().display()),
            stderr: String::new(),
            exit_code: 0,
        }
    );
}

#[tokio::test]
async fn applies_shell_environment_overrides() {
    let root = temp_dir();
    let mut shell_env = indexmap::IndexMap::new();
    shell_env.insert("PI_SESSION_FILE".into(), Some("/stale/parent.jsonl".into()));
    shell_env.insert("PI_CODING_AGENT".into(), Some("true".into()));
    let env = NodeExecutionEnv::new(NodeExecutionEnvOptions {
        cwd: JsString::from(root.path().to_str().expect("utf-8 test path")),
        shell_path: None,
        shell_env,
    });

    let missing = env
        .exec(
            &JsString::from("printf '%s:%s|%s' \"${PI_SESSION_FILE+x}\" \"${PI_SESSION_FILE-}\" \"$PI_CODING_AGENT\""),
            ShellExecOptions::default(),
        )
        .await
        .unwrap();
    assert_eq!(missing.stdout, "x:/stale/parent.jsonl|true");

    let empty = env
        .exec(
            &JsString::from("printf '%s:%s|%s' \"${PI_SESSION_FILE+x}\" \"${PI_SESSION_FILE-}\" \"$PI_CODING_AGENT\""),
            ShellExecOptions {
                env: Some([("PI_SESSION_FILE".into(), JsString::default())].into()),
                ..ShellExecOptions::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(empty.stdout, "x:|true");
}

#[tokio::test]
async fn streams_stdout_and_stderr_chunks() {
    let root = temp_dir();
    let env = env_at(root.path());
    let stdout = Arc::new(std::sync::Mutex::new(String::new()));
    let stderr = Arc::new(std::sync::Mutex::new(String::new()));
    let out_sink = Arc::clone(&stdout);
    let err_sink = Arc::clone(&stderr);
    let result = env
        .exec(
            &JsString::from("printf out; printf err >&2"),
            ShellExecOptions {
                on_stdout: Some(Arc::new(move |chunk: &str| {
                    out_sink.lock().unwrap().push_str(chunk);
                    Ok(())
                })),
                on_stderr: Some(Arc::new(move |chunk: &str| {
                    err_sink.lock().unwrap().push_str(chunk);
                    Ok(())
                })),
                ..ShellExecOptions::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(result.stdout, "out");
    assert_eq!(result.stderr, "err");
    assert_eq!(*stdout.lock().unwrap(), "out");
    assert_eq!(*stderr.lock().unwrap(), "err");
}

#[tokio::test]
async fn returns_non_zero_exit_codes_and_timeout_errors() {
    let sleep_command = JsString::from("sleep 5");
    let root = temp_dir();
    let env = env_at(root.path());
    let result = env
        .exec(&JsString::from("exit 7"), ShellExecOptions::default())
        .await
        .unwrap();
    assert_eq!(result.exit_code, 7);

    let error = env
        .exec(
            &sleep_command,
            ShellExecOptions {
                timeout: Some(0.01),
                ..ShellExecOptions::default()
            },
        )
        .await
        .map_err(operation::returned_error)
        .unwrap_err();
    assert_eq!(error.code, ExecutionErrorCode::Timeout);
    assert_eq!(error.message, "timeout:0.01");

    let invalid = env
        .exec(
            &JsString::from("printf ok"),
            ShellExecOptions {
                timeout: Some(0.0),
                ..ShellExecOptions::default()
            },
        )
        .await
        .map_err(operation::returned_error)
        .unwrap_err();
    assert_eq!(invalid.code, ExecutionErrorCode::Timeout);
    assert_eq!(invalid.message, "Invalid timeout: must be a finite number of seconds");
}

#[tokio::test]
async fn returns_callback_errors_from_stream_handlers() {
    let root = temp_dir();
    let env = env_at(root.path());
    let error = env
        .exec(
            &JsString::from("printf out"),
            ShellExecOptions {
                on_stdout: Some(Arc::new(|_: &str| Err(JsString::from("callback failed")))),
                ..ShellExecOptions::default()
            },
        )
        .await
        .map_err(operation::returned_error)
        .unwrap_err();
    assert_eq!(error.code, ExecutionErrorCode::CallbackError);
    assert_eq!(error.message, "callback failed");
}

#[tokio::test]
async fn returns_shell_unavailable_and_spawn_errors() {
    let root = temp_dir();
    let missing_shell = root.path().join("missing-shell");
    let env = NodeExecutionEnv::new(NodeExecutionEnvOptions {
        cwd: JsString::from(root.path().to_str().expect("utf-8 test path")),
        shell_path: Some(missing_shell.to_str().expect("utf-8 test path").into()),
        shell_env: Default::default(),
    });
    let error = env
        .exec(&JsString::from("printf ok"), ShellExecOptions::default())
        .await
        .map_err(operation::returned_error)
        .unwrap_err();
    assert_eq!(error.code, ExecutionErrorCode::ShellUnavailable);
    assert_eq!(
        error.message,
        format!("Custom shell path not found: {}", missing_shell.display())
    );

    let shell_path = root.path().join("not-executable-shell");
    env.write_file(
        &JsString::from("not-executable-shell"),
        text(&JsString::from("not executable")),
        None,
    )
    .await
    .unwrap();
    let env = NodeExecutionEnv::new(NodeExecutionEnvOptions {
        cwd: JsString::from(root.path().to_str().expect("utf-8 test path")),
        shell_path: Some(shell_path.to_str().expect("utf-8 test path").into()),
        shell_env: Default::default(),
    });
    let error = env
        .exec(&JsString::from("printf ok"), ShellExecOptions::default())
        .await
        .map_err(operation::returned_error)
        .unwrap_err();
    assert_eq!(error.code, ExecutionErrorCode::SpawnError);
}

#[tokio::test]
async fn returns_an_aborted_result_for_aborted_commands() {
    let sleep_command = JsString::from("sleep 5");
    let root = temp_dir();
    let env = env_at(root.path());
    let controller_signal = pi_js_compat::abort::AbortController::new();
    let signal = controller_signal.signal();
    let execution = env.exec(
        &sleep_command,
        ShellExecOptions {
            abort: Some(signal.clone()),
            ..ShellExecOptions::default()
        },
    );
    controller_signal.abort(None);
    let error = execution.await.map_err(operation::returned_error).unwrap_err();
    assert_eq!(error.code, ExecutionErrorCode::Aborted);
    assert_eq!(error.message, "aborted");
}

#[tokio::test]
async fn cleanup_is_best_effort() {
    let root = temp_dir();
    let env = env_at(root.path());
    env.cleanup().await;
}

#[tokio::test]
async fn abort_does_not_run_callbacks_after_settlement() {
    let root = temp_dir();
    let env = env_at(root.path());
    let called = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&called);
    env.exec(
        &JsString::from("printf out"),
        ShellExecOptions {
            on_stdout: Some(Arc::new(move |_: &str| {
                flag.store(true, Ordering::SeqCst);
                Ok(())
            })),
            ..ShellExecOptions::default()
        },
    )
    .await
    .unwrap();
    assert!(called.load(Ordering::SeqCst));
}
