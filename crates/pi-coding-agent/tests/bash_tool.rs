use std::collections::BTreeMap;
use std::sync::Arc;

use pi_agent_core::agent::{AgentTool, AgentToolResult, ToolError};
use pi_ai::request::BoxFuture;
use pi_ai::types::TextOrImageContent;
use pi_coding_agent::core::tools::bash::{
    BashDetachOptions, BashDetachedResult, BashExecOptions, BashExecResult, BashOperations, BashTool, BashToolOptions,
    DetachedJobArtifacts, create_local_bash_operations,
};
use serde_json::json;
use tokio_util::sync::CancellationToken;

fn execute(tool: &BashTool, params: serde_json::Value) -> Result<AgentToolResult, String> {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    runtime
        .block_on(tool.execute("call".to_owned(), params, None, Arc::new(|_result| {}), None))
        .map_err(|error| error.message)
}

fn text(result: &AgentToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|entry| match entry {
            TextOrImageContent::Text(text) => Some(text.text.clone()),
            TextOrImageContent::Image(_) => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn executes_simple_commands() {
    let temp = tempfile::tempdir().expect("temp dir");
    let result = execute(
        &BashTool::new(temp.path().to_string_lossy()),
        json!({"command": "echo 'test output'"}),
    )
    .expect("bash result");
    assert_eq!(text(&result).trim(), "test output");
    assert!(result.details.is_none());
}

#[test]
fn reports_non_zero_exit_codes() {
    let temp = tempfile::tempdir().expect("temp dir");
    let error = execute(
        &BashTool::new(temp.path().to_string_lossy()),
        json!({"command": "exit 1"}),
    )
    .expect_err("exit 1");
    assert!(error.contains("Command exited with code 1"), "{error}");
}

#[test]
fn respects_the_timeout() {
    let temp = tempfile::tempdir().expect("temp dir");
    let error = execute(
        &BashTool::new(temp.path().to_string_lossy()),
        json!({"command": "sleep 5", "timeout": 1}),
    )
    .expect_err("timeout");
    assert!(error.contains("Command timed out after 1 seconds"), "{error}");
}

#[test]
fn rejects_invalid_timeouts() {
    let temp = tempfile::tempdir().expect("temp dir");
    let tool = BashTool::new(temp.path().to_string_lossy());
    let error = execute(&tool, json!({"command": "true", "timeout": 0})).expect_err("zero timeout");
    assert_eq!(error, "Invalid timeout: must be a finite number of seconds");
    let error = execute(&tool, json!({"command": "true", "timeout": 2_147_484})).expect_err("large timeout");
    assert_eq!(error, "Invalid timeout: maximum is 2147483.647 seconds");
}

#[test]
fn reports_missing_working_directories() {
    let missing = "/this/directory/definitely/does/not/exist/12345";
    let error = execute(&BashTool::new(missing), json!({"command": "echo test"})).expect_err("missing cwd");
    assert!(error.contains("Working directory does not exist"), "{error}");
}

#[test]
fn reports_shell_spawn_failures_and_custom_shell_errors() {
    let temp = tempfile::tempdir().expect("temp dir");
    let tool = BashTool::with_options(
        temp.path().to_string_lossy(),
        BashToolOptions {
            shell_path: Some("/nonexistent-shell-path-xyz123".to_owned()),
            ..Default::default()
        },
    );
    let error = execute(&tool, json!({"command": "echo test"})).expect_err("bad shell");
    assert!(error.contains("Custom shell path not found"), "{error}");
}

#[test]
fn prepends_the_command_prefix() {
    let temp = tempfile::tempdir().expect("temp dir");
    let tool = BashTool::with_options(
        temp.path().to_string_lossy(),
        BashToolOptions {
            command_prefix: Some("export TEST_VAR=hello".to_owned()),
            ..Default::default()
        },
    );
    let result = execute(&tool, json!({"command": "echo $TEST_VAR"})).expect("prefix result");
    assert_eq!(text(&result).trim(), "hello");
}

#[test]
fn local_operations_stream_output_and_inherit_the_environment() {
    let temp = tempfile::tempdir().expect("temp dir");
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let chunks = Arc::new(std::sync::Mutex::new(Vec::new()));
    let captured = Arc::clone(&chunks);
    let mut env = std::env::vars().collect::<BTreeMap<_, _>>();
    env.insert("TEST_LOCAL_BASH_OPS".to_owned(), "from-local-ops".to_owned());
    let result = runtime
        .block_on(create_local_bash_operations(None).exec(
            "echo $TEST_LOCAL_BASH_OPS",
            &temp.path().to_string_lossy(),
            BashExecOptions {
                on_data: Arc::new(move |data| captured.lock().expect("chunks").extend_from_slice(data)),
                signal: None,
                timeout: None,
                env,
                detach: None,
            },
        ))
        .expect("local ops");
    assert_eq!(result.exit_code, Some(0));
    let output = String::from_utf8(chunks.lock().expect("chunks").clone()).expect("utf8");
    assert_eq!(output.trim(), "from-local-ops");
}

struct ScriptedOperations {
    lines: usize,
    error: Option<&'static str>,
    bytes: Option<Vec<u8>>,
}

impl BashOperations for ScriptedOperations {
    fn exec<'a>(
        &'a self,
        _command: &'a str,
        _cwd: &'a str,
        options: BashExecOptions,
    ) -> BoxFuture<'a, Result<BashExecResult, ToolError>> {
        Box::pin(async move {
            if let Some(bytes) = &self.bytes {
                (options.on_data)(bytes);
            }
            for index in 1..=self.lines {
                (options.on_data)(format!("{index}\n").as_bytes());
            }
            match self.error {
                Some(error) => Err(ToolError::new(error)),
                None => Ok(BashExecResult {
                    exit_code: Some(0),
                    detached: None,
                }),
            }
        })
    }
}

