//! Bundled `browser-cli` extension (`extensions/browser-cli/src/index.ts`).

use std::sync::Arc;

use pi_agent_core::agent::types::{AgentToolResult, AgentToolUpdateCallback};
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_json::{JsString, JsValue, encode_node_utf8};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::core::extensions::types::{BoxFuture, ExtensionApi, ExtensionContext, HandlerError, ToolDefinition};
use crate::utils::child_process::node_spawn_error_message;

const BROWSER_CLI_EXECUTABLE: &str = "browser-cli";

const BROWSER_CLI_PROMPT_GUIDELINES: [&str; 7] = [
    "When spawn_agent is available, use a fresh agentType \"browser\" for multi-step browser workflows; direct browser-cli use remains allowed for simple actions.",
    "Pass deterministic command sequences in one ordered commands batch. Use separate calls when an intermediate result determines the next action.",
    "Each agent drives its own dedicated background tab (BROWSER_CLI_SESSION is set automatically); `tabs list` marks it with *. Never activate or navigate other tabs.",
    "Observe with snapshot --interactive --compact before interacting. Re-observe after navigation or page mutation instead of repeatedly guessing selectors.",
    "Use open for navigation; fill, click, and press for interaction. `press` takes only a key, not a selector. Wait for bounded delays or selectors; use get url, get title, and visible text for completion proof.",
    "When a login form appears, attempt mapped authentication with broker-unlock --current-origin followed by broker-fill --current-origin. Never request or pass credential values.",
    "After broker fill, inspect the page, click the sign-in control, and verify the resulting authenticated state. Stop for user action when MFA or CAPTCHA appears.",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserCliCommandResult {
    pub stdout: JsString,
    pub stderr: JsString,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserCliCommandExecution {
    pub args: Vec<JsString>,
    pub stdout: JsString,
    pub stderr: JsString,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserCliBatchResult {
    pub commands: Vec<BrowserCliCommandExecution>,
}

pub type BrowserCliCommandRunner = Arc<
    dyn Fn(
            String,
            Vec<JsString>,
            JsString,
            Option<AbortSignal>,
        ) -> BoxFuture<'static, Result<BrowserCliCommandResult, HandlerError>>
        + Send
        + Sync,
>;

#[derive(Clone, Default)]
pub struct BrowserCliExtensionOptions {
    pub run_command: Option<BrowserCliCommandRunner>,
}

pub fn browser_cli_extension(pi: &ExtensionApi, options: BrowserCliExtensionOptions) -> Result<(), HandlerError> {
    if !crate::utils::executable::is_executable_available(&BROWSER_CLI_EXECUTABLE.into(), &Default::default())
        .map_err(|error| HandlerError::from(*error))?
    {
        return Ok(());
    }
    pi.register_tool(create_browser_cli_tool_definition(options))
}

pub fn create_browser_cli_tool_definition(options: BrowserCliExtensionOptions) -> ToolDefinition {
    let run_command = options.run_command.unwrap_or_else(|| {
        Arc::new(|executable, args, session, signal| {
            Box::pin(run_browser_cli_command(executable, args, session, signal))
        })
    });

    ToolDefinition {
        name: "browser-cli".to_owned(),
        label: "Browser CLI".into(),
        description: "Run ordered browser-cli argv commands sequentially and return each command result.".into(),
        prompt_snippet: Some("Control the browser through ordered browser-cli command batches.".into()),
        prompt_guidelines: Some(
            BROWSER_CLI_PROMPT_GUIDELINES
                .iter()
                .map(|line| (*line).into())
                .collect(),
        ),
        approval_required: Some(true),
        approval_kind: None,
        parameters: browser_cli_schema(),
        render_shell: None,
        prepare_arguments: None,
        prepare_execution: None,
        execution_mode: Some(pi_agent_core::harness::types::ToolExecutionMode::Sequential),
        execute: Arc::new(
            move |_tool_call_id: String,
                  params: JsValue,
                  signal: Option<AbortSignal>,
                  _on_update: Option<AgentToolUpdateCallback>,
                  ctx: Option<ExtensionContext>| {
                let run_command = Arc::clone(&run_command);
                Box::pin(async move {
                    let session = browser_cli_session(ctx.as_ref())?;
                    execute_browser_cli(run_command, params, session, signal).await
                })
            },
        ),
        render_call: None,
        render_result: None,
    }
}

pub fn browser_cli_session(ctx: Option<&ExtensionContext>) -> Result<JsString, HandlerError> {
    let ctx = ctx.ok_or_else(|| HandlerError::new("browser-cli needs a session ID to select its dedicated tab"))?;
    let manager = ctx.session_manager()?;
    let session_id = crate::core::session_manager::lock_session_manager(&manager)
        .get_session_id()
        .clone();
    if session_id.is_empty() {
        return Err(HandlerError::new(
            "browser-cli needs a session ID to select its dedicated tab",
        ));
    }
    let agent_id = match &ctx.tool_execution_agent_id {
        Some(id) => JsString::from(id),
        None => ctx.multi_agent_agent_id()?.unwrap_or_else(|| "main".into()),
    };
    Ok(pi_js_compat::js_format!("{}:{}", session_id, agent_id))
}

fn browser_cli_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "required": ["commands"],
        "properties": {
            "commands": {
                "type": "array",
                "items": {
                    "type": "array",
                    "items": {"type": "string", "description": "One browser-cli argument."},
                    "minItems": 1
                },
                "description": "Ordered browser-cli argv commands to execute sequentially.",
                "minItems": 1
            }
        }
    })
}

