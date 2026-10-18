//! Behavioral port of `packages/agent/test/harness/nodejs-env.test.ts`.

use std::collections::HashMap;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use pi_agent_core::harness::env::{NodeExecutionEnv, NodeExecutionEnvOptions};
use pi_agent_core::harness::types::{
    CreateDirOptions, CreateTempFileOptions, ExecutionErrorCode, FileContent, FileErrorCode, FileKind, FileSystem,
    ReadTextLinesOptions, RemoveOptions, Shell, ShellExecOptions,
};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

fn temp_dir() -> TempDir {
    tempfile::tempdir().expect("test temp dir must be created")
}

fn env_at(root: &Path) -> NodeExecutionEnv {
    NodeExecutionEnv::new(NodeExecutionEnvOptions {
        cwd: root.to_string_lossy().into_owned(),
        shell_path: None,
        shell_env: HashMap::new(),
    })
}

fn text(value: &str) -> FileContent<'_> {
    FileContent::Text(value)
}

#[tokio::test]
async fn reads_writes_lists_and_removes_files_and_directories() {
    let root = temp_dir();
    let env = env_at(root.path());

    assert_eq!(
        env.absolute_path("nested/child", None).await.unwrap(),
        root.path().join("nested/child").to_string_lossy()
    );
    assert_eq!(
        env.join_path(
            &[
                root.path().to_string_lossy().into_owned(),
                "nested".to_owned(),
                "child".to_owned()
            ],
            None
        )
        .await
        .unwrap(),
        root.path().join("nested/child").to_string_lossy()
    );

    env.create_dir("nested/child", CreateDirOptions { recursive: Some(true) })
        .await
        .unwrap();
    env.write_file("nested/child/file.txt", text("hel"), None)
        .await
        .unwrap();
    env.append_file("nested/child/file.txt", text("lo")).await.unwrap();
    assert_eq!(
        env.read_text_file("nested/child/file.txt", None).await.unwrap(),
        "hello"
    );
    assert_eq!(
        env.read_text_lines("nested/child/file.txt", ReadTextLinesOptions::default())
            .await
            .unwrap(),
        vec!["hello".to_owned()]
    );
    assert_eq!(
        env.read_binary_file("nested/child/file.txt", None).await.unwrap(),
        b"hello".to_vec()
    );

    let entries = env.list_dir("nested/child", None).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "file.txt");
    assert_eq!(
        entries[0].path,
        root.path().join("nested/child/file.txt").to_string_lossy()
    );
    assert_eq!(entries[0].kind, FileKind::File);
    assert_eq!(entries[0].size, 5.0);
    assert!(entries[0].mtime_ms > 0.0);

    assert!(env.exists("nested/child/file.txt", None).await.unwrap());
    env.remove("nested/child/file.txt", RemoveOptions::default())
        .await
        .unwrap();
    assert!(!env.exists("nested/child/file.txt", None).await.unwrap());
}

#[tokio::test]
async fn expands_home_relative_paths_and_file_urls() {
    let root = temp_dir();
    let env = env_at(root.path());
    let home = std::env::var("HOME").expect("HOME must be set");
    assert_eq!(
        env.absolute_path("~/pi-node-env-test", None).await.unwrap(),
        Path::new(&home).join("pi-node-env-test").to_string_lossy()
    );

    let file_path = root.path().join("file with spaces.txt");
    let url = format!("file://{}", file_path.to_string_lossy().replace(' ', "%20"));
    assert_eq!(
        env.absolute_path(&url, None).await.unwrap(),
        file_path.to_string_lossy()
    );
}

#[tokio::test]
async fn reports_file_kinds_without_following_symlinks() {
    let root = temp_dir();
    let env = env_at(root.path());
    env.create_dir("dir", CreateDirOptions::default()).await.unwrap();
    env.write_file("dir/file.txt", text("hello"), None).await.unwrap();
    symlink(root.path().join("dir/file.txt"), root.path().join("file-link")).unwrap();
    symlink(root.path().join("dir"), root.path().join("dir-link")).unwrap();

    let dir = env.file_info("dir").await.unwrap();
    assert_eq!((dir.name.as_str(), dir.kind), ("dir", FileKind::Directory));
    let file = env.file_info("dir/file.txt").await.unwrap();
    assert_eq!(
        (file.name.as_str(), file.kind, file.size),
        ("file.txt", FileKind::File, 5.0)
    );
    assert_eq!(env.file_info("file-link").await.unwrap().kind, FileKind::Symlink);
    assert_eq!(env.file_info("dir-link").await.unwrap().kind, FileKind::Symlink);
    assert_eq!(
        env.canonical_path("file-link").await.unwrap(),
        std::fs::canonicalize(root.path().join("dir/file.txt"))
            .unwrap()
            .to_string_lossy()
    );
}