#[test]
fn timeout_and_abort_errors_include_the_full_output_path() {
    for (error, expected) in [
        ("timeout:5", "Command timed out after 5 seconds"),
        ("aborted", "Command aborted"),
    ] {
        let temp = tempfile::tempdir().expect("temp dir");
        let tool = BashTool::with_options(
            temp.path().to_string_lossy(),
            BashToolOptions {
                operations: Some(Arc::new(ScriptedOperations {
                    lines: 3000,
                    error: Some(error),
                    bytes: None,
                })),
                ..Default::default()
            },
        );
        let error = execute(&tool, json!({"command": "chatty-fail"})).expect_err("scripted failure");
        assert!(error.contains(expected), "{error}");
        assert!(error.contains("[Showing lines "), "{error}");
        assert!(error.contains("Full output: "), "{error}");
        let path = error
            .split("Full output: ")
            .nth(1)
            .and_then(|rest| rest.split(']').next())
            .expect("full output path");
        let full_output = std::fs::read_to_string(path).expect("full output file");
        assert!(full_output.contains("1\n2\n3"));
        assert!(full_output.contains("2998\n2999\n3000"));
    }
}

#[test]
fn trailing_newline_does_not_add_a_truncated_line() {
    let temp = tempfile::tempdir().expect("temp dir");
    let tool = BashTool::with_options(
        temp.path().to_string_lossy(),
        BashToolOptions {
            operations: Some(Arc::new(ScriptedOperations {
                lines: 4000,
                error: None,
                bytes: None,
            })),
            ..Default::default()
        },
    );
    let result = execute(&tool, json!({"command": "many-lines"})).expect("bash result");
    let details = result.details.as_ref().expect("details");
    assert_eq!(details["truncation"]["totalLines"], json!(4000));
    assert_eq!(details["truncation"]["outputLines"], json!(2000));
    let output = text(&result);
    assert!(output.starts_with("2001\n"), "{output}");
    assert!(
        output.contains("[Showing lines 2001-4000 of 4000. Full output: "),
        "{output}"
    );
}

#[test]
fn decodes_utf8_characters_split_across_chunks() {
    let temp = tempfile::tempdir().expect("temp dir");
    let euro = "€\n".as_bytes().to_vec();
    let tool = BashTool::with_options(
        temp.path().to_string_lossy(),
        BashToolOptions {
            operations: Some(Arc::new(SplitBytes { bytes: euro })),
            ..Default::default()
        },
    );
    let result = execute(&tool, json!({"command": "split-utf8"})).expect("bash result");
    assert_eq!(text(&result).trim(), "€");
}