async fn execute_browser_cli(
    run_command: BrowserCliCommandRunner,
    params: JsValue,
    session: JsString,
    signal: Option<AbortSignal>,
) -> Result<AgentToolResult, HandlerError> {
    let commands = parse_commands(&params)?;
    let executions = run_browser_cli_batch(run_command, commands, session, signal).await?;
    let text = format_browser_cli_batch_output(&executions);
    Ok(AgentToolResult {
        content: vec![TextOrImageContent::Text(TextContent::new(text))],
        details: Some(pi_js_compat::js_json!({"commands": executions})),
        is_error: None,
        terminate: None,
    })
}

#[cfg(test)]
mod lossless_tests {
    use super::*;
    #[tokio::test]
    async fn default_runner_passes_dedicated_browser_session_to_real_child() {
        let result = run_browser_cli_command("env".to_owned(), Vec::new(), "browser-test:main".into(), None)
            .await
            .unwrap();
        assert_eq!(result.exit_code, Some(0));
        assert!(
            result
                .stdout
                .split('\n')
                .any(|line| line == "BROWSER_CLI_SESSION=browser-test:main")
        );
    }

    #[test]
    fn failure_output_trims_javascript_whitespace_in_stream_order() {
        let execution = BrowserCliCommandExecution {
            args: Vec::new(),
            stdout: "\u{feff}out\u{feff}".into(),
            stderr: "\u{feff}bad\u{feff}".into(),
            exit_code: None,
        };
        assert_eq!(
            format_browser_cli_failure(0, &execution),
            "browser-cli command 1 exited with code unknown: bad\nout"
        );
    }