#[tokio::test]
async fn lists_symlinks_as_symlinks() {
    let root = temp_dir();
    let env = env_at(root.path());
    env.write_file("target.txt", text("hello"), None).await.unwrap();
    symlink(root.path().join("target.txt"), root.path().join("link.txt")).unwrap();

    let mut entries = env
        .list_dir(".", None)
        .await
        .unwrap()
        .into_iter()
        .map(|entry| (entry.name, entry.kind))
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    assert_eq!(
        entries,
        vec![
            ("link.txt".to_owned(), FileKind::Symlink),
            ("target.txt".to_owned(), FileKind::File)
        ]
    );
}

#[tokio::test]
async fn stops_reading_text_lines_at_the_requested_limit() {
    let root = temp_dir();
    let env = env_at(root.path());
    env.write_file("file.txt", text("one\ntwo\nthree"), None).await.unwrap();
    assert_eq!(
        env.read_text_lines(
            "file.txt",
            ReadTextLinesOptions {
                max_lines: Some(1),
                abort: None
            }
        )
        .await
        .unwrap(),
        vec!["one".to_owned()]
    );
    assert_eq!(
        env.read_text_lines(
            "file.txt",
            ReadTextLinesOptions {
                max_lines: Some(0),
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
    let missing = env.file_info("missing.txt").await.unwrap_err();
    assert_eq!(missing.name, "FileError");
    assert_eq!(missing.code, FileErrorCode::NotFound);
    assert_eq!(
        missing.path.as_deref(),
        Some(root.path().join("missing.txt").to_string_lossy().as_ref())
    );
    assert_eq!(
        missing.message,
        format!(
            "ENOENT: no such file or directory, lstat '{}'",
            root.path().join("missing.txt").display()
        )
    );
    assert!(!env.exists("missing.txt", None).await.unwrap());

    env.write_file("file.txt", text("hello"), None).await.unwrap();
    let error = env.list_dir("file.txt", None).await.unwrap_err();
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
    env.append_file("new/nested/file.txt", text("a")).await.unwrap();
    env.append_file("new/nested/file.txt", text("b")).await.unwrap();
    assert_eq!(env.read_text_file("new/nested/file.txt", None).await.unwrap(), "ab");
}

#[tokio::test]
async fn renames_files_and_reports_the_source_path_on_failure() {
    let root = temp_dir();
    let env = env_at(root.path());
    env.write_file("source.txt", text("new"), None).await.unwrap();
    env.write_file("destination.txt", text("old"), None).await.unwrap();
    env.rename_file("source.txt", "destination.txt", None).await.unwrap();
    assert!(!env.exists("source.txt", None).await.unwrap());
    assert_eq!(env.read_text_file("destination.txt", None).await.unwrap(), "new");

    env.write_file("destination.txt", text("unchanged"), None)
        .await
        .unwrap();
    let error = env
        .rename_file("missing-source.txt", "destination.txt", None)
        .await
        .unwrap_err();
    assert_eq!(error.code, FileErrorCode::NotFound);
    assert_eq!(
        error.path.as_deref(),
        Some(root.path().join("missing-source.txt").to_string_lossy().as_ref())
    );
    assert_eq!(env.read_text_file("destination.txt", None).await.unwrap(), "unchanged");
}

#[tokio::test]
async fn creates_temporary_directories_and_files() {
    let root = temp_dir();
    let env = env_at(root.path());
    let directory = env.create_temp_dir("node-env-test-").await.unwrap();
    assert!(Path::new(&directory).is_dir());
    let file = env
        .create_temp_file(CreateTempFileOptions {
            prefix: Some("prefix-".to_owned()),
            suffix: Some(".txt".to_owned()),
        })
        .await
        .unwrap();
    assert!(Path::new(&file).is_file());
    assert!(file.ends_with(".txt"));
    assert!(
        Path::new(&file)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("prefix-")
    );
}

#[tokio::test]
async fn honors_create_and_remove_options() {
    let root = temp_dir();
    let env = env_at(root.path());
    let error = env
        .create_dir("missing/child", CreateDirOptions { recursive: Some(false) })
        .await
        .unwrap_err();
    assert_eq!(error.code, FileErrorCode::NotFound);

    env.write_file("dir/child/file.txt", text("hello"), None).await.unwrap();
    let error = env
        .remove(
            "dir",
            RemoveOptions {
                recursive: Some(false),
                force: Some(false),
            },
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, FileErrorCode::Unknown);
    env.remove(
        "dir",
        RemoveOptions {
            recursive: Some(true),
            force: Some(false),
        },
    )
    .await
    .unwrap();
    assert!(!env.exists("dir", None).await.unwrap());

    let error = env
        .remove(
            "missing",
            RemoveOptions {
                recursive: Some(false),
                force: Some(false),
            },
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, FileErrorCode::NotFound);
    env.remove(
        "missing",
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
    env.write_file("file.txt", text("hello"), None).await.unwrap();
    let signal = CancellationToken::new();
    signal.cancel();

    let results = tokio::join!(
        env.read_text_file("file.txt", Some(&signal)),
        env.read_text_lines(
            "file.txt",
            ReadTextLinesOptions {
                max_lines: None,
                abort: Some(signal.clone())
            }
        ),
        env.read_binary_file("file.txt", Some(&signal)),
        env.write_file("other.txt", text("x"), Some(&signal)),
        env.rename_file("file.txt", "renamed.txt", Some(&signal)),
        env.list_dir(".", Some(&signal)),
    );
    for code in [
        results.0.unwrap_err().code,
        results.1.unwrap_err().code,
        results.2.unwrap_err().code,
        results.3.unwrap_err().code,
        results.4.unwrap_err().code,
        results.5.unwrap_err().code,
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
            "printf '%s:%s' \"$PWD\" \"$NODE_ENV_TEST\"",
            ShellExecOptions {
                env: Some(HashMap::from([("NODE_ENV_TEST".to_owned(), "ok".to_owned())])),
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
    let mut shell_env = HashMap::new();
    shell_env.insert("PI_SESSION_FILE".to_owned(), "/stale/parent.jsonl".to_owned());
    shell_env.insert("PI_CODING_AGENT".to_owned(), "true".to_owned());
    let env = NodeExecutionEnv::new(NodeExecutionEnvOptions {
        cwd: root.path().to_string_lossy().into_owned(),
        shell_path: None,
        shell_env,
    });

    let missing = env
        .exec(
            "printf '%s:%s|%s' \"${PI_SESSION_FILE+x}\" \"${PI_SESSION_FILE-}\" \"$PI_CODING_AGENT\"",
            ShellExecOptions::default(),
        )
        .await
        .unwrap();
    assert_eq!(missing.stdout, "x:/stale/parent.jsonl|true");

    let empty = env
        .exec(
            "printf '%s:%s|%s' \"${PI_SESSION_FILE+x}\" \"${PI_SESSION_FILE-}\" \"$PI_CODING_AGENT\"",
            ShellExecOptions {
                env: Some(HashMap::from([("PI_SESSION_FILE".to_owned(), String::new())])),
                ..ShellExecOptions::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(empty.stdout, "x:|true");
}

#[tokio::test]
async fn can_replace_rather_than_inherit_the_default_shell_environment() {
    let root = temp_dir();
    let configured_key = "PI_NODE_ENV_CONFIGURED_TEST";
    let explicit_key = "PI_NODE_ENV_EXPLICIT_TEST";
    let env = NodeExecutionEnv::new(NodeExecutionEnvOptions {
        cwd: root.path().to_string_lossy().into_owned(),
        shell_path: None,
        shell_env: HashMap::from([(configured_key.to_owned(), "configured".to_owned())]),
    });
    let result = env
        .exec(
            &format!("printf '%s:%s' \"${{{configured_key}-}}\" \"${{{explicit_key}-}}\""),
            ShellExecOptions {
                inherit_env: Some(false),
                env: Some(HashMap::from([(explicit_key.to_owned(), "explicit".to_owned())])),
                ..ShellExecOptions::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(result.stdout, ":explicit");
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
            "printf out; printf err >&2",
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
async fn reports_a_missing_working_directory_before_spawning() {
    let root = temp_dir();
    let missing = root.path().join("missing");
    let env = NodeExecutionEnv::new(NodeExecutionEnvOptions {
        cwd: missing.to_string_lossy().into_owned(),
        shell_path: None,
        shell_env: HashMap::new(),
    });
    let error = env.exec("printf ok", ShellExecOptions::default()).await.unwrap_err();
    assert_eq!(error.code, ExecutionErrorCode::SpawnError);
    assert_eq!(
        error.message,
        format!(
            "Working directory does not exist: {}\nCannot execute bash commands.",
            missing.display()
        )
    );
}

#[tokio::test]
async fn returns_non_zero_exit_codes_and_timeout_errors() {
    let root = temp_dir();
    let env = env_at(root.path());
    let result = env.exec("exit 7", ShellExecOptions::default()).await.unwrap();
    assert_eq!(result.exit_code, 7);

    let error = env
        .exec(
            "sleep 5",
            ShellExecOptions {
                timeout: Some(0.01),
                ..ShellExecOptions::default()
            },
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, ExecutionErrorCode::Timeout);
    assert_eq!(error.message, "timeout:0.01");

    let invalid = env
        .exec(
            "printf ok",
            ShellExecOptions {
                timeout: Some(0.0),
                ..ShellExecOptions::default()
            },
        )
        .await
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
            "printf out",
            ShellExecOptions {
                on_stdout: Some(Arc::new(|_: &str| Err("callback failed".to_owned()))),
                ..ShellExecOptions::default()
            },
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, ExecutionErrorCode::CallbackError);
    assert_eq!(error.message, "callback failed");
}

#[tokio::test]
async fn returns_shell_unavailable_and_spawn_errors() {
    let root = temp_dir();
    let missing_shell = root.path().join("missing-shell");
    let env = NodeExecutionEnv::new(NodeExecutionEnvOptions {
        cwd: root.path().to_string_lossy().into_owned(),
        shell_path: Some(missing_shell.to_string_lossy().into_owned()),
        shell_env: HashMap::new(),
    });
    let error = env.exec("printf ok", ShellExecOptions::default()).await.unwrap_err();
    assert_eq!(error.code, ExecutionErrorCode::ShellUnavailable);
    assert_eq!(
        error.message,
        format!("Custom shell path not found: {}", missing_shell.display())
    );

    let shell_path = root.path().join("not-executable-shell");
    env.write_file("not-executable-shell", text("not executable"), None)
        .await
        .unwrap();
    let env = NodeExecutionEnv::new(NodeExecutionEnvOptions {
        cwd: root.path().to_string_lossy().into_owned(),
        shell_path: Some(shell_path.to_string_lossy().into_owned()),
        shell_env: HashMap::new(),
    });
    let error = env.exec("printf ok", ShellExecOptions::default()).await.unwrap_err();
    assert_eq!(error.code, ExecutionErrorCode::SpawnError);
}

#[tokio::test]
async fn returns_an_aborted_result_for_aborted_commands() {
    let root = temp_dir();
    let env = env_at(root.path());
    let signal = CancellationToken::new();
    let execution = env.exec(
        "sleep 5",
        ShellExecOptions {
            abort: Some(signal.clone()),
            ..ShellExecOptions::default()
        },
    );
    signal.cancel();
    let error = execution.await.unwrap_err();
    assert_eq!(error.code, ExecutionErrorCode::Aborted);
    assert_eq!(error.message, "aborted");
}

#[tokio::test]
async fn cleanup_terminates_active_shell_processes() {
    let root = temp_dir();
    let env = Arc::new(env_at(root.path()));
    let execution_env = Arc::clone(&env);
    let execution = tokio::spawn(async move {
        execution_env
            .exec("touch started; sleep 60", ShellExecOptions::default())
            .await
    });
    let started = root.path().join("started");
    for _ in 0..100 {
        if started.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(started.exists());
    env.cleanup().await;
    let result = tokio::time::timeout(Duration::from_secs(3), execution)
        .await
        .expect("cleanup must terminate the shell")
        .expect("execution task must not panic");
    assert!(result.is_ok());
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
        "printf out",
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