struct SplitBytes {
    bytes: Vec<u8>,
}

impl BashOperations for SplitBytes {
    fn exec<'a>(
        &'a self,
        _command: &'a str,
        _cwd: &'a str,
        options: BashExecOptions,
    ) -> BoxFuture<'a, Result<BashExecResult, ToolError>> {
        Box::pin(async move {
            (options.on_data)(&self.bytes[..1]);
            (options.on_data)(&self.bytes[1..]);
            Ok(BashExecResult {
                exit_code: Some(0),
                detached: None,
            })
        })
    }
}

#[test]
fn coalesces_streaming_updates() {
    struct Chatty;
    impl BashOperations for Chatty {
        fn exec<'a>(
            &'a self,
            _command: &'a str,
            _cwd: &'a str,
            options: BashExecOptions,
        ) -> BoxFuture<'a, Result<BashExecResult, ToolError>> {
            Box::pin(async move {
                for index in 0..5000 {
                    (options.on_data)(format!("line {index}\n").as_bytes());
                }
                Ok(BashExecResult {
                    exit_code: Some(0),
                    detached: None,
                })
            })
        }
    }
    let temp = tempfile::tempdir().expect("temp dir");
    let tool = BashTool::with_options(
        temp.path().to_string_lossy(),
        BashToolOptions {
            operations: Some(Arc::new(Chatty)),
            ..Default::default()
        },
    );
    let updates = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = Arc::clone(&updates);
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let result = runtime
        .block_on(tool.execute(
            "call".to_owned(),
            json!({"command": "chatty"}),
            None,
            Arc::new(move |_result| {
                counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }),
            None,
        ))
        .expect("chatty result");
    assert!(updates.load(std::sync::atomic::Ordering::SeqCst) < 25);
    assert!(text(&result).contains("line 4999"));
}

#[test]
fn abort_signal_reports_command_aborted() {
    let temp = tempfile::tempdir().expect("temp dir");
    let token = CancellationToken::new();
    token.cancel();
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let error = runtime
        .block_on(BashTool::new(temp.path().to_string_lossy()).execute(
            "call".to_owned(),
            json!({"command": "echo hi"}),
            Some(token),
            Arc::new(|_result| {}),
            None,
        ))
        .expect_err("aborted");
    assert_eq!(error.message, "Command aborted");
}

#[test]
fn detached_output_keeps_the_truncation_footer() {
    struct Detaching;
    impl BashOperations for Detaching {
        fn exec<'a>(
            &'a self,
            _command: &'a str,
            _cwd: &'a str,
            options: BashExecOptions,
        ) -> BoxFuture<'a, Result<BashExecResult, ToolError>> {
            Box::pin(async move {
                let output = (1..=2500).map(|line| format!("line {line}\n")).collect::<String>();
                (options.on_data)(output.as_bytes());
                Ok(BashExecResult {
                    exit_code: None,
                    detached: Some(BashDetachedResult {
                        job_id: "background".to_owned(),
                        message: "Background job started".to_owned(),
                        log_path: None,
                    }),
                })
            })
        }
    }
    let temp = tempfile::tempdir().expect("temp dir");
    let tool = BashTool::with_options(
        temp.path().to_string_lossy(),
        BashToolOptions {
            operations: Some(Arc::new(Detaching)),
            ..Default::default()
        },
    );
    let result = execute(&tool, json!({"command": "detach"})).expect("detached result");
    let output = text(&result);
    assert!(
        output.contains("[Showing lines 1477-2500 of 2500 (10.0KB limit). Full output: "),
        "{output}"
    );
    assert!(
        output.contains("Command moved to background as job background. Background job started"),
        "{output}"
    );
    assert_eq!(result.details.expect("details")["backgroundJobId"], json!("background"));
}