    #[tokio::test]
    async fn abort_keeps_default_runner_alive_to_send_sigterm_and_reap() {
        let dir = tempfile::tempdir().unwrap();
        let ready = dir.path().join("ready");
        let terminated = dir.path().join("terminated");
        let script = "import signal,time,sys,pathlib,os\nsignal.signal(signal.SIGTERM, lambda *args: (pathlib.Path(sys.argv[2]).write_text('term'), sys.exit(0)))\npathlib.Path(sys.argv[1]).write_text(str(os.getpid()))\ntime.sleep(2)\n";
        let controller = pi_js_compat::abort::AbortController::new();
        let signal = controller.signal();
        let args = vec![
            JsString::from("-c"),
            script.into(),
            ready.to_str().unwrap().into(),
            terminated.to_str().unwrap().into(),
        ];
        let future = Box::pin(run_browser_cli_command(
            "/usr/bin/python3".into(),
            args,
            "fixture:main".into(),
            Some(signal.clone()),
        ));
        let task = tokio::spawn(wait_for_abort(future, Some(signal)));
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            while !ready.exists() {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("child ready");
        controller.abort(None);
        assert_eq!(task.await.unwrap().unwrap_err().message, "browser-cli command aborted");
        let observed = tokio::time::timeout(std::time::Duration::from_secs(3), async {
            while !terminated.exists() {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await;
        assert!(
            observed.is_ok(),
            "the real child must receive SIGTERM, not SIGKILL or a dropped future"
        );
        assert_eq!(std::fs::read_to_string(terminated).unwrap(), "term");
        let pid = std::fs::read_to_string(ready).unwrap();
        let proc = std::path::PathBuf::from(format!("/proc/{pid}"));
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            while proc.exists() {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("aborted child must be reaped");
    }

    #[test]
    fn injected_runner_output_retains_raw_utf16_through_results_and_failure_text() {
        let wire = pi_js_compat::js_json::parse_lossless(r#"{"stdout":"x\ud800","stderr":"y\udc00","exitCode":null}"#)
            .unwrap();
        let result: BrowserCliCommandResult = pi_js_compat::js_json::from_value(&wire).expect("native runner result");
        assert_eq!(pi_js_compat::js_json::to_value(&result).unwrap(), wire);
        let execution = BrowserCliCommandExecution {
            args: Vec::new(),
            stdout: result.stdout,
            stderr: result.stderr,
            exit_code: result.exit_code,
        };
        assert_eq!(
            pi_js_compat::js_json::to_value(&format_browser_cli_batch_output(std::slice::from_ref(&execution)))
                .unwrap(),
            pi_js_compat::js_json::parse_lossless(r#""x\ud800y\udc00""#).unwrap()
        );
        assert_eq!(
            pi_js_compat::js_json::to_value(&format_browser_cli_failure(0, &execution)).unwrap(),
            pi_js_compat::js_json::parse_lossless(
                r#""browser-cli command 1 exited with code unknown: y\udc00\nx\ud800""#
            )
            .unwrap()
        );
    }

    #[test]
    fn command_arguments_remain_raw_before_process_boundary() {
        let params = pi_js_compat::js_json::parse_lossless(r#"{"commands":[["fill","\ud800"]]}"#).unwrap();
        let commands = parse_commands(&params).unwrap();
        assert_eq!(commands[0][1].units(), &[0xd800]);
    }
}

fn parse_commands(params: &JsValue) -> Result<Vec<Vec<JsString>>, HandlerError> {
    let commands = params
        .get("commands")
        .and_then(JsValue::as_array)
        .ok_or_else(|| HandlerError::new("commands must be an array"))?;
    commands
        .iter()
        .map(|command| {
            command
                .as_array()
                .ok_or_else(|| HandlerError::new("each command must be an array"))?
                .iter()
                .map(|argument| {
                    argument
                        .as_string()
                        .cloned()
                        .ok_or_else(|| HandlerError::new("browser-cli arguments must be strings"))
                })
                .collect()
        })
        .collect()
}

async fn run_browser_cli_batch(
    run_command: BrowserCliCommandRunner,
    commands: Vec<Vec<JsString>>,
    session: JsString,
    signal: Option<AbortSignal>,
) -> Result<Vec<BrowserCliCommandExecution>, HandlerError> {
    let mut results = Vec::new();
    for (index, args) in commands.into_iter().enumerate() {
        let result = wait_for_abort(
            run_command(
                BROWSER_CLI_EXECUTABLE.to_owned(),
                args.clone(),
                session.clone(),
                signal.clone(),
            ),
            signal.clone(),
        )
        .await?;
        let execution = BrowserCliCommandExecution {
            args,
            stdout: result.stdout,
            stderr: result.stderr,
            exit_code: result.exit_code,
        };
        if execution.exit_code != Some(0) {
            return Err(HandlerError::new(format_browser_cli_failure(index, &execution)));
        }
        results.push(execution);
    }
    Ok(results)
}

async fn wait_for_abort<T: Send + 'static>(
    future: BoxFuture<'static, Result<T, HandlerError>>,
    signal: Option<AbortSignal>,
) -> Result<T, HandlerError> {
    let Some(token) = signal else {
        return future.await;
    };
    if token.aborted() {
        return Err(create_abort_error());
    }
    let mut task = tokio::spawn(future);
    let result = tokio::select! {
        result = &mut task => result,
        _ = token.wait() => return Err(create_abort_error()),
    };
    result.unwrap_or_else(|error| {
        crate::utils::uncaught::throw_uncaught(crate::utils::uncaught::ERROR, error.to_string())
    })
}

async fn run_browser_cli_command(
    executable: String,
    args: Vec<JsString>,
    session: JsString,
    signal: Option<AbortSignal>,
) -> Result<BrowserCliCommandResult, HandlerError> {
    if signal.as_ref().is_some_and(AbortSignal::aborted) {
        return Err(create_abort_error());
    }
    let mut child = tokio::process::Command::new(&executable)
        .args(args.iter().map(encode_node_utf8))
        .env_remove("BROWSER_CLI_TAB")
        .env("BROWSER_CLI_SESSION", encode_node_utf8(&session))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| HandlerError::new(spawn_error_message(&executable, &error)))?;

    let captured = crate::utils::child_process::capture_child_process(
        &mut child,
        None,
        signal.as_ref(),
        rustix::process::Signal::TERM,
    )
    .await
    .map_err(|error| HandlerError::new(error.to_string()))?;
    if captured.stopped == Some(crate::utils::child_process::CaptureStop::Aborted)
        || signal.as_ref().is_some_and(AbortSignal::aborted)
    {
        return Err(create_abort_error());
    }
    Ok(BrowserCliCommandResult {
        stdout: JsString::from(String::from_utf8_lossy(&captured.output.stdout).as_ref()),
        stderr: JsString::from(String::from_utf8_lossy(&captured.output.stderr).as_ref()),
        exit_code: captured.output.status,
    })
}

fn create_abort_error() -> HandlerError {
    HandlerError::new("browser-cli command aborted")
}

fn format_browser_cli_batch_output(commands: &[BrowserCliCommandExecution]) -> JsString {
    JsString::join(
        commands
            .iter()
            .flat_map(|command| [&command.stdout, &command.stderr])
            .filter(|value| !value.is_empty()),
        "",
    )
}

fn format_browser_cli_failure(index: usize, result: &BrowserCliCommandExecution) -> JsString {
    let output = JsString::join(
        [result.stderr.trim(), result.stdout.trim()]
            .into_iter()
            .filter(|value| !value.is_empty()),
        "\n",
    );
    let detail = if output.is_empty() {
        JsString::default()
    } else {
        JsString::from(": ") + output
    };
    let exit_code = result
        .exit_code
        .map(|code| code.to_string())
        .unwrap_or_else(|| "unknown".to_owned());
    JsString::from(format!(
        "browser-cli command {} exited with code {exit_code}",
        index + 1
    )) + detail
}

fn spawn_error_message(command: &str, error: &std::io::Error) -> pi_js_compat::js_json::JsString {
    node_spawn_error_message("spawn", &pi_js_compat::js_json::JsString::from(command), error)
}
