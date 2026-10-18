use std::collections::BTreeMap;
use std::sync::Arc;

use pi_agent_core::agent::{AgentTool, AgentToolResult, ToolError};
use pi_ai::request::BoxFuture;
use pi_ai::types::TextOrImageContent;
use pi_coding_agent::core::tools::bash::{
    BashExecOptions, BashExecResult, BashOperations, BashTool, BashToolOptions, create_local_bash_operations,
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