#[test]
fn local_operations_remove_artifacts_when_not_detached() {
    let temp = tempfile::tempdir().expect("temp dir");
    let directory = temp.path().join("job");
    std::fs::create_dir_all(&directory).expect("artifact directory");
    let artifacts = DetachedJobArtifacts {
        directory: directory.to_string_lossy().into_owned(),
        output_path: directory.join("output.log").to_string_lossy().into_owned(),
    };
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let result = runtime
        .block_on(create_local_bash_operations(None).exec(
            "true",
            &temp.path().to_string_lossy(),
            BashExecOptions {
                on_data: Arc::new(|_| {}),
                signal: None,
                timeout: None,
                env: BTreeMap::new(),
                detach: Some(BashDetachOptions {
                    artifacts: Some(artifacts),
                    job_id: None,
                    signal: CancellationToken::new(),
                }),
            },
        ))
        .expect("local command");
    assert_eq!(result.exit_code, Some(0));
    assert!(result.detached.is_none());
    assert!(!directory.exists(), "artifacts directory must be removed");
}

#[test]
fn local_operations_detach_uses_the_default_result() {
    let temp = tempfile::tempdir().expect("temp dir");
    let directory = temp.path().join("job");
    std::fs::create_dir_all(&directory).expect("artifact directory");
    let artifacts = DetachedJobArtifacts {
        directory: directory.to_string_lossy().into_owned(),
        output_path: directory.join("output.log").to_string_lossy().into_owned(),
    };
    let detach_signal = CancellationToken::new();
    detach_signal.cancel();
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let result = runtime
        .block_on(create_local_bash_operations(None).exec(
            "sleep 0.2",
            &temp.path().to_string_lossy(),
            BashExecOptions {
                on_data: Arc::new(|_| {}),
                signal: None,
                timeout: None,
                env: BTreeMap::new(),
                detach: Some(BashDetachOptions {
                    artifacts: Some(artifacts),
                    job_id: None,
                    signal: detach_signal,
                }),
            },
        ))
        .expect("local command");
    let detached = result.detached.expect("detached result");
    assert_eq!(detached.job_id, "background");
    assert_eq!(detached.message, "Background job started");
    assert_eq!(detached.log_path, None);
}

#[test]
fn finishes_without_an_extra_update() {
    struct OneChunk;
    impl BashOperations for OneChunk {
        fn exec<'a>(
            &'a self,
            _command: &'a str,
            _cwd: &'a str,
            options: BashExecOptions,
        ) -> BoxFuture<'a, Result<BashExecResult, ToolError>> {
            Box::pin(async move {
                (options.on_data)(b"line\n");
                Ok(BashExecResult {
                    exit_code: Some(0),
                    detached: None,
                })
            })
        }
    }
    let temp = tempfile::tempdir().expect("temp dir");
    let tool = BashTool::with_options(
        temp.path().to_string_lossy(),
        BashToolOptions {
            operations: Some(Arc::new(OneChunk)),
            ..Default::default()
        },
    );
    let updates = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = Arc::clone(&updates);
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    runtime
        .block_on(tool.execute(
            "call".to_owned(),
            json!({"command": "one-chunk"}),
            None,
            Arc::new(move |_result| {
                counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }),
            None,
        ))
        .expect("bash result");
    assert_eq!(updates.load(std::sync::atomic::Ordering::SeqCst), 2);
}

#[test]
fn does_not_emit_updates_after_completion() {
    struct TwoChunks;
    impl BashOperations for TwoChunks {
        fn exec<'a>(
            &'a self,
            _command: &'a str,
            _cwd: &'a str,
            options: BashExecOptions,
        ) -> BoxFuture<'a, Result<BashExecResult, ToolError>> {
            Box::pin(async move {
                (options.on_data)(b"a\n");
                (options.on_data)(b"b\n");
                Ok(BashExecResult {
                    exit_code: Some(0),
                    detached: None,
                })
            })
        }
    }
    let temp = tempfile::tempdir().expect("temp dir");
    let tool = BashTool::with_options(
        temp.path().to_string_lossy(),
        BashToolOptions {
            operations: Some(Arc::new(TwoChunks)),
            ..Default::default()
        },
    );
    let updates = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = Arc::clone(&updates);
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    runtime
        .block_on(tool.execute(
            "call".to_owned(),
            json!({"command": "two-chunks"}),
            None,
            Arc::new(move |_result| {
                counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }),
            None,
        ))
        .expect("bash result");
    let completed = updates.load(std::sync::atomic::Ordering::SeqCst);
    runtime.block_on(async {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    });
    assert_eq!(updates.load(std::sync::atomic::Ordering::SeqCst), completed);
}
