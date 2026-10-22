//! Bundled `browser-cli` extension (`extensions/browser-cli/src/index.ts`).

use std::sync::Arc;

use pi_agent_core::agent::types::{AgentToolResult, AgentToolUpdateCallback};
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_js_compat::js_json::{JsString, JsValue, encode_node_utf8};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::core::extensions::types::{BoxFuture, ExtensionApi, ExtensionContext, HandlerError, ToolDefinition};
use crate::utils::child_process::node_spawn_error_message;

const BROWSER_CLI_EXECUTABLE: &str = "browser-cli";

const BROWSER_CLI_PROMPT_GUIDELINES: [&str; 6] = [
    "When spawn_agent is available, use a fresh agentType \"browser\" for multi-step browser workflows; direct browser-cli use remains allowed for simple actions.",
    "Pass deterministic command sequences in one ordered commands batch. Use separate calls when an intermediate result determines the next action.",
    "Observe with snapshot --interactive --compact before interacting. Re-observe after navigation or page mutation instead of repeatedly guessing selectors.",
    "Use open for navigation; fill, click, and press for interaction. `press` takes only a key, not a selector. Wait for bounded delays or selectors; use get url, get title, and visible text for completion proof.",
    "When a login form appears, attempt mapped authentication with broker-unlock --current-origin followed by broker-fill --current-origin. Never request or pass credential values.",
    "After broker fill, inspect the page, click the sign-in control, and verify the resulting authenticated state. Stop for user action when MFA or CAPTCHA appears.",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserCliCommandResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserCliCommandExecution {
    pub args: Vec<JsString>,
    pub stdout: String,
    pub stderr: String,
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
            Option<CancellationToken>,
        ) -> BoxFuture<'static, Result<BrowserCliCommandResult, HandlerError>>
        + Send
        + Sync,
>;

#[derive(Clone, Default)]
pub struct BrowserCliExtensionOptions {
    pub run_command: Option<BrowserCliCommandRunner>,
}

pub fn browser_cli_extension(pi: &ExtensionApi, options: BrowserCliExtensionOptions) -> Result<(), HandlerError> {
    pi.register_tool(create_browser_cli_tool_definition(options))
        .map_err(HandlerError::new)
}

pub fn create_browser_cli_tool_definition(options: BrowserCliExtensionOptions) -> ToolDefinition {
    let run_command = options.run_command.unwrap_or_else(|| {
        Arc::new(|executable, args, signal| Box::pin(run_browser_cli_command(executable, args, signal)))
    });

    ToolDefinition {
        name: "browser-cli".to_owned(),
        label: "Browser CLI".to_owned(),
        description: "Run ordered browser-cli argv commands sequentially and return each command result.".to_owned(),
        prompt_snippet: Some("Control the browser through ordered browser-cli command batches.".to_owned()),
        prompt_guidelines: Some(
            BROWSER_CLI_PROMPT_GUIDELINES
                .iter()
                .map(|line| (*line).to_owned())
                .collect(),
        ),
        approval_required: Some(true),
        approval_kind: None,
        parameters: browser_cli_schema(),
        constrained_sampling: None,
        render_shell: None,
        prepare_arguments: None,
        prepare_execution: None,
        execution_mode: Some(pi_agent_core::harness::types::ToolExecutionMode::Sequential),
        execute: Arc::new(
            move |_tool_call_id: JsString,
                  params: JsValue,
                  signal: Option<CancellationToken>,
                  _on_update: Option<AgentToolUpdateCallback>,
                  _ctx: ExtensionContext| {
                let run_command = Arc::clone(&run_command);
                Box::pin(async move { execute_browser_cli(run_command, params, signal).await })
            },
        ),
        render_call: None,
        render_result: None,
    }
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
    signal: Option<CancellationToken>,
) -> Result<AgentToolResult, HandlerError> {
    let commands = parse_commands(&params)?;
    let executions = run_browser_cli_batch(run_command, commands, signal).await?;
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
    signal: Option<CancellationToken>,
) -> Result<Vec<BrowserCliCommandExecution>, HandlerError> {
    let mut results = Vec::new();
    for (index, args) in commands.into_iter().enumerate() {
        let result = wait_for_abort(
            run_command(BROWSER_CLI_EXECUTABLE.to_owned(), args.clone(), signal.clone()),
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

async fn wait_for_abort<T>(
    future: BoxFuture<'static, Result<T, HandlerError>>,
    signal: Option<CancellationToken>,
) -> Result<T, HandlerError> {
    let Some(token) = signal else {
        return future.await;
    };
    if token.is_cancelled() {
        return Err(create_abort_error());
    }
    tokio::select! {
        result = future => result,
        () = token.cancelled() => Err(create_abort_error()),
    }
}

async fn run_browser_cli_command(
    executable: String,
    args: Vec<JsString>,
    signal: Option<CancellationToken>,
) -> Result<BrowserCliCommandResult, HandlerError> {
    if signal.as_ref().is_some_and(CancellationToken::is_cancelled) {
        return Err(create_abort_error());
    }
    let mut child = tokio::process::Command::new(&executable)
        .args(args.iter().map(encode_node_utf8))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| HandlerError::new(spawn_error_message(&executable, &error)))?;

    let wait = crate::utils::child_process::wait_for_child_process(&mut child);
    let output = match signal {
        Some(token) => {
            tokio::select! {
                () = token.cancelled() => {
                    let _ = child.kill().await;
                    return Err(create_abort_error());
                }
                output = wait => output,
            }
        }
        None => wait.await,
    }
    .map_err(|error| HandlerError::new(error.to_string()))?;

    Ok(BrowserCliCommandResult {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        exit_code: output.status,
    })
}

fn create_abort_error() -> HandlerError {
    HandlerError::new("browser-cli command aborted")
}

fn format_browser_cli_batch_output(commands: &[BrowserCliCommandExecution]) -> String {
    commands
        .iter()
        .flat_map(|command| [command.stdout.as_str(), command.stderr.as_str()])
        .filter(|value| !value.is_empty())
        .collect()
}

fn format_browser_cli_failure(index: usize, result: &BrowserCliCommandExecution) -> String {
    let output = [result.stderr.trim(), result.stdout.trim()]
        .into_iter()
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    let detail = if output.is_empty() {
        String::new()
    } else {
        format!(": {output}")
    };
    let exit_code = result
        .exit_code
        .map(|code| code.to_string())
        .unwrap_or_else(|| "unknown".to_owned());
    format!("browser-cli command {} exited with code {exit_code}{detail}", index + 1)
}

fn spawn_error_message(command: &str, error: &std::io::Error) -> String {
    node_spawn_error_message("spawn", command, error)
}
